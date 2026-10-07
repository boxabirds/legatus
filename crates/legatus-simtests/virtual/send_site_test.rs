//! Story 121 unit tests: path recognition, the single send function, the transport idle time, deps defaults.
use bytes::Bytes;
use futures_util::StreamExt;
use http::{HeaderMap, Method, StatusCode};
use legatus_common::ids::NodeId;
use legatus_common::protocol::Protocol;
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::net::hyper_transport::HyperTransport;
use legatus_proxy::protocol::paths::{recognise, RouteKind, CHAT_COMPLETIONS_PATH};
use legatus_proxy::upstream::send::send_to_node;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest};
use legatus_testkit::fleet::{one_node_registry_text, FLEET_NODE_URL};
use legatus_testkit::virt::{FakeTransport, Script};
use std::sync::Arc;
use std::time::Duration;

fn request() -> UpstreamRequest {
    UpstreamRequest { method: Method::POST, uri: "http://node.invalid/v1/chat/completions".parse().unwrap(), headers: HeaderMap::new(), body: Bytes::from_static(b"{}") }
}

#[test]
fn recognise_matches_the_chat_path_exactly_and_nothing_else() {
    assert_eq!(recognise(CHAT_COMPLETIONS_PATH), Some((Protocol::OpenAiChat, RouteKind::Chat)));
    for other in ["/v1/chat/completions/", "/v1/chat/completions?x=1", "/V1/chat/completions", "/v1/messages", "/v1/responses", "/v1/models", "/", ""] {
        assert_eq!(recognise(other), None, "{other:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn send_to_node_returns_connect_before_the_head() {
    let fake = FakeTransport::new(Script::Connect);
    let result = send_to_node(&fake, &NodeId("n".into()), request()).await;
    assert_eq!(result.err(), Some(UpstreamError::Connect));
    assert_eq!(fake.requests().len(), 1, "one call, no retry");
}

#[tokio::test(start_paused = true)]
async fn send_to_node_returns_the_head_first_and_a_reset_stays_in_the_body_stream() {
    let chunks = vec![(Duration::from_millis(5), Ok(Bytes::from_static(b"part"))), (Duration::from_millis(5), Err(UpstreamError::Reset))];
    let fake = FakeTransport::new(Script::Response { status: StatusCode::OK, chunks });
    let reply = send_to_node(&fake, &NodeId("n".into()), request()).await.expect("the head arrives");
    assert_eq!(reply.status, StatusCode::OK);
    let items: Vec<_> = reply.body.collect().await;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_ref().unwrap(), &Bytes::from_static(b"part"));
    assert_eq!(items[1].as_ref().unwrap_err(), &UpstreamError::Reset);
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn send_to_node_passes_an_error_status_as_a_response_and_makes_no_retry() {
    let fake = FakeTransport::new(Script::Response { status: StatusCode::BAD_GATEWAY, chunks: vec![] });
    let reply = send_to_node(&fake, &NodeId("n".into()), request()).await.unwrap();
    assert_eq!(reply.status, StatusCode::BAD_GATEWAY);
    assert_eq!(fake.requests().len(), 1);
}

#[test]
fn tc27_the_real_transport_has_the_idle_time_it_was_given_and_the_catalogue_default_otherwise() {
    assert_eq!(HyperTransport::with_idle_timeout(Duration::from_secs(4)).idle_timeout(), Duration::from_secs(4));
    assert_eq!(HyperTransport::with_idle_timeout(Duration::from_secs(1)).idle_timeout(), Duration::from_secs(1));
    let registry = Registry::from_text(&one_node_registry_text(FLEET_NODE_URL)).unwrap();
    assert_eq!(HyperTransport::new().idle_timeout(), Duration::from_secs(u64::from(registry.settings.upstream_idle_reuse_max_s)), "the transport default is the catalogue default");
}

#[test]
fn tc27_a_transport_configured_after_creation_takes_the_setting_once() {
    let transport = HyperTransport::unconfigured();
    assert!(transport.configure(Duration::from_secs(3)));
    assert_eq!(transport.idle_timeout(), Duration::from_secs(3));
    assert!(!transport.configure(Duration::from_secs(1)), "the pool exists, the setting is not changed in place");
    assert_eq!(transport.idle_timeout(), Duration::from_secs(3));
}

#[test]
fn tc28_the_registry_handle_swaps_whole_and_a_snapshot_keeps_its_generation() {
    let handle = RegistryHandle::empty();
    let before = handle.snapshot();
    assert_eq!((before.generation, before.nodes.len()), (0, 0));
    let mut next = Registry::from_text(&one_node_registry_text(FLEET_NODE_URL)).unwrap();
    next.generation = 7;
    handle.store(Arc::new(next));
    assert_eq!(handle.snapshot().generation, 7);
    assert_eq!(handle.snapshot().nodes.len(), 1);
    assert_eq!(before.generation, 0, "a request that took its snapshot earlier keeps it");
}

#[test]
fn tc28_harness_rows_are_read_into_the_typed_registry() {
    let text = format!("{}harnesses:\n  - name: pi\n    session_header: x-session-affinity\n    never_key_headers: [authorization]\n", one_node_registry_text(FLEET_NODE_URL));
    let registry = Registry::from_text(&text).unwrap();
    assert_eq!(registry.harness_rows.len(), 1);
    assert_eq!(registry.harness_rows[0].name, "pi");
    assert_eq!(registry.harness_rows[0].session_header.as_deref(), Some("x-session-affinity"));
    assert_eq!(registry.harness_rows[0].never_key_headers, vec!["authorization"]);
}
