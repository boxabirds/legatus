//! Story 179 integration tests: a request that spans a reload keeps the record it started with,
//! a request after the swap does not use a removed node, and the observer notice reaches the
//! admin state.
use axum::body::Body;
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::lifecycle::start::AdminState;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::{build_router, RouterDeps, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::virt::{serve_duplex, FakeTransport, Script, SimWall};
use std::sync::Arc;
use std::time::Duration;

const WITH_A: &str = "version: 1\nnodes:\n  a:\n    engine: { name: ollama }\n    model: model-a\n    endpoints: [ { protocol: openai-chat, base_url: \"http://a.invalid:1\" } ]\naliases:\n  x: { nodes: [a] }\n";
const WITH_C: &str = "version: 1\nnodes:\n  c:\n    engine: { name: ollama }\n    model: model-c\n    endpoints: [ { protocol: openai-chat, base_url: \"http://c.invalid:3\" } ]\naliases:\n  x: { nodes: [c] }\n";

fn post() -> Request<Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").body(Body::from("{\"model\":\"x\"}")).unwrap()
}

fn chunks(count: usize, gap_ms: u64) -> Vec<(Duration, Result<Bytes, legatus_proxy::upstream::transport::UpstreamError>)> {
    (0..count).map(|i| (Duration::from_millis(gap_ms), Ok(Bytes::from(format!("part-{i};"))))).collect()
}

#[tokio::test(start_paused = true)]
async fn tc08_a_stream_from_a_node_the_reload_removed_finishes_and_a_new_request_does_not_use_it() {
    let h = super::reload_helpers::harness(WITH_A);
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: chunks(6, 1000) }));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake.clone(), log: Arc::new(DiscardSink), sim: SimPoints::new() };
    let client = Arc::new(serve_duplex(build_router(RouterDeps::for_test(seams).with_registry(h.handle.clone()))).await);
    let first = {
        let client = client.clone();
        tokio::spawn(async move {
            let response = client.send(post()).await;
            response.into_body().collect().await.unwrap().to_bytes()
        })
    };
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(!first.is_finished(), "the stream is still running");
    h.source.set_text(WITH_C);
    h.reloader.reload_once().await;
    assert_eq!(h.handle.snapshot().generation, 2);
    assert!(h.handle.snapshot().node(&legatus_common::ids::NodeId("a".into())).is_none());
    let bytes = first.await.unwrap();
    assert_eq!(bytes, Bytes::from("part-0;part-1;part-2;part-3;part-4;part-5;"), "the stream finished unbroken");
    let second = client.send(post()).await;
    second.into_body().collect().await.unwrap();
    let uris: Vec<String> = fake.requests().iter().map(|r| r.uri.to_string()).collect();
    assert_eq!(uris, vec!["http://a.invalid:1/v1/chat/completions", "http://c.invalid:3/v1/chat/completions"]);
    assert_eq!(fake.requests()[1].body, Bytes::from("{\"model\":\"model-c\"}"));
}

#[tokio::test(start_paused = true)]
async fn the_admin_state_is_the_first_observer_and_follows_every_swap() {
    let h = super::reload_helpers::harness(WITH_A);
    let admin = Arc::new(AdminState::default());
    admin.publish(&h.handle.snapshot());
    h.hub.register(admin.clone());
    assert_eq!(admin.node_views.get().len(), 1);
    h.source.set_text(WITH_C);
    h.reloader.reload_once().await;
    let views = admin.node_views.get();
    assert_eq!(views.iter().map(|v| v.name.0.as_str()).collect::<Vec<_>>(), vec!["c"]);
    h.source.set_text("version: 9\nnodes: {}\naliases: {}\n");
    h.reloader.reload_once().await;
    assert_eq!(admin.node_views.get().len(), 1, "a rejected reload leaves the served views as they were");
    assert!(!admin.setting_views.get().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_registry_handle_shared_with_the_router_is_the_one_the_reload_swaps() {
    let handle = Arc::new(RegistryHandle::new(Arc::new(Registry::from_text(WITH_A).unwrap())));
    let router_handle = handle.clone();
    assert!(Arc::ptr_eq(&handle, &router_handle));
}

#[tokio::test(start_paused = true)]
async fn the_observer_is_called_once_after_the_valid_swap_with_the_removed_node_and_not_by_a_rejected_or_identical_reload() {
    use legatus_proxy::config::diff::RegistryDiff;
    use legatus_proxy::config::reload::ReloadObserver;
    use std::sync::Mutex;
    struct Recorder(Arc<Mutex<Vec<RegistryDiff>>>);
    impl ReloadObserver for Recorder {
        fn on_reload(&self, _old: &Registry, _new: &Registry, diff: &RegistryDiff) {
            self.0.lock().unwrap().push(diff.clone());
        }
    }
    let h = super::reload_helpers::harness(WITH_A);
    let calls = Arc::new(Mutex::new(Vec::new()));
    h.hub.register(Arc::new(Recorder(calls.clone())));
    h.source.set_text(WITH_A);
    h.reloader.reload_once().await;
    h.source.set_text("version: 5\nnodes: {}\naliases: {}\n");
    h.reloader.reload_once().await;
    assert!(calls.lock().unwrap().is_empty(), "neither an identical file nor a rejected one calls the observer");
    h.source.set_text(WITH_C);
    h.reloader.reload_once().await;
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].nodes_removed, vec![legatus_common::ids::NodeId("a".into())]);
    assert_eq!(calls[0].nodes_added, vec![legatus_common::ids::NodeId("c".into())]);
}
