//! Story 179 unit tests: rejected reloads keep the old registry, no secret or prompt in any
//! output, and the restart-required rule.
use super::reload_helpers::*;
use legatus_proxy::config::registry::ReloadResult;
use legatus_proxy::config::settings::Source;
use legatus_proxy::config::settings::setting_views;

const CANARY_SECRET: &str = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";
const CANARY_PROMPT: &str = "Summarise the confidential merger memo for Halvorsen Biotech";

fn errors_of(result: &ReloadResult) -> Vec<String> {
    match result {
        ReloadResult::Rejected { errors, .. } => errors.iter().map(|e| e.to_string()).collect(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn tc04_a_file_with_errors_after_a_valid_one_leaves_generation_2_in_use_and_lists_every_error() {
    let h = harness(V1);
    h.source.set_text(V2);
    h.reloader.reload_once().await;
    h.source.set_text("version: 3\nnodes: []\naliases: {}\nzzz: 1\n");
    let result = h.reloader.reload_once().await;
    assert_eq!(
        errors_of(&result),
        vec![
            "registry error bad_type at nodes: Expected map, found list.",
            "registry error bad_version at version: Schema version 1 is the only version.",
            "registry error unknown_field at zzz: Field is not part of the schema.",
        ]
    );
    let now = h.handle.snapshot();
    assert_eq!((now.generation, now.nodes.len()), (2, 2), "no part of the bad file was applied");
    assert!(h.out.text().contains("registry reload rejected: 3 error(s), the old registry stays in use"), "{}", h.out.text());
    assert!(h.out.text().contains("registry error bad_version at version"));
}

#[tokio::test(start_paused = true)]
async fn tc05_an_unreadable_file_keeps_the_old_registry_and_the_process_goes_on() {
    let h = harness(V1);
    h.source.remove();
    let result = h.reloader.reload_once().await;
    assert_eq!(errors_of(&result), vec!["registry error file_unreadable at registry.yaml: File does not exist."]);
    assert_eq!(h.handle.snapshot().generation, 1);
    h.source.set_text(V2);
    assert!(matches!(h.reloader.reload_once().await, ReloadResult::Loaded { generation: 2, .. }), "the next reload works: nothing exited");
}

#[tokio::test(start_paused = true)]
async fn tc06_a_file_cut_off_in_the_middle_keeps_the_old_registry_at_every_cut_point() {
    let h = harness(V1);
    let bytes = V2.as_bytes();
    for cut in (0..bytes.len()).step_by(7) {
        let generation_before = h.handle.snapshot().generation;
        h.source.set_bytes(Some(bytes[..cut].to_vec()));
        let result = h.reloader.reload_once().await;
        match result {
            ReloadResult::Rejected { .. } => assert_eq!(h.handle.snapshot().generation, generation_before, "cut at {cut}"),
            ReloadResult::Loaded { .. } => {
                // A cut can leave a complete smaller file (for example after a whole line); it is
                // then a valid file, and the swap is whole.
                let now = h.handle.snapshot();
                for alias in now.aliases.iter() {
                    assert!(alias.nodes.iter().all(|n| now.node(n).is_some()), "cut at {cut}");
                }
                h.source.set_text(V1);
                h.reloader.reload_once().await;
            }
            ReloadResult::NoneYet => panic!("a reload always gives a result"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn tc13_no_output_holds_a_canary_secret_or_prompt() {
    let h = harness(V1);
    // A rejected file: wrong kinds that hold canaries.
    h.source.set_text(&format!("version: 1\nnodes: {CANARY_SECRET}\naliases:\n  a: {CANARY_PROMPT}\nsettings: \"{CANARY_PROMPT}\"\n"));
    let rejected = h.reloader.reload_once().await;
    // A valid file with a secret reference. (Descriptive text fields such as quantisation are
    // ordinary data and are printed by Debug; the guarantee is for references and for the values
    // of a rejected file.)
    let valid = V2.replace("model: model-b\n", &format!("model: model-b\n    auth: {{ scheme: bearer, key_ref: \"{CANARY_SECRET}\" }}\n"));
    h.source.set_text(&valid);
    let loaded = h.reloader.reload_once().await;
    let registry = h.handle.snapshot();
    let outputs = [h.out.text(), format!("{rejected:?}"), format!("{loaded:?}"), format!("{:?}", registry.nodes), format!("{:?}", setting_views(&registry.settings))];
    for text in outputs {
        assert!(!text.contains("sk-ant") && !text.contains("Halvorsen"), "{text}");
    }
    let diff = legatus_proxy::config::diff::diff_registries(&legatus_proxy::config::typed::Registry::from_text(V1).unwrap(), &registry);
    assert_eq!(diff.secret_refs_changed, vec!["nodes.b.auth.key_ref".to_string()], "the diff names the place, not the reference");
    assert!(!format!("{diff:?}").contains("sk-ant"));
}

const KEYS_BASE: &str = "version: 1\nsettings:\n  listen: 127.0.0.1:18080\n  log_dir: /var/log/legatus-a\n  log_queue_capacity: 1000\n  state_dir: /var/lib/legatus-a\n  admin:\n    listen: 127.0.0.1:18081\n    token_file: /etc/legatus/a.token\n  hold_limit_s: 250\nnodes:\n  a:\n    engine: { name: ollama }\n    model: model-a\n    endpoints: [ { protocol: openai-chat, base_url: \"http://a.invalid:1\" } ]\naliases:\n  x: { nodes: [a] }\n";

#[tokio::test(start_paused = true)]
async fn tc10_each_start_only_setting_keeps_its_running_value_warns_once_and_the_other_changes_apply() {
    let h = harness(KEYS_BASE);
    let changed = KEYS_BASE
        .replace("127.0.0.1:18080", "127.0.0.1:19080")
        .replace("/var/log/legatus-a", "/var/log/legatus-b")
        .replace("1000", "2000")
        .replace("/var/lib/legatus-a", "/var/lib/legatus-b")
        .replace("127.0.0.1:18081", "127.0.0.1:19081")
        .replace("/etc/legatus/a.token", "/etc/legatus/b.token")
        .replace("hold_limit_s: 250", "hold_limit_s: 280");
    h.source.set_text(&changed);
    let result = h.reloader.reload_once().await;
    assert!(matches!(result, ReloadResult::Loaded { generation: 2, changed: true, .. }), "{result:?}");
    let now = h.handle.snapshot();
    assert_eq!(now.settings.hold_limit_s, 280, "the other changes are applied");
    assert_eq!(now.settings.listen.as_deref(), Some("127.0.0.1:18080"));
    assert_eq!((now.settings.log_dir.as_deref(), now.settings.log_queue_capacity, now.settings.state_dir.as_str()), (Some("/var/log/legatus-a"), 1000, "/var/lib/legatus-a"));
    assert_eq!((now.settings.admin_listen.as_str(), now.settings.admin_token_file.as_deref()), ("127.0.0.1:18081", Some("/etc/legatus/a.token")));
    let warnings: Vec<String> = now.warnings.iter().map(|w| w.to_string()).collect();
    assert_eq!(warnings.len(), 6, "{warnings:?}");
    for key in ["listen", "admin.listen", "admin.token_file", "log_dir", "log_queue_capacity", "state_dir"] {
        assert_eq!(warnings.iter().filter(|w| w.contains(&format!("restart_required at settings.{key}:"))).count(), 1, "{key}: {warnings:?}");
    }
    let views = setting_views(&now.settings);
    let listen = views.iter().find(|v| v.name == "listen").unwrap();
    assert_eq!((listen.value.as_str(), listen.declared.as_deref(), listen.source), ("127.0.0.1:18080", Some("127.0.0.1:19080"), Source::File));
    assert!(views.iter().find(|v| v.name == "hold_limit_s").unwrap().declared.is_none());
    assert!(h.out.text().matches("restart_required").count() >= 6);
}

#[tokio::test(start_paused = true)]
async fn tc10_the_warning_comes_again_on_the_next_reload_while_the_declared_value_still_differs() {
    let h = harness(KEYS_BASE);
    let changed = KEYS_BASE.replace("127.0.0.1:18080", "127.0.0.1:19080");
    h.source.set_text(&changed);
    h.reloader.reload_once().await;
    h.reloader.reload_once().await;
    assert_eq!(h.out.text().matches("restart_required at settings.listen").count(), 2);
    assert_eq!(h.handle.snapshot().generation, 2, "the second reload found nothing new to swap");
}

#[tokio::test(start_paused = true)]
async fn tc11_start_only_settings_that_did_not_change_give_no_warning_while_another_setting_changes() {
    let h = harness(KEYS_BASE);
    h.source.set_text(&KEYS_BASE.replace("hold_limit_s: 250", "hold_limit_s: 270"));
    h.reloader.reload_once().await;
    assert!(!h.out.text().contains("restart_required"), "{}", h.out.text());
    assert!(h.handle.snapshot().warnings.is_empty());
    assert_eq!(h.handle.snapshot().settings.hold_limit_s, 270);
}

#[test]
fn the_restart_key_list_is_the_catalogue_flag() {
    use legatus_proxy::config::reload::RESTART_REQUIRED_KEYS;
    use legatus_proxy::config::settings::CATALOGUE;
    let mut from_catalogue: Vec<&str> = CATALOGUE.iter().filter(|d| d.restart).map(|d| d.name).collect();
    let mut listed: Vec<&str> = RESTART_REQUIRED_KEYS.to_vec();
    from_catalogue.sort_unstable();
    listed.sort_unstable();
    assert_eq!(listed, from_catalogue);
}
