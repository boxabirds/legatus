//! TC-03, TC-13 (idle close), TC-28 and TC-29 of story 162: the stubs as processes on real sockets.
use bytes::Bytes;
use http::Method;
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use legatus_testkit::faults::idle::{assert_fresh_connection_after_idle, reuse_after_idle, IdleError};
use legatus_testkit::stubs::run::{run_in_memory, run_process};
use legatus_testkit::stubs::scenario::*;
use legatus_testkit::stubs::start_process;
use std::time::Duration;

const SCENARIO: &str = "
scenario both
stub llama kind=multi_slot_llama slots=2 load_time_ms=0
stub mlx kind=batching_mlx
fault llama action=status:503:30:busy limit=count:1
step 0 llama /v1/chat/completions 26
step 50 llama /v1/chat/completions 26
step 100 mlx /v1/chat/completions 26
step 150 mlx /props 0
";

#[tokio::test]
async fn tc03_the_same_scenario_in_process_mode_gives_the_same_statuses_and_bytes() {
    let scenario = parse_scenario(SCENARIO).unwrap();
    let memory = run_in_memory(&scenario).await;
    let process = run_process(&scenario).await;
    let digest = |r: &[legatus_testkit::stubs::run::StepResult]| r.iter().map(|x| (x.stub.clone(), x.status, x.body.clone())).collect::<Vec<_>>();
    assert_eq!(digest(&memory), digest(&process));
    assert_eq!(memory.iter().map(|r| r.status).collect::<Vec<_>>(), vec![503, 200, 200, 404]);
}

fn spec(kind: StubKind) -> StubSpec {
    let mut s = StubSpec::new("s", kind);
    s.speed.load_time_ms = 0;
    s
}

fn rule(action: FaultAction, limit: Limit) -> FaultRule {
    FaultRule { stub: "s".into(), when: Match::default(), action, limit }
}

async fn call(addr: std::net::SocketAddr, tokens: u32) -> Result<(u16, Vec<u8>), hyper_util::client::legacy::Error> {
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
    let body = format!("{{\"messages\":[],\"prompt_tokens\":{tokens},\"output_tokens\":4}}");
    let request = http::Request::builder().method(Method::POST).uri(format!("http://{addr}/v1/chat/completions")).body(Full::new(Bytes::from(body))).unwrap();
    let reply = client.request(request).await?;
    let status = reply.status().as_u16();
    match reply.into_body().collect().await {
        Ok(c) => Ok((status, c.to_bytes().to_vec())),
        Err(_) => Ok((status, Vec::new())),
    }
}

#[tokio::test]
async fn tc28_refuse_reset_hang_and_drop_over_real_sockets() {
    let refused = start_process(&spec(StubKind::MultiSlotLlama), &[rule(FaultAction::Refuse, Limit::Always)]).await;
    let err = call(refused.addr, 26).await.err().expect("connect error");
    assert!(err.is_connect(), "{err:?}");

    let reset = start_process(&spec(StubKind::MultiSlotLlama), &[rule(FaultAction::Reset { once: false }, Limit::Always)]).await;
    let err = call(reset.addr, 26).await.err().expect("the connection is closed without a response");
    assert!(!err.is_connect(), "reached the stub, then reset: {err:?}");

    let hang = start_process(&spec(StubKind::MultiSlotLlama), &[rule(FaultAction::Hang, Limit::Always)]).await;
    assert!(tokio::time::timeout(Duration::from_millis(700), call(hang.addr, 26)).await.is_err(), "no byte arrives");

    let drop_after = start_process(&spec(StubKind::MultiSlotLlama), &[rule(FaultAction::DropAfterChunks { n: 2, clean_end: false }, Limit::Always)]).await;
    let (status, body) = call(drop_after.addr, 26).await.unwrap();
    assert_eq!(status, 200);
    assert!(body.is_empty() || !String::from_utf8_lossy(&body).contains("[DONE]"), "early end of file, no end marker");
}

#[tokio::test]
async fn tc13_a_connection_idle_past_idle_close_has_been_closed_by_the_stub() {
    let mut s = spec(StubKind::MultiSlotLlama);
    s.idle_close_s = 2;
    let stub = start_process(&s, &[]).await;
    assert_eq!(reuse_after_idle(&stub, 1).await, Ok(()), "idle 1 s of 2 s: still open");
    assert_eq!(reuse_after_idle(&stub, 3).await, Err(IdleError::Reused), "idle 3 s of 2 s: closed by the stub");
}

#[tokio::test]
async fn tc29_stub_half_of_the_idle_check_fresh_connection_at_4s_and_reuse_fails_at_6s() {
    let stub = start_process(&spec(StubKind::MultiSlotLlama), &[]).await;
    assert_eq!(assert_fresh_connection_after_idle(&stub, 4).await, Ok(()), "a client idle 4 s opens a new connection and is served");
    assert_eq!(reuse_after_idle(&stub, 6).await, Err(IdleError::Reused), "reusing a connection idle 6 s fails: the stub closed it at 5 s");
}

#[tokio::test]
async fn tc28_a_reset_fault_is_a_tcp_reset_not_a_clean_close() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let stub = start_process(&spec(StubKind::MultiSlotLlama), &[rule(FaultAction::Reset { once: false }, Limit::Always)]).await;
    let mut socket = tokio::net::TcpStream::connect(stub.addr).await.unwrap();
    let body = "{\"messages\":[],\"prompt_tokens\":26,\"output_tokens\":4}";
    let request = format!("POST /v1/chat/completions HTTP/1.1\r\nHost: stub\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    socket.write_all(request.as_bytes()).await.unwrap();
    let mut buf = [0u8; 64];
    let read = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buf)).await.expect("the stub answers or closes");
    let err = read.err().expect("a reset is an error, a clean close would read zero bytes");
    assert_eq!(err.kind(), std::io::ErrorKind::ConnectionReset, "{err:?}");
}
