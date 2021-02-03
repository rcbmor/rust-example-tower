#![allow(warnings)]
use std::convert::Infallible;
use std::net::SocketAddr;
use std::task::{Context, Poll};
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use futures::future::{ready, Ready};
use tower::Service;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Construct our SocketAddr to listen on...
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // And a MakeService to handle each connection...
    let make_service = make_service_fn(|_conn| async {
        let svc = HelloWorld;
        let svc = Logging::new(svc);
        let svc = Logging::new(svc);
        let svc = Logging::new(svc);
        Ok::<_, Infallible>(svc)
    });

    // Then bind and serve...
    let server = Server::bind(&addr).serve(make_service);

    // And run forever...
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

#[derive(Clone, Copy)]
struct HelloWorld;

impl Service<Request<Body>> for HelloWorld {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        //todo!()
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        ready(Ok(Response::new(Body::from("Hello World"))))
    }
}

#[derive(Clone, Copy)]
struct Logging<S> {
    inner: S,
}

impl<S> Logging<S> {
    fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S, B> Service<Request<B>> for Logging<S>
where
    S: Service<Request<B>> + Clone + Send + 'static,
    B: 'static + Send,
    S::Future: 'static + Send
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let method = req.method().clone();
            let uri = req.uri().path().to_string();
            log::debug!("processing request {} {}", method, uri);
            let response = inner.call(req).await;
            log::debug!("finished processing request {} {}", method, uri);
            response
        })
    }
}