//! TC-16 to TC-18, TC-20 to TC-22 of story 162: the mlx_lm stub and the speed model.
use super::stub_helpers::*;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamTransport};
use legatus_testkit::stubs::mlx::BatchingMlxStub;
use legatus_testkit::stubs::scenario::{StubKind, StubSpec};
use legatus_testkit::stubs::speed::SpeedModel;
use std::time::Duration;
use tokio::time::Instant;

fn spec() -> StubSpec {
    StubSpec::new("mlx", StubKind::BatchingMlx)
}

#[tokio::test(start_paused = true)]
async fn tc16_eight_requests_are_admitted_at_once_and_load_pages_are_404() {
    let stub = BatchingMlxStub::new(spec());
    let started = Instant::now();
    let mut replies = Vec::new();
    for _ in 0..8 {
        replies.push(stub.send(chat(260, 3)).await.ok().unwrap());
    }
    assert_eq!(started.elapsed().as_millis(), 0, "all headers at once");
    assert_eq!(stub.busy(), 8);
    for page in ["/props", "/metrics", "/slots"] {
        assert_eq!(stub.send(get(page)).await.ok().unwrap().status.as_u16(), 404, "{page}");
    }
}

#[test]
fn tc17_per_request_rate_is_the_flat_aggregate_over_n() {
    let m = SpeedModel::mlx();
    for (n, aggregate) in [(1u32, 55.0), (2, 58.0), (4, 53.0), (8, 51.0)] {
        let rate = m.decode_rate(n);
        assert!((rate - aggregate / f64::from(n)).abs() < 1e-9, "N={n}: {rate}");
    }
}

#[tokio::test(start_paused = true)]
async fn tc18_invalid_drops_cancel_keeps_busy_cache_evicts_and_wedge_stops_answers() {
    let stub = BatchingMlxStub::new(spec());
    assert_eq!(stub.send(post("/v1/chat/completions", "not json")).await.err(), Some(UpstreamError::Reset));
    assert_eq!(stub.send(post("/v1/chat/completions", "{\"no_messages\":1}")).await.err(), Some(UpstreamError::Reset), "no status for an invalid request");

    let id = stub.next_request_id();
    let _reply = stub.send(chat(260, 3)).await.ok().unwrap();
    stub.cancel(id);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(stub.busy(), 1, "a request cancelled in prefill stays counted busy");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(stub.busy(), 0, "until its normal end");

    for i in 0..11 {
        let body = format!("{{\"messages\":[],\"prompt_tokens\":1,\"output_tokens\":1,\"cache_key\":\"k{i}\"}}");
        let _ = stub.send(post("/v1/chat/completions", &body)).await.ok().unwrap();
    }
    let keys = stub.cache_keys();
    assert_eq!((keys.len(), keys.first().map(String::as_str)), (10, Some("k1")), "the 11th entry evicts the oldest");

    let mut wedged = spec();
    wedged.wedge_after = Some(3);
    let stub = BatchingMlxStub::new(wedged);
    for _ in 0..3 {
        let _ = stub.send(chat(1, 1)).await.ok().unwrap();
    }
    let fourth = tokio::time::timeout(Duration::from_secs(60), stub.send(chat(1, 1))).await;
    assert!(fourth.is_err(), "the stub stops answering after 3 requests");
}

#[test]
fn tc20_first_token_is_queue_wait_plus_recompute_over_r_over_n() {
    let m = SpeedModel::mlx();
    for (n, want) in [(1u32, 10_000u64), (2, 20_000), (3, 30_000)] {
        assert_eq!(m.first_token_ms(2600, 2600, n, 0), want, "N={n}");
        assert_eq!(m.first_token_ms(2600, 2600, n, 500), want + 500);
    }
    assert_eq!(m.first_token_ms(0, 2600, 3, 750), 750, "zero recompute gives the queue wait");
}

#[test]
fn tc21_llama_server_four_slots_decode_table_and_clamp() {
    let m = SpeedModel::llama_4_slots();
    for (n, aggregate) in [(1u32, 28.0), (2, 34.0), (4, 46.0), (8, 49.0), (16, 49.0)] {
        let want = aggregate / f64::from(n);
        assert!((m.decode_rate(n) - want).abs() < 1e-9, "N={n}: {}", m.decode_rate(n));
    }
    assert!((SpeedModel::ollama_4_parallel().decode_rate(4) - 43.0 / 4.0).abs() < 1e-9);
    assert!((SpeedModel::llama_1_slot().decode_rate(2) - 25.0 / 2.0).abs() < 1e-9);
}

#[test]
fn tc22_prefill_table_interpolates_and_clamps() {
    let m = SpeedModel::strix_halo();
    let first_at = |ctx: u32| {
        // recompute 1000 tokens at one prompt in prefill: ms = 1000 / R * 1000
        let ms = m.first_token_ms(1000, ctx, 1, 0) as f64;
        1000.0 / ms * 1000.0
    };
    assert!((first_at(2_000) - 351.0).abs() < 1.0);
    assert!((first_at(120_000) - 172.0).abs() < 1.0);
    assert!((first_at(61_000) - 261.5).abs() < 1.5, "61k interpolates between 351 and 172");
    assert!((first_at(1_000) - 351.0).abs() < 1.0, "below the table clamps");
    assert!((first_at(500_000) - 172.0).abs() < 1.0, "above the table clamps");
}
