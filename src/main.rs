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
        Ok::<_, Infallible>(svc)
    });

    // Then bind and serve...
    let server = Server::bind(&addr).serve(make_service);

    // And run forever...
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

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
        log::debug!("received request {} {}", req.method(), req.uri());
        ready(Ok(Response::new(Body::from("Hello World"))))
    }
}
