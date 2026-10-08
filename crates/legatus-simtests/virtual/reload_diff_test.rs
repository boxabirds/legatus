//! Story 179 unit tests: the registry diff and the observer calls.
use super::reload_helpers::*;
use legatus_proxy::config::diff::{diff_registries, NodeChange};
use legatus_proxy::config::typed::Registry;
use legatus_common::ids::{AliasName, NodeId};
use std::sync::{Arc, Mutex};

fn reg(text: &str) -> Registry {
    Registry::from_text(text).unwrap()
}

fn node_id(n: &str) -> NodeId {
    NodeId(n.to_string())
}

#[test]
fn tc12_node_added_removed_and_alias_changes_are_listed_exactly() {
    let diff = diff_registries(&reg(V1), &reg(V2));
    assert_eq!(diff.nodes_added, vec![node_id("b")]);
    assert!(diff.nodes_removed.is_empty() && diff.nodes_changed.is_empty());
    assert_eq!(diff.aliases_changed, vec![AliasName("x".into()), AliasName("y".into())]);
    let back = diff_registries(&reg(V2), &reg(V1));
    assert_eq!(back.nodes_removed, vec![node_id("b")]);
    assert_eq!(back.aliases_changed, vec![AliasName("x".into()), AliasName("y".into())]);
}

fn change_in_a(replace: &str, with: &str) -> bool {
    let changed = V1.replace(replace, with);
    assert_ne!(changed, V1, "the mutation applies: {replace}");
    let diff = diff_registries(&reg(V1), &reg(&changed));
    assert_eq!(diff.nodes_changed.len(), 1, "{replace}");
    assert_eq!(diff.nodes_changed[0].name, node_id("a"));
    assert!(diff.nodes_added.is_empty() && diff.nodes_removed.is_empty());
    diff.nodes_changed[0].probe_key_changed
}

#[test]
fn tc12_probe_key_changes_are_version_model_context_patch_and_flags_only() {
    assert!(change_in_a("version: \"0.35\"", "version: \"0.36\""), "engine version");
    assert!(change_in_a("model: model-a", "model: model-a2"), "model");
    assert!(change_in_a("    model: model-a\n", "    model: model-a\n    context: { per_slot: 8192, on_overflow: error_400 }\n"), "context");
    assert!(change_in_a("    model: model-a\n", "    model: model-a\n    patch: { set: { reasoning_effort: none } }\n"), "patch");
    assert!(change_in_a("    model: model-a\n", "    model: model-a\n    engine_flags: { jinja: true }\n"), "flags");
    assert!(!change_in_a("    model: model-a\n", "    model: model-a\n    quantisation: q4\n"), "quantisation is not a probe key");
    assert!(!change_in_a("    model: model-a\n", "    model: model-a\n    always_on: false\n"), "always_on is not a probe key");
    assert!(!change_in_a("    model: model-a\n", "    model: model-a\n    slots: 2\n"), "slots is not a probe key");
}

#[test]
fn tc12_machine_setting_and_secret_reference_changes() {
    let with_machine = V1.replace("nodes:\n  a:", "machines:\n  m1: { host: m1.local, mem_gb: 16 }\nnodes:\n  a:");
    let moved = with_machine.replace("host: m1.local", "host: m2.local");
    let diff = diff_registries(&reg(&with_machine), &reg(&moved));
    assert_eq!(diff.machines_changed, vec!["m1".to_string()]);
    assert!(diff.nodes_changed.is_empty(), "a machine change needs no probe");
    let diff = diff_registries(&reg(V1), &reg(&V1.replace("listen: 127.0.0.1:18080\n", "listen: 127.0.0.1:18080\n  hold_limit_s: 280\n")));
    assert_eq!(diff.settings_changed, vec!["hold_limit_s"]);
    let keyed = V1.replace("    model: model-a\n", "    model: model-a\n    auth: { scheme: bearer, key_ref: \"env:ONE\" }\n");
    let rotated = keyed.replace("env:ONE", "env:TWO");
    let diff = diff_registries(&reg(&keyed), &reg(&rotated));
    assert_eq!(diff.secret_refs_changed, vec!["nodes.a.auth.key_ref".to_string()]);
    assert_eq!(diff.nodes_changed.len(), 1);
    assert!(!diff.nodes_changed[0].probe_key_changed);
    let tokens = V1.replace("listen: 127.0.0.1:18080\n", "listen: 127.0.0.1:18080\n  client_tokens_ref: \"file:/etc/a\"\n");
    let diff = diff_registries(&reg(&tokens), &reg(&tokens.replace("/etc/a", "/etc/b")));
    assert_eq!(diff.secret_refs_changed, vec!["settings.client_tokens_ref".to_string()]);
    assert_eq!(NodeChange { name: node_id("a"), probe_key_changed: true }.name, node_id("a"));
}

#[tokio::test(start_paused = true)]
async fn tc14_an_identical_file_gives_an_empty_diff_no_swap_no_observer_call_and_an_unchanged_event() {
    let h = harness(V1);
    let seen = Arc::new(Mutex::new(Vec::new()));
    h.hub.register(Arc::new(Labelled { label: "one", seen: seen.clone(), panics: false }));
    let before = h.handle.snapshot();
    let result = h.reloader.reload_once().await;
    assert!(matches!(result, legatus_proxy::config::registry::ReloadResult::Loaded { generation: 1, changed: false, .. }), "{result:?}");
    assert!(Arc::ptr_eq(&before, &h.handle.snapshot()), "no swap");
    assert!(seen.lock().unwrap().is_empty());
    assert!(h.out.text().contains("registry reloaded: unchanged (generation 1)"));
    assert_eq!(h.sink.len(), 1, "one registry_loaded event");
    assert!(diff_registries(&reg(V1), &reg(V1)).is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc15_observers_run_after_the_swap_in_registration_order_and_a_panic_does_not_undo_or_stop_the_rest() {
    let h = harness(V1);
    let seen = Arc::new(Mutex::new(Vec::new()));
    for (label, panics) in [("secrets", false), ("client_gate", false), ("cap_counter", true), ("seat_table", false), ("probe_controller", false)] {
        h.hub.register(Arc::new(Labelled { label, seen: seen.clone(), panics }));
    }
    h.source.set_text(V2);
    h.reloader.reload_once().await;
    assert_eq!(*seen.lock().unwrap(), vec!["secrets:1->2", "client_gate:1->2", "cap_counter:1->2", "seat_table:1->2", "probe_controller:1->2"]);
    assert_eq!(h.handle.snapshot().generation, 2, "the swap stands although an observer panicked");
}

#[tokio::test(start_paused = true)]
async fn tc15_an_observer_sees_the_new_registry_already_in_the_handle() {
    struct Check(Arc<legatus_proxy::config::typed::RegistryHandle>, Arc<Mutex<Option<u64>>>);
    impl legatus_proxy::config::reload::ReloadObserver for Check {
        fn on_reload(&self, _old: &Registry, new: &Registry, _diff: &legatus_proxy::config::diff::RegistryDiff) {
            *self.1.lock().unwrap() = Some(self.0.snapshot().generation);
            assert_eq!(new.generation, self.0.snapshot().generation);
        }
    }
    let h = harness(V1);
    let seen = Arc::new(Mutex::new(None));
    h.hub.register(Arc::new(Check(h.handle.clone(), seen.clone())));
    h.source.set_text(V2);
    h.reloader.reload_once().await;
    assert_eq!(*seen.lock().unwrap(), Some(2));
}
