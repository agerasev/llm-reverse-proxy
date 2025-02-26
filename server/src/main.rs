use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use http::HeaderValue;
use hyper::{
    Request, Response, Uri, body::Incoming, client::conn::http1::SendRequest, server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

#[derive(Clone, Debug)]
struct Client {
    url: Uri,
    sender: Arc<Mutex<SendRequest<Incoming>>>,
}

async fn run_client(url: Uri) -> Client {
    // Get the host and the port
    let host = url.host().expect("uri has no host");
    let port = url.port_u16().unwrap_or(80);

    let address = format!("{}:{}", host, port);

    // Open a TCP connection to the remote host
    let stream = TcpStream::connect(address).await.unwrap();

    // Use an adapter to access something implementing `tokio::io` traits as if they implement
    // `hyper::rt` IO traits.
    let io = TokioIo::new(stream);

    // Create the Hyper client
    let (sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();

    // Spawn a task to poll the connection, driving the HTTP state
    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            println!("Connection failed: {:?}", err);
        }
    });

    Client {
        url,
        sender: Arc::new(Mutex::new(sender)),
    }
}

async fn hello(
    mut req: Request<Incoming>,
    Client { url, sender }: Client,
) -> Result<Response<Incoming>, Infallible> {
    println!("request: {req:?}");

    /*
    let (parts, body) = req.into_parts();
    let bytes: Bytes = body.collect().await.unwrap().to_bytes();
    let mut req = Request::from_parts(parts, Full::new(bytes));
    */

    let authority = url.authority().cloned().unwrap();
    req.headers_mut().insert(
        hyper::header::HOST,
        HeaderValue::from_str(authority.as_str()).unwrap(),
    );

    // Await the response...
    let res = sender.lock().await.send_request(req).await.unwrap();

    println!("Response status: {}", res.status());

    Ok(res)
}

async fn serve(addr: SocketAddr, client: Client) -> ! {
    // We create a TcpListener and bind it to addr
    let listener = TcpListener::bind(addr).await.unwrap();

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await.unwrap();

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        let client = client.clone();

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(io, service_fn(move |req| hello(req, client.clone())))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

#[tokio::main]
async fn main() -> ! {
    let client = run_client("http://127.0.0.1:8080".parse::<Uri>().unwrap()).await;

    serve(SocketAddr::from(([0, 0, 0, 0], 4000)), client).await
}
