//! `legatus-stub`: one stub engine with its fault rules, served over HTTP/1 on a real socket.
//! Reads a scenario (one stub) from stdin, prints `addr=<ip:port>` and serves until killed.
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use legatus_proxy::upstream::transport::{UpstreamRequest, UpstreamTransport};
use legatus_testkit::stubs::scenario::parse_scenario;
use legatus_testkit::stubs::start_in_memory;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

async fn handle(transport: Arc<dyn UpstreamTransport>, reset: Arc<AtomicBool>, req: Request<Incoming>) -> Result<Response<UnsyncBoxBody<Bytes, BoxErr>>, BoxErr> {
    let (parts, body) = req.into_parts();
    let bytes = body.collect().await?.to_bytes();
    let uri = format!("http://stub{}", parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("/")).parse()?;
    let upstream = UpstreamRequest { method: parts.method, uri, headers: parts.headers, body: bytes };
    // A connect or reset fault closes the connection without a response; a reset
    // marks the connection so that it is closed with linger zero (a TCP RST).
    let reply = transport.send(upstream).await.map_err(|e| -> BoxErr {
        reset.store(true, Ordering::SeqCst);
        Box::new(e)
    })?;
    let flag = reset.clone();
    let frames = reply.body.map(move |item| match item {
        Ok(b) => Ok(Frame::data(b)),
        Err(e) => {
            flag.store(true, Ordering::SeqCst);
            Err(Box::new(e) as BoxErr)
        }
    });
    let mut response = Response::new(BodyExt::boxed_unsync(StreamBody::new(frames)));
    *response.status_mut() = reply.status;
    *response.headers_mut() = reply.headers;
    Ok(response)
}

#[tokio::main]
async fn main() {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).expect("read the scenario from stdin");
    let scenario = parse_scenario(&text).expect("valid scenario");
    let spec = scenario.stubs.first().expect("one stub").clone();
    let transport = start_in_memory(&spec, &scenario.faults);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    println!("addr={}", listener.local_addr().expect("local addr"));
    let idle = Duration::from_secs(u64::from(spec.idle_close_s));
    loop {
        let Ok((socket, _)) = listener.accept().await else { continue };
        let transport = transport.clone();
        // A duplicate handle shares the socket, so linger zero can be set on it after a
        // reset fault and takes effect when the last handle closes.
        let Ok(std_socket) = socket.into_std() else { continue };
        let Ok(dup) = std_socket.try_clone() else { continue };
        let Ok(socket) = tokio::net::TcpStream::from_std(std_socket) else { continue };
        let Ok(dup) = tokio::net::TcpStream::from_std(dup) else { continue };
        tokio::spawn(async move {
            let reset = Arc::new(AtomicBool::new(false));
            let flag = reset.clone();
            let service = hyper::service::service_fn(move |req| handle(transport.clone(), flag.clone(), req));
            let _ = hyper::server::conn::http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(idle)
                .serve_connection(TokioIo::new(socket), service)
                .await;
            if reset.load(Ordering::SeqCst) {
                #[allow(deprecated)]
                let _ = dup.set_linger(Some(Duration::ZERO));
            }
            drop(dup);
        });
    }
}
