use std::{convert::Infallible, pin::Pin};

use anyhow::{Error, anyhow, bail};
use http::{header, uri::PathAndQuery};
use http_body_util::{BodyExt, BodyStream, Full, StreamBody};
use hyper::{
    Request, Response, Uri,
    body::{Bytes, Frame, Incoming},
    client::conn::http1::SendRequest,
};
use hyper_util::rt::TokioIo;
use openssl::ssl::{Ssl, SslContext, SslMethod};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    sync::Mutex,
    task::JoinHandle,
};
use tokio_openssl::SslStream;
use tokio_stream::StreamExt;

use crate::{
    Outgoing, Service,
    http_util::{
        self,
        sse::{Event, EventReader},
    },
    openai::api::{self, Message},
};

fn url_to_host_and_port(url: &Uri) -> Result<(&str, u16), Error> {
    // Get the host and the port
    let host = url
        .host()
        .ok_or_else(|| anyhow!("URL has no host: {url}"))?;
    let port = url.port_u16();
    match url
        .scheme_str()
        .ok_or_else(|| anyhow!("Server address has no scheme: {url}"))?
    {
        "http" => Ok((host, port.unwrap_or(80))),
        "https" => Ok((host, port.unwrap_or(443))),
        scheme => Err(anyhow!("Unsupported scheme: {scheme}")),
    }
}

struct Connection {
    sender: SendRequest<Full<Bytes>>,
    task: JoinHandle<()>,
}

impl Connection {
    async fn connect(url: &Uri) -> Result<Self, Error> {
        // Open a TCP connection to the remote host
        let stream = TcpStream::connect(url_to_host_and_port(url)?).await?;
        Self::connect_raw_socket(stream, url).await
    }

    async fn connect_through_proxy(proxy: &Uri, dst: &Uri) -> Result<Self, Error> {
        let mut stream = TcpStream::connect(url_to_host_and_port(proxy)?).await?;
        http_util::proxy::handshake(&mut stream, url_to_host_and_port(dst)?).await?;
        Self::connect_raw_socket(stream, dst).await
    }

    async fn connect_raw_socket(stream: TcpStream, url: &Uri) -> Result<Self, Error> {
        let addr = url_to_host_and_port(url)?;
        match url.scheme_str().expect("Server address has no scheme") {
            "http" => Self::connect_stream(stream, addr).await,
            "https" => {
                let ssl_context = SslContext::builder(SslMethod::tls())?.build();
                let mut ssl = Ssl::new(&ssl_context)?;
                ssl.set_hostname(addr.0)?;
                let mut ssl_stream = SslStream::new(ssl, stream)?;
                Pin::new(&mut ssl_stream).connect().await?;

                Self::connect_stream(ssl_stream, addr).await
            }
            scheme => bail!("Unsupported scheme: {scheme}"),
        }
    }

    async fn connect_stream<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        stream: S,
        addr: (&str, u16),
    ) -> Result<Self, Error> {
        let addr = format!("{}:{}", addr.0, addr.1);

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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub enum ServerKind {
    #[default]
    LlamaCpp,
    OpenAi,
}

pub struct ReverseProxy {
    url: Uri,
    proxy: Option<Uri>,

    model: String,
    kind: ServerKind,

    api_key: Option<String>,
    system_prompt: Option<String>,

    connection: Mutex<Option<Connection>>,
}

impl ReverseProxy {
    pub fn new(url: Uri) -> Self {
        Self {
            url,
            proxy: None,
            model: String::new(),
            kind: ServerKind::default(),
            api_key: None,
            system_prompt: None,
            connection: Mutex::new(None),
        }
    }

    pub fn model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn kind(mut self, kind: ServerKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn proxy(mut self, proxy: Option<Uri>) -> Self {
        self.proxy = proxy;
        self
    }

    pub fn api_key(mut self, api_key: Option<String>) -> Self {
        self.api_key = api_key;
        self
    }

    pub fn system_prompt(mut self, prompt: Option<impl Into<String>>) -> Self {
        self.system_prompt = prompt.map(|s| s.into());
        self
    }

    async fn send(&self, req: Request<Full<Bytes>>) -> Result<Response<Incoming>, Error> {
        let mut guard = self.connection.lock().await;
        let conn = loop {
            let conn = match guard.take() {
                Some(conn) => conn,
                None => match &self.proxy {
                    None => Connection::connect(&self.url).await?,
                    Some(proxy) => Connection::connect_through_proxy(proxy, &self.url).await?,
                },
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
        match self.forward(req).await {
            Ok(res) => Ok(res),
            Err(err) => {
                log::error!("Reverse-proxy forwarding error:\n{err}");
                Ok(Response::builder()
                    .status(500)
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(
                        Full::new(Bytes::from(err.to_string()))
                            .map_err(|_: Infallible| unreachable!())
                            .boxed(),
                    )?)
            }
        }
    }
}

struct RequestParams {
    streaming: bool,
}

impl ReverseProxy {
    async fn forward(&self, req: Request<Incoming>) -> Result<Response<Outgoing>, Error> {
        log::trace!("Incoming: {req:?}");

        let (req, params) = self.convert_request(req).await?;
        log::trace!("Outgoing: {req:?}");

        // Await the response...
        let res = self.send(req).await?;
        log::trace!("Outgoing: {res:?}");

        let res = self.convert_response(res, params).await?;
        log::trace!("Incoming: {res:?}");

        Ok(res)
    }

    async fn convert_request(
        &self,
        req: Request<Incoming>,
    ) -> Result<(Request<Full<Bytes>>, RequestParams), Error> {
        let host = self.url.authority().expect("Client URL must be set");
        let uri = req.uri().clone();
        if uri.path() != "/chat/completions" {
            bail!("Path must be '/chat/completions' but got {:?}", uri.path());
        }
        let data = req.into_body().collect().await?.to_bytes();
        log::trace!("Incoming request data: {}", String::from_utf8_lossy(&data));
        let msg: api::Request = serde_json::from_slice(&data)?;

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
            model: self.model.as_str().into(),
            messages,
            stream: Some(streaming),
        };

        let data = Bytes::from(serde_json::to_vec(&msg)?);
        log::trace!(
            "Outgoing request data: {:?}",
            String::from_utf8_lossy(&data)
        );

        let uri = {
            let mut parts = uri.into_parts();
            parts.path_and_query = Some(PathAndQuery::from_static(match self.kind {
                ServerKind::LlamaCpp => "/chat/completions",
                ServerKind::OpenAi { .. } => "/v1/chat/completions",
            }));
            Uri::from_parts(parts)?
        };
        let mut builder = Request::builder()
            .method(http::Method::POST)
            .uri(&uri)
            .header(header::HOST, host.as_str())
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(api_key) = &self.api_key {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {api_key}"));
        }

        Ok((builder.body(Full::new(data))?, RequestParams { streaming }))
    }

    async fn convert_response(
        &self,
        res: Response<Incoming>,
        params: RequestParams,
    ) -> Result<Response<Outgoing>, Error> {
        if !res.status().is_success() {
            bail!("Response status is {}", res.status());
        }

        let body = if params.streaming {
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

                        let mut choices = msg.choices;
                        for choice in choices.iter_mut() {
                            if choice.finish_reason.is_some() && choice.delta.role.is_none() {
                                choice.delta.role = Some("assistant".into());
                            }
                        }
                        let msg = api::ResponseStreamChunk { choices };
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

            let msg = api::Response {
                choices: msg.choices,
            };
            log::trace!("Incoming response struct: {:?}", msg);
            let data = serde_json::to_string(&msg)?;
            Full::new(Bytes::from(data))
                .map_err(|_: Infallible| unreachable!())
                .boxed()
        };

        Ok(Response::builder()
            .header(
                header::CONTENT_TYPE,
                if params.streaming {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .body(body)?)
    }
}
