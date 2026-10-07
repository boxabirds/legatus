//! TC-09 to TC-12, TC-14, TC-15 of story 162: the multi-slot llama-server stub.
use super::stub_helpers::*;
use futures_util::StreamExt;
use legatus_proxy::upstream::transport::UpstreamTransport;
use legatus_testkit::stubs::llama::MultiSlotLlamaStub;
use legatus_testkit::stubs::scenario::{StubKind, StubSpec};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

fn spec() -> StubSpec {
    let mut s = StubSpec::new("llama", StubKind::MultiSlotLlama);
    s.speed.load_time_ms = 0;
    s
}

async fn yield_a_few() {
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn tc09_pages_show_slots_metrics_only_when_on_and_six_requests_on_four_slots() {
    let off = MultiSlotLlamaStub::new(spec());
    let (_, props) = text_of(&off, get("/props")).await;
    assert!(props.contains("\"total_slots\":4"), "{props}");
    assert_eq!(text_of(&off, get("/metrics")).await.0, 404, "metrics off gives 404");
    assert_eq!(text_of(&off, get("/health")).await.0, 200);

    let mut s = spec();
    s.metrics_on = true;
    s.ctx_total = 16384;
    let stub = Arc::new(MultiSlotLlamaStub::new(s));
    let mut keep = Vec::new();
    for _ in 0..6 {
        let stub = stub.clone();
        keep.push(tokio::spawn(async move {
            let reply = stub.send(chat(2600, 3)).await.ok().unwrap();
            let mut body = reply.body;
            body.next().await
        }));
    }
    yield_a_few().await;
    let (_, metrics) = text_of(&*stub, get("/metrics")).await;
    assert!(metrics.contains("llamacpp:requests_processing 4"), "{metrics}");
    assert!(metrics.contains("llamacpp:requests_deferred 2"), "{metrics}");
    let (_, slots) = text_of(&*stub, get("/slots")).await;
    assert_eq!(slots.matches("\"is_processing\":true").count(), 4, "{slots}");
    assert!(slots.contains("\"n_ctx\":4096"), "{slots}");
    for k in keep {
        k.abort();
    }
}

#[tokio::test(start_paused = true)]
async fn tc10_a_queued_request_gets_headers_at_arrival_and_tokens_after_a_slot_frees() {
    let stub = Arc::new(MultiSlotLlamaStub::new(spec()));
    let started = Instant::now();
    let mut running = Vec::new();
    for _ in 0..4 {
        let stub = stub.clone();
        running.push(tokio::spawn(async move {
            let reply = stub.send(chat(260, 3)).await.ok().unwrap();
            drain(reply, started).await
        }));
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let fifth = stub.send(chat(260, 3)).await.ok().unwrap();
    assert_eq!(started.elapsed().as_millis(), 100, "headers come at the arrival instant");
    let (_, _, instants) = drain(fifth, started).await;
    assert!(instants[0] > 1000, "first token only after a slot frees: {instants:?}");
    for r in running {
        r.await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn tc11_context_per_slot_explicit_split_versus_unified() {
    let mut explicit = spec();
    explicit.ctx_total = 16384;
    let stub = MultiSlotLlamaStub::new(explicit);
    let (status, body) = text_of(&stub, chat(6850, 1)).await;
    assert_eq!(status, 400);
    assert!(body.contains("exceed_context_size_error"), "{body}");
    let mut big = spec();
    big.ctx_total = 32768;
    assert_eq!(text_of(&MultiSlotLlamaStub::new(big), chat(6850, 1)).await.0, 200);
    let mut unified = spec();
    unified.ctx_total = 16384;
    unified.kv_unified = true;
    assert_eq!(text_of(&MultiSlotLlamaStub::new(unified), chat(6850, 1)).await.0, 200);
}

#[tokio::test(start_paused = true)]
async fn tc12_malformed_json_is_500_and_valid_json_is_never_500() {
    let stub = MultiSlotLlamaStub::new(spec());
    assert_eq!(text_of(&stub, post("/v1/chat/completions", "{not json")).await.0, 500);
    for body in ["{}", "[]", "{\"messages\":[]}", "{\"prompt_tokens\":10}", "null"] {
        let (status, _) = text_of(&stub, post("/v1/chat/completions", body)).await;
        assert_ne!(status, 500, "valid JSON {body} must not give 500");
    }
}

#[tokio::test(start_paused = true)]
async fn tc14_slots_wakes_a_sleeping_engine_and_pays_load_time_other_pages_do_not() {
    let mut s = spec();
    s.sleep_on = true;
    s.metrics_on = true;
    s.speed.load_time_ms = 1500;
    let stub = MultiSlotLlamaStub::new(s);
    stub.force_sleep();
    for page in ["/metrics", "/health", "/props"] {
        let _ = text_of(&stub, get(page)).await;
        assert!(stub.is_asleep(), "{page} must not wake the engine");
    }
    let started = Instant::now();
    let (status, _) = text_of(&stub, get("/slots")).await;
    assert_eq!((status, started.elapsed().as_millis()), (200, 1500));
    assert!(!stub.is_asleep());
    // Idle for keep_alive_s puts it back to sleep.
    tokio::time::sleep(Duration::from_secs(301)).await;
    let _ = text_of(&stub, get("/props")).await;
    assert!(stub.is_asleep());
}

#[tokio::test(start_paused = true)]
async fn tc15_similarity_above_point_one_picks_the_similar_slot_otherwise_least_recently_used() {
    async fn run(similarity: f64) -> usize {
        let stub = MultiSlotLlamaStub::new(spec());
        // Use slot 0 first so it is the most recently used.
        let _ = text_of(&stub, chat(26, 1)).await;
        let body = format!("{{\"messages\":[],\"prompt_tokens\":26,\"output_tokens\":1,\"similar_slot\":2,\"prefix_similarity\":{similarity}}}");
        let _ = text_of(&stub, post("/v1/chat/completions", &body)).await;
        stub.last_slot().unwrap()
    }
    assert_eq!(run(0.11).await, 2);
    assert_eq!(run(0.1).await, 1, "exactly 0.1 is not above 0.1: least recently used");
    assert_eq!(run(0.09).await, 1);
}

#[tokio::test(start_paused = true)]
async fn tc19_two_simultaneous_7000_token_prompts_take_about_twice_as_long() {
    let mut s = spec();
    s.slots = 2;
    s.ctx_total = 32768;
    let single = Arc::new(MultiSlotLlamaStub::new(s.clone()));
    let started = Instant::now();
    let reply = single.send(chat(7000, 1)).await.ok().unwrap();
    let (_, _, one) = drain(reply, started).await;
    let two = Arc::new(MultiSlotLlamaStub::new(s));
    let started = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..2 {
        let two = two.clone();
        handles.push(tokio::spawn(async move { drain(two.send(chat(7000, 1)).await.ok().unwrap(), started).await.2[0] }));
    }
    for h in handles {
        let first = h.await.unwrap();
        let ratio = first as f64 / one[0] as f64;
        assert!((1.9..=2.1).contains(&ratio), "two prompts took {ratio} times one (PROPOSED: within 10 percent of twice)");
    }
}
