mod files;
mod proxy;

use std::{net::SocketAddr, sync::Arc};

use anyhow::Error;
use files::FileServer;
use http::{Request, Response};
use http_body_util::combinators::BoxBody;
use hyper::{
    Uri,
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use self::proxy::ReverseProxy;

pub type Outgoing = BoxBody<Bytes, Error>;

/// TODO: Use hyper::Service
pub trait Service: Send + Sync {
    fn call(
        &self,
        req: Request<Incoming>,
    ) -> impl Future<Output = Result<Response<Outgoing>, Error>> + Send;

    fn call_arc(
        self: Arc<Self>,
        req: Request<Incoming>,
    ) -> impl Future<Output = Result<Response<Outgoing>, Error>> + Send
    where
        Self: 'static,
    {
        async move { self.call(req).await }
    }
}

struct Handler {
    proxy: ReverseProxy,
    files: FileServer,
}

impl Service for Handler {
    async fn call(&self, req: Request<Incoming>) -> Result<Response<Outgoing>, Error> {
        if req.uri().path() == "/chat/completions" {
            self.proxy.call(req).await
        } else {
            self.files.call(req).await
        }
    }
}

async fn serve<S, F>(addr: SocketAddr, mut make_service: F) -> Result<(), Error>
where
    S: Service + 'static,
    F: AsyncFnMut() -> Result<S, Error>,
{
    // We create a TcpListener and bind it to addr
    let listener = TcpListener::bind(addr).await?;
    log::info!("Listening for incoming connections at {addr}");

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, addr) = listener.accept().await?;
        log::debug!("Incoming connection from {addr} established");

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        let service = Arc::new(make_service().await?);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(
                    io,
                    service_fn({
                        let service = service.clone();
                        move |req| service.clone().call_arc(req)
                    }),
                )
                .await
            {
                if err.is_incomplete_message() {
                    log::warn!("Incoming connection from {addr} unexpected EOF");
                } else {
                    log::error!("Incoming connection from {addr} failed: {err:?}");
                }
            } else {
                log::debug!("Incoming connection closed: {addr}");
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::builder().init();

    let server_addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    let proxy_url = "http://127.0.0.1:8080".parse::<Uri>()?;
    let static_path = "../client-example";

    serve(server_addr, async move || {
        Ok(Handler {
            proxy: ReverseProxy::new(proxy_url.clone()),
            files: FileServer::new(static_path),
        })
    })
    .await
}
