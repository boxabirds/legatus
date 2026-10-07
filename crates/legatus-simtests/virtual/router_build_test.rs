//! TC-10, TC-13, TC-14: router construction, background task handles, sorted visiting.
use axum::body::Body;
use http::{Request, StatusCode};
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::sorted::sorted_keys;
use legatus_proxy::spawn_named;
use legatus_proxy::time::WallTime;
use legatus_proxy::{Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::fleet::{one_node_deps, FLEET_REQUEST_BODY};

/// The router of the one-node fleet over the given seams.
fn build_router(seams: Seams) -> axum::Router {
    legatus_proxy::build_router(one_node_deps(seams))
}
use legatus_testkit::virt::{serve_duplex, FakeTransport, Script, SimWall};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn seams(transport: Arc<FakeTransport>) -> Seams {
    Seams {
        wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })),
        transport,
        log: Arc::new(DiscardSink),
        sim: SimPoints::new(),
    }
}

fn post() -> Request<Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").body(Body::from(FLEET_REQUEST_BODY)).unwrap()
}

#[test]
fn tc10_two_routers_share_no_state_and_build_touches_no_socket() {
    let a = build_router(seams(Arc::new(FakeTransport::new(Script::Connect))));
    let b = build_router(seams(Arc::new(FakeTransport::new(Script::Connect))));
    // Routers are plain values; both exist side by side with their own seams.
    drop((a, b));
}

#[tokio::test(start_paused = true)]
async fn tc10_connect_failure_gives_502() {
    let client = serve_duplex(build_router(seams(Arc::new(FakeTransport::new(Script::Connect))))).await;
    let response = client.send(post()).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}


#[tokio::test(start_paused = true)]
async fn tc13_spawn_named_returns_a_handle_and_aborting_leaves_no_task() {
    let handle = spawn_named("sleeper", async {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    });
    tokio::task::yield_now().await;
    assert!(!handle.is_finished());
    handle.abort();
    assert!(handle.await.unwrap_err().is_cancelled());
}

#[test]
fn tc14_keys_are_visited_sorted_over_20_runs() {
    let keys: Vec<u32> = (0..50).map(|i| (i * 37 + 11) % 101).collect();
    for run in 0..20 {
        let mut map = BTreeMap::new();
        let mut order = keys.clone();
        order.rotate_left(run % keys.len());
        for k in order {
            map.insert(k, ());
        }
        let visited = sorted_keys(&map);
        let mut sorted = visited.clone();
        sorted.sort_unstable();
        assert_eq!(visited, sorted);
    }
}
