pub mod files;
pub mod http_util;
pub mod openai;
pub mod service;

pub use self::service::{Outgoing, Router, Service};

use std::sync::Arc;

use anyhow::Error;
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, ToSocketAddrs};

pub async fn serve<A, S, F>(addr: A, mut make_service: F) -> Result<(), Error>
where
    A: ToSocketAddrs,
    S: Service + 'static,
    F: AsyncFnMut() -> Result<S, Error>,
{
    // We create a TcpListener and bind it to addr
    let listener = TcpListener::bind(addr).await?;
    log::info!(
        "Listening for incoming connections at {}",
        listener.local_addr()?
    );

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
