//! TC-23 to TC-27, TC-30, TC-31 of story 162: fault actions in memory, the sink and the clock.
use super::stub_helpers::*;
use futures_util::StreamExt;
use legatus_proxy::obs::log_sink::{DropReason, LogRecord, LogSink, Offer, SystemRecord};
use legatus_proxy::time::{Instant as ProxyInstant, WallClock, WallTime};
use legatus_proxy::upstream::transport::UpstreamError;
use legatus_common::event::SystemEventKind;
use legatus_testkit::faults::clock::apply_clock_jump;
use legatus_testkit::faults::FaultySink;
use legatus_testkit::stubs::scenario::*;
use legatus_testkit::stubs::start_in_memory;
use legatus_testkit::virt::SimWall;
use std::time::Duration;
use tokio::time::Instant;

fn llama() -> StubSpec {
    let mut s = StubSpec::new("s", StubKind::MultiSlotLlama);
    s.speed.load_time_ms = 0;
    s
}

fn rule(action: FaultAction, limit: Limit) -> FaultRule {
    FaultRule { stub: "s".into(), when: Match::default(), action, limit }
}

#[tokio::test(start_paused = true)]
async fn tc23_refuse_inside_its_window_gives_connect_and_before_and_after_it_is_served() {
    let t = start_in_memory(&llama(), &[rule(FaultAction::Refuse, Limit::Window { from_ms: 1000, to_ms: 2000 })]);
    let start = Instant::now();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(t.send(chat(26, 1)).await.is_ok(), "before the window");
    tokio::time::sleep_until(start + Duration::from_millis(1500)).await;
    assert_eq!(t.send(chat(26, 1)).await.err(), Some(UpstreamError::Connect));
    tokio::time::sleep_until(start + Duration::from_millis(2000)).await;
    assert!(t.send(chat(26, 1)).await.is_ok(), "the window is half open: served at 2000");
}

#[tokio::test(start_paused = true)]
async fn tc24_hang_sends_nothing_and_slow_first_byte_arrives_at_the_exact_instant() {
    let t = start_in_memory(&llama(), &[rule(FaultAction::Hang, Limit::Always)]);
    assert!(tokio::time::timeout(Duration::from_secs(600), t.send(chat(26, 1))).await.is_err(), "nothing is ever sent");
    for secs in [5u64, 20, 60, 120] {
        let t = start_in_memory(&llama(), &[rule(FaultAction::SlowFirstByte { ms: secs * 1000 }, Limit::Count(1))]);
        let started = Instant::now();
        let reply = t.send(chat(26, 1)).await.ok().unwrap();
        assert_eq!(started.elapsed().as_millis(), u128::from(secs) * 1000);
        drop(reply);
    }
    // No timeout error exists: the transport error type has exactly two kinds.
    for e in [UpstreamError::Connect, UpstreamError::Reset] {
        let _ = err_kind(e);
    }
}

#[tokio::test(start_paused = true)]
async fn tc25_drop_after_n_chunks_closes_after_exactly_n_and_clean_end_omits_the_marker() {
    let rough = start_in_memory(&llama(), &[rule(FaultAction::DropAfterChunks { n: 2, clean_end: false }, Limit::Always)]);
    let mut stream = rough.send(chat(26, 5)).await.ok().unwrap().body;
    assert!(stream.next().await.unwrap().is_ok());
    assert!(stream.next().await.unwrap().is_ok());
    assert_eq!(stream.next().await.unwrap().err(), Some(UpstreamError::Reset));
    assert!(stream.next().await.is_none());

    let clean = start_in_memory(&llama(), &[rule(FaultAction::DropAfterChunks { n: 2, clean_end: true }, Limit::Always)]);
    let (_, body, instants) = drain(clean.send(chat(26, 5)).await.ok().unwrap(), Instant::now()).await;
    assert_eq!(instants.len(), 2, "exactly n chunks");
    assert!(!String::from_utf8_lossy(&body).contains("[DONE]"), "no end marker");
}

#[tokio::test(start_paused = true)]
async fn tc26_status_script_with_and_without_retry_after_and_empty_ok() {
    let with = start_in_memory(&llama(), &[rule(FaultAction::Status { code: 503, body: "busy".into(), retry_after_s: Some(30) }, Limit::Always)]);
    let reply = with.send(chat(26, 1)).await.ok().unwrap();
    assert_eq!(reply.status.as_u16(), 503);
    assert_eq!(reply.headers.get("retry-after").unwrap(), "30");
    let (_, body, _) = drain(reply, Instant::now()).await;
    assert_eq!(body, b"busy");
    let without = start_in_memory(&llama(), &[rule(FaultAction::Status { code: 529, body: String::new(), retry_after_s: None }, Limit::Always)]);
    let reply = without.send(chat(26, 1)).await.ok().unwrap();
    assert_eq!((reply.status.as_u16(), reply.headers.contains_key("retry-after")), (529, false));
    let empty = start_in_memory(&llama(), &[rule(FaultAction::EmptyOk, Limit::Always)]);
    let (status, body, _) = drain(empty.send(chat(26, 1)).await.ok().unwrap(), Instant::now()).await;
    assert_eq!((status, body.len()), (200, 0));
}

#[tokio::test(start_paused = true)]
async fn tc27_reset_once_then_served_reset_always_never_served_and_idle_close_is_a_spec_field() {
    let once = start_in_memory(&llama(), &[rule(FaultAction::Reset { once: true }, Limit::Always)]);
    assert_eq!(once.send(chat(26, 1)).await.err(), Some(UpstreamError::Reset));
    assert!(once.send(chat(26, 1)).await.is_ok(), "served after the single reset");
    let always = start_in_memory(&llama(), &[rule(FaultAction::Reset { once: false }, Limit::Always)]);
    for _ in 0..3 {
        assert_eq!(always.send(chat(26, 1)).await.err(), Some(UpstreamError::Reset));
    }
    assert_eq!(StubSpec::new("x", StubKind::SerialOllama).idle_close_s, 5, "default idle close is 5 s (spike s3)");
    let s = parse_scenario("stub a kind=multi_slot_llama idle_close_s=2").unwrap();
    assert_eq!(s.stubs[0].idle_close_s, 2);
}

fn record() -> LogRecord {
    LogRecord::System(SystemRecord::new(SystemEventKind::Warning))
}

#[tokio::test(start_paused = true)]
async fn tc30_sink_faults_change_status_delay_or_drop_count_and_no_fault_changes_nothing() {
    let sink = FaultySink::new();
    assert_eq!(sink.offer(record()), Offer::Queued);
    assert!(!sink.disk_low());
    assert_eq!((sink.dropped(), sink.accepted()), (0, 1), "no fault: no change");

    sink.set_fault(Some(SinkFault::DiskLow));
    assert!(sink.disk_low());
    sink.set_fault(None);
    assert!(!sink.disk_low());

    sink.set_fault(Some(SinkFault::Stall { ms: 500 }));
    assert_eq!(sink.offer(record()), Offer::Dropped(DropReason::Paused));
    tokio::time::sleep(Duration::from_millis(501)).await;
    assert_eq!(sink.offer(record()), Offer::Queued, "the stall is over");

    sink.set_fault(Some(SinkFault::Fail));
    assert_eq!(sink.offer(record()), Offer::Dropped(DropReason::BuildError));
    assert_eq!(sink.dropped(), 2);
}

#[tokio::test(start_paused = true)]
async fn tc31_clock_jump_back_one_hour_and_forward_leaves_the_monotonic_instant_unchanged() {
    let wall = SimWall::new(WallTime { unix_ms: 10_000_000 });
    let before = ProxyInstant::now();
    apply_clock_jump(&wall, -3_600_000);
    assert_eq!(wall.now().unix_ms, 10_000_000 - 3_600_000);
    apply_clock_jump(&wall, 3_600_000 + 250);
    assert_eq!(wall.now().unix_ms, 10_000_250);
    assert_eq!(ProxyInstant::now(), before);
}
