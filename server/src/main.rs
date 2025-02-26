use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use http::HeaderValue;
use hyper::{
    Error, Request, Response, Uri, body::Incoming, client::conn::http1::SendRequest,
    server::conn::http1, service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::JoinHandle,
};

struct Connection {
    sender: SendRequest<Incoming>,
    task: JoinHandle<()>,
}

impl Connection {
    async fn connect(url: &Uri) -> Self {
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
        let task = tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });

        Self { sender, task }
    }

    async fn send(&mut self, req: Request<Incoming>) -> Result<Response<Incoming>, Error> {
        self.sender.send_request(req).await
    }

    fn is_closed(&self) -> bool {
        self.task.is_finished()
    }
}

#[derive(Clone)]
struct Client {
    url: Uri,
    connection: Arc<Mutex<Option<Connection>>>,
}

impl Client {
    async fn new(url: Uri) -> Self {
        Self {
            url,
            connection: Arc::new(Mutex::new(None)),
        }
    }

    async fn send(&self, req: Request<Incoming>) -> Result<Response<Incoming>, Error> {
        let mut guard = self.connection.lock().await;
        let conn = loop {
            let conn = match guard.take() {
                Some(conn) => conn,
                None => Connection::connect(&self.url).await,
            };
            if conn.is_closed() {
                continue;
            }
            break guard.insert(conn);
        };
        conn.send(req).await
    }

    async fn forward(self, mut req: Request<Incoming>) -> Result<Response<Incoming>, Infallible> {
        println!("request: {req:?}");

        let authority = self.url.authority().cloned().unwrap();
        req.headers_mut().insert(
            hyper::header::HOST,
            HeaderValue::from_str(authority.as_str()).unwrap(),
        );

        // Await the response...
        let res = self.send(req).await.unwrap();

        println!("Response status: {}", res.status());

        Ok(res)
    }
}

async fn serve(addr: SocketAddr, url: Uri) -> ! {
    // We create a TcpListener and bind it to addr
    let listener = TcpListener::bind(addr).await.unwrap();

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await.unwrap();

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        let client = Client::new(url.clone()).await;

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(io, service_fn(move |req| client.clone().forward(req)))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

#[tokio::main]
async fn main() -> ! {
    let server_addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    let client_addr = "http://127.0.0.1:8080".parse::<Uri>().unwrap();

    serve(server_addr, client_addr).await
}
