#![allow(warnings)]

use futures::future::{ready, Ready};
use futures::prelude::*;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use pin_project::pin_project;
use std::convert::Infallible;
use std::fmt::Display;
use std::net::SocketAddr;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use std::{future::Future, pin::Pin};
use tokio::time::Sleep;
use tower::BoxError;
use tower::Layer;
use tower::Service;
use tower::ServiceBuilder;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Construct our SocketAddr to listen on...
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let base_layer = ServiceBuilder::new()
        .layer(LoggingLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(2)))
        .into_inner();

    let limit_layer = ServiceBuilder::new()
        .rate_limit(100, Duration::from_secs(1))
        .concurrency_limit(1000)
        .into_inner();

    // And a MakeService to handle each connection...
    let make_service = make_service_fn(|_conn| async {
        // the order we wrap the services is important!
        let svc = ServiceBuilder::new()
            .layer(base_layer)
            .layer(limit_layer)
            .service(service_fn(handle));

        Ok::<_, Infallible>(svc)
    });

    // Then bind and serve...
    let server = Server::bind(&addr).serve(make_service);

    // And run forever...
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn handle(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(Response::new(Body::from("Hello World async fn")))
}

#[derive(Clone, Copy)]
struct HelloWorld;

impl Service<Request<Body>> for HelloWorld {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        ready(Ok(Response::new(Body::from("Hello World"))))
    }
}

struct LoggingLayer;
impl LoggingLayer {
    fn new() -> Self {
        Self
    }
}
impl<S> Layer<S> for LoggingLayer {
    type Service = Logging<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Logging::new(inner)
    }
}

struct TimeoutLayer {
    timeout: Duration,
}
impl TimeoutLayer {
    fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}
impl<S> Layer<S> for TimeoutLayer {
    type Service = Timeout<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Timeout::new(inner, self.timeout)
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
    S: Service<Request<B>, Response = Response<B>> + Clone + Send + 'static,
    B: 'static + Send,
    S::Future: 'static + Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = LoggingFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        log::trace!("polling the future...");
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        log::debug!("processing request {} {} ", method, path);

        let start = Instant::now();
        LoggingFuture {
            future: self.inner.call(req),
            method,
            path,
            start,
        }
    }
}

#[pin_project]
struct LoggingFuture<F> {
    #[pin]
    future: F,
    method: hyper::Method,
    path: String,
    start: Instant,
}

impl<F, B, E> Future for LoggingFuture<F>
where
    F: Future<Output = Result<Response<B>, E>>,
{
    type Output = Result<Response<B>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let res: Result<Response<B>, E> = match this.future.poll(cx) {
            Poll::Ready(res) => res,
            Poll::Pending => return Poll::Pending,
        };

        let duration = this.start.elapsed();

        let status = if let Ok(res) = &res {
            res.status().as_u16()
        } else {
            500
        };

        log::debug!(
            "finisned processing request {} {}. time={:?}, status={} ",
            this.method,
            this.path,
            duration,
            status,
        );

        Poll::Ready(res)
    }
}

#[derive(Clone, Copy)]
struct Timeout<S> {
    inner: S,
    timeout: Duration,
}

impl<S> Timeout<S> {
    fn new(inner: S, timeout: Duration) -> Self {
        Self { inner, timeout }
    }
}

impl<S, R> Service<R> for Timeout<S>
where
    S: Service<R>,
    S::Error: Into<BoxError> + Send + Sync + 'static,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = TimeoutFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: R) -> Self::Future {
        TimeoutFuture {
            future: self.inner.call(req),
            sleep: tokio::time::sleep(self.timeout),
        }
    }
}

#[pin_project]
struct TimeoutFuture<F> {
    #[pin]
    future: F,
    #[pin]
    sleep: Sleep,
}

impl<F, T, E> Future for TimeoutFuture<F>
where
    F: Future<Output = Result<T, E>>,
    E: Into<BoxError> + Send + Sync + 'static,
{
    type Output = Result<T, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // first the future itself
        match this.future.poll(cx) {
            Poll::Pending => {}
            Poll::Ready(result) => {
                return match result {
                    Ok(res) => Poll::Ready(Ok(res)),
                    Err(err) => Poll::Ready(Err(err.into())),
                }
            }
        }

        // then the sleep timeout
        match this.sleep.poll(cx) {
            Poll::Pending => {}
            Poll::Ready(_) => return Poll::Ready(Err(Box::new(Elapsed))),
        }

        Poll::Pending
    }
}

#[derive(Debug)]
struct Elapsed;
impl Display for Elapsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "timeout elapsed")
    }
}
impl std::error::Error for Elapsed {}
