use std::path::{Path, PathBuf};

use anyhow::Error;
use http::{HeaderName, Request, Response};
use http_body_util::{BodyExt, Empty, StreamBody};
use hyper::body::{Frame, Incoming};
use tokio::fs::File;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use crate::{Outgoing, Service};

fn mime(path: &Path) -> &str {
    match path.extension().and_then(|s| s.to_str()) {
        Some("html") => "text/html",
        Some("js") => "text/javascript",
        _ => "text/plain",
    }
}

#[derive(Clone, Debug)]
pub struct FileServer {
    base_path: PathBuf,
}

impl FileServer {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_owned(),
        }
    }
}

impl Service for FileServer {
    async fn call(&self, req: Request<Incoming>) -> Result<Response<Outgoing>, Error> {
        let path = match (|| {
            let mut path = req.uri().path();
            if path.contains("/.") || path.contains("..") {
                return None;
            }
            if !path.is_empty() {
                path = path.strip_prefix("/")?;
            }

            let mut path = self.base_path.join(path);
            if path.is_dir() {
                path = path.join("index.html");
            }
            if !path.is_file() {
                return None;
            }
            Some(path)
        })() {
            Some(path) => path,
            None => {
                log::debug!("File {:?} not found", req.uri().path());
                return Ok(Response::builder()
                    .status(404)
                    .body(Empty::new().map_err(Error::new).boxed())?);
            }
        };

        log::debug!("Reading file {:?}", path);
        let file = File::open(&path).await?;
        let stream = ReaderStream::new(file).map(|r| match r {
            Ok(x) => Ok(Frame::data(x)),
            Err(e) => Err(Error::from(e)),
        });

        let res = Response::builder()
            .status(200)
            .header(HeaderName::from_static("content-type"), mime(&path))
            .body(StreamBody::new(stream).boxed())?;

        Ok(res)
    }
}
