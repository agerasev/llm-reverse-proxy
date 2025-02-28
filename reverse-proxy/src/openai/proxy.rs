use std::convert::Infallible;

use anyhow::Error;
use http::header;
use http_body_util::{BodyExt, BodyStream, Full, StreamBody};
use hyper::{
    Request, Response, Uri,
    body::{Bytes, Frame, Incoming},
    client::conn::http1::SendRequest,
};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_stream::StreamExt;

use crate::{
    Outgoing, Service,
    openai::api::{self, Message},
    sse::{Event, EventReader},
};

struct Connection {
    sender: SendRequest<Full<Bytes>>,
    task: JoinHandle<()>,
}

impl Connection {
    async fn connect(url: &Uri) -> Result<Self, Error> {
        // Get the host and the port
        let host = url.host().expect("URL has no host");
        let port = url.port_u16().unwrap_or(80);

        let addr = format!("{}:{}", host, port);

        // Open a TCP connection to the remote host
        let stream = TcpStream::connect(&addr).await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Create the Hyper client
        let (sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        log::debug!("Outgoing connection to {addr} established");

        // Spawn a task to poll the connection, driving the HTTP state
        let task = tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                log::error!("Outgoing connection to {addr} failed: {:?}", err);
            } else {
                log::debug!("Outgoing connection to {addr} closed");
            }
        });

        Ok(Self { sender, task })
    }

    async fn send(&mut self, req: Request<Full<Bytes>>) -> Result<Response<Incoming>, Error> {
        Ok(self.sender.send_request(req).await?)
    }

    fn is_closed(&self) -> bool {
        self.task.is_finished()
    }
}

pub struct ReverseProxy {
    url: Uri,
    connection: Mutex<Option<Connection>>,
    system_prompt: Option<String>,
}

impl ReverseProxy {
    pub fn new(url: Uri) -> Self {
        Self {
            url,
            connection: Mutex::new(None),
            system_prompt: None,
        }
    }

    pub fn with_system_prompt(mut self, prompt: Option<impl Into<String>>) -> Self {
        self.system_prompt = prompt.map(|s| s.into());
        self
    }

    async fn send(&self, req: Request<Full<Bytes>>) -> Result<Response<Incoming>, Error> {
        let mut guard = self.connection.lock().await;
        let conn = loop {
            let conn = match guard.take() {
                Some(conn) => conn,
                None => Connection::connect(&self.url).await?,
            };
            if conn.is_closed() {
                continue;
            }
            break guard.insert(conn);
        };
        conn.send(req).await
    }
}

impl Service for ReverseProxy {
    async fn call(&self, req: Request<Incoming>) -> Result<Response<Outgoing>, Error> {
        log::trace!("Request: {req:?}");

        let host = self.url.authority().expect("Client URL must be set");
        let uri = req.uri().clone();
        let data = req.into_body().collect().await?.to_bytes();
        log::trace!("Incoming request data: {}", String::from_utf8_lossy(&data));
        let msg: api::Request = serde_json::from_slice(&data)?;
        log::trace!("Incoming request struct: {:?}", msg);

        let mut messages = vec![];
        if let Some(prompt) = &self.system_prompt {
            messages.push(Message {
                role: "system".into(),
                content: prompt.into(),
            });
        }
        messages.extend(msg.messages);
        let streaming = msg.stream.unwrap_or(false);
        let msg = api::Request {
            model: "".into(),
            messages,
            stream: Some(streaming),
        };
        log::trace!("Outgoing request struct: {:?}", msg);
        let data = Bytes::from(serde_json::to_vec(&msg)?);
        log::trace!(
            "Outgoing request data: {:?}",
            String::from_utf8_lossy(&data)
        );

        let req = Request::builder()
            .method(http::Method::POST)
            .uri(&uri)
            .header(header::HOST, host.as_str())
            .body(Full::new(data))?;

        // Await the response...
        let res = self.send(req).await?;
        log::trace!("Response: {res:?}");

        let body = if streaming {
            let mut event_reader = EventReader::default();
            StreamBody::new(BodyStream::new(res.into_body()).map(move |res| {
                let input = match res?.into_data() {
                    Ok(data) => data.to_vec(),
                    Err(frame) => return Ok(frame),
                };
                log::trace!(
                    "Outgoing response data frame: {}",
                    String::from_utf8_lossy(&input)
                );
                let mut output = String::new();

                const DONE: &str = "[DONE]";
                for event in event_reader.next_events(&input)? {
                    let data = match event.data {
                        Some(data) => data,
                        None => continue,
                    };

                    let event = if data == DONE {
                        Event {
                            data: Some(DONE.into()),
                            ..Default::default()
                        }
                    } else {
                        let msg: api::ResponseStreamChunk = serde_json::from_str(&data)?;
                        log::trace!("Outgoing response event struct: {:?}", msg);

                        let msg = api::ResponseStreamChunk {
                            choices: msg.choices,
                        };
                        log::trace!("Incoming response event struct: {:?}", msg);
                        Event {
                            // TODO: Write to output without allocation
                            data: Some(serde_json::to_string(&msg)?.into()),
                            ..Default::default()
                        }
                    };

                    event.write_to(&mut output)?;
                }

                log::trace!("Incoming response data frame: {}", output);
                Ok(Frame::data(Bytes::from(output)))
            }))
            .boxed()
        } else {
            let data = res.into_body().collect().await?.to_bytes();
            log::trace!("Outgoing response data: {}", String::from_utf8_lossy(&data));
            let msg: api::Response = serde_json::from_slice(&data)?;
            log::trace!("Outgoing response struct: {:?}", msg);
            let msg = api::Response {
                choices: msg.choices,
            };
            log::trace!("Incoming response struct: {:?}", msg);
            let data = serde_json::to_string(&msg)?;
            log::trace!("Incoming response data: {}", data);
            Full::new(Bytes::from(data))
                .map_err(|_: Infallible| unreachable!())
                .boxed()
        };
        let res = Response::builder()
            .header(
                header::CONTENT_TYPE,
                if streaming {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .body(body)?;

        Ok(res)
    }
}
