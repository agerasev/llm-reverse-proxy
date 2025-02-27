use anyhow::Error;
use http::header;
use http_body_util::{BodyExt, BodyStream, Full, StreamBody};
use hyper::{
    Request, Response, Uri,
    body::{Bytes, Incoming},
    client::conn::http1::SendRequest,
};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};

use crate::{Outgoing, Service};

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
}

impl ReverseProxy {
    pub fn new(url: Uri) -> Self {
        Self {
            url,
            connection: Mutex::new(None),
        }
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
        let body = req.into_body().collect().await?.to_bytes();
        log::trace!("Request data: {}", String::from_utf8_lossy(&body));

        let req = Request::builder()
            .method(http::Method::POST)
            .uri(&uri)
            .header(header::HOST, host.as_str())
            .body(Full::new(body))?;

        // Await the response...
        let res = self.send(req).await?;
        log::trace!("Response: {res:?}");

        let body = StreamBody::new(BodyStream::new(
            res.into_body().map_err(Error::new).map_frame(|f| {
                if let Some(bytes) = f.data_ref() {
                    log::trace!("Response data frame: {}", String::from_utf8_lossy(bytes));
                }
                f
            }),
        ))
        .boxed();
        let res = Response::builder().body(body)?;

        Ok(res)
    }
}
