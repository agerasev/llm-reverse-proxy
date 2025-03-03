use std::{convert::Infallible, pin::Pin, sync::Arc};

use anyhow::Error;
use http::{Request, Response};
use http_body_util::{BodyExt, Empty, combinators::BoxBody};
use hyper::body::{Bytes, Incoming};

pub type Outgoing = BoxBody<Bytes, Error>;

/// TODO: Use tower::Service or hyper::Service
pub trait Service: Send + Sync {
    fn call(
        &self,
        req: Request<Incoming>,
    ) -> impl Future<Output = Result<Response<Outgoing>, Error>> + Send + '_;

    fn call_arc(
        self: Arc<Self>,
        req: Request<Incoming>,
    ) -> impl Future<Output = Result<Response<Outgoing>, Error>> + Send
    where
        Self: 'static,
    {
        async move { self.call(req).await }
    }

    fn into_dyn(self) -> Arc<dyn ServiceDyn>
    where
        Self: Sized + 'static,
    {
        Arc::new(self)
    }
}

pub trait ServiceDyn: Send + Sync {
    fn call_dyn(
        &self,
        req: Request<Incoming>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Outgoing>, Error>> + Send + '_>>;
}

impl<S: Service> ServiceDyn for S {
    fn call_dyn(
        &self,
        req: Request<Incoming>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Outgoing>, Error>> + Send + '_>> {
        Box::pin(self.call(req))
    }
}

impl Service for dyn ServiceDyn {
    fn call(
        &self,
        req: Request<Incoming>,
    ) -> impl Future<Output = Result<Response<Outgoing>, Error>> + Send + '_ {
        self.call_dyn(req)
    }
}

pub struct Router {
    routes: Vec<(String, Arc<dyn ServiceDyn>)>,
    default: Arc<dyn ServiceDyn>,
}

impl Router {
    pub fn new<S: Service + 'static>(default: S) -> Self {
        Self {
            routes: Vec::new(),
            default: Arc::new(default),
        }
    }
    pub fn push<S: Service + 'static>(mut self, prefix: &str, service: S) -> Self {
        self.routes.push((prefix.to_string(), Arc::new(service)));
        self
    }
}

impl Service for Router {
    async fn call(&self, req: Request<Incoming>) -> Result<Response<Outgoing>, Error> {
        for (prefix, service) in &self.routes {
            if req.uri().path().starts_with(prefix) {
                return service.call(req).await;
            }
        }
        self.default.call(req).await
    }
}

#[derive(Clone, Default, Debug)]
pub struct Nothing;

impl Service for Nothing {
    async fn call(&self, _req: Request<Incoming>) -> Result<Response<Outgoing>, Error> {
        Ok(Response::builder().status(404).body(
            Empty::new()
                .map_err(|_: Infallible| -> Error { unreachable!() })
                .boxed(),
        )?)
    }
}

impl<S: Service> Service for Option<S> {
    async fn call(&self, req: Request<Incoming>) -> Result<Response<Outgoing>, Error> {
        match self {
            Some(service) => service.call(req).await,
            None => Nothing.call(req).await,
        }
    }
}
