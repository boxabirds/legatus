//! Story 165 unit tests: the catalogue, defaults, key limits, wrapper tags.
use super::settings_helpers::*;
use legatus_common::ids::AliasName;
use legatus_proxy::config::read::parse_registry_text;
use legatus_proxy::config::settings::*;

fn view<'a>(views: &'a [SettingView], name: &str) -> &'a SettingView {
    views.iter().find(|v| v.name == name).unwrap_or_else(|| panic!("no view for {name}"))
}

const SETTING_COUNT: usize = 38;

#[test]
fn tc01_no_settings_map_returns_every_catalogue_default_and_the_view_marks_default_sources() {
    let s = defaults();
    assert_eq!((s.hold_limit_s, s.protected_window_s, s.probation_window_s, s.mature_turns), (250, 180, 30, 2));
    assert_eq!((s.max_held, s.max_held_bytes, s.hold_retry_after_s, s.table_ttl_s, s.table_cap), (64, 268_435_456, 30, 600, 10_000));
    assert_eq!((s.spill_after_s, s.feedback_window, s.feedback_min_turns, s.feedback_min_tokens), (None, 20, 10, 2048));
    assert_eq!((s.reuse_floor, s.reprobe_after_s, s.key_text_limit_system, s.key_text_limit_first), (0.20, 600, 32_768, 8_192));
    assert_eq!((s.body_limit_bytes, s.cold_allowance_tokens, s.probe_budget_s, s.ready_health_wait_s), (33_554_432, 516, 600, 5));
    assert_eq!((s.node_probe_interval_s, s.upstream_idle_reuse_max_s, s.truncation_report_ratio), (2, 4, 0.25));
    assert_eq!((s.ollama_truncation_limit_ratio, s.ollama_bytes_per_token), (1.0, 3));
    assert_eq!((s.log_check_interval_s, s.log_pause_free_bytes, s.log_resume_free_bytes, s.log_queue_capacity), (10, 1_073_741_824, 2_147_483_648, 8192));
    assert_eq!((s.state_dir.as_str(), s.admin_listen.as_str(), s.listen.clone(), s.log_dir.clone(), s.admin_token_file.clone(), s.client_tokens_ref.clone()), ("legatus-state", "127.0.0.1:8081", None, None, None, None));
    assert_eq!(s.hold_limit_status(), None, "not set by the file");
    let views = setting_views(&s);
    assert_eq!(views.len(), SETTING_COUNT);
    assert!(views.iter().all(|v| v.source == Source::Default));
    let hold = view(&views, "hold_limit_s");
    assert_eq!((hold.value.as_str(), hold.default.as_str(), hold.unit, hold.status, hold.restart), ("250", "250", Unit::Seconds, Status::Proposed, false));
    assert_eq!(view(&views, "listen").restart, true);
    assert_eq!(view(&views, "spill_after_s").value, "off");
}

#[test]
fn tc02_catalogue_integrity_every_row_has_an_owner_a_spec_id_and_a_unique_name() {
    assert_eq!(CATALOGUE.len(), SETTING_COUNT);
    let mut names: Vec<&str> = CATALOGUE.iter().map(|d| d.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), CATALOGUE.len(), "names are unique");
    for d in CATALOGUE {
        assert!(!d.owner_story.is_empty() && d.owner_story.chars().all(|c| c.is_ascii_digit()), "{} owner", d.name);
        assert!(!d.spec_ids.is_empty(), "{} spec ids", d.name);
    }
    let status = |n: &str| CATALOGUE.iter().find(|d| d.name == n).unwrap().status;
    assert_eq!(status("protected_window_s"), Status::Proposed);
    assert_eq!(status("hold_limit_s"), Status::Proposed);
    assert_eq!(status("mature_turns"), Status::Pending);
    assert_eq!(status("hold_retry_after_s"), Status::Pending);
    assert_eq!(status("body_limit_bytes"), Status::NotSet);
    assert_eq!(status("log_queue_capacity"), Status::NotSet);
    assert_eq!(status("hold_limit_status"), Status::Approved);
    assert_eq!(status("listen"), Status::Approved);
    let views = setting_views(&defaults());
    assert_eq!(view(&views, "mature_turns").status.code(), "PD");
    assert_eq!(view(&views, "body_limit_bytes").status.code(), "NS");
    assert_eq!(view(&views, "reuse_floor").status.code(), "P");
    assert_eq!(view(&views, "listen").status.code(), "A");
}

#[test]
fn tc02_the_fixed_constants_have_their_values_and_owner_stories() {
    let value = |n: &str| FIXED_CONSTANTS.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no constant {n}"));
    for (name, v, owner) in [
        ("ADMIN_TOKEN_POLL_S", 2, "159"), ("STATE_SOCKET_MAX_CONNS", 8, "159"), ("DASHBOARD_REFRESH_S", 5, "173"), ("DASHBOARD_WINDOW_S", 3600, "173"),
        ("DASHBOARD_RECENT_DECISIONS", 20, "173"), ("USAGE_TAIL_BYTES", 65536, "171"), ("AGENT_STALE_AFTER_INTERVALS", 3, "181"),
        ("VLLM_REUSE_MEASURE_FLOOR_TOKENS", 528, "183"), ("HOLD_LIMIT_DEFAULT_S", 250, "119"), ("MODEL_LIST_CREATED", 0, "150"),
        ("RESPONSE_ID_SCAN_LIMIT_BYTES", 16384, "137"), ("UNKNOWN_ENGINE_CAP", 1, "127"), ("LLAMA_SERVER_DEFAULT_SLOTS", 4, "151"),
        ("OLLAMA_DEFAULT_SLOTS", 1, "174"), ("MLX_LM_DEFAULT_SLOTS", 1, "183"),
    ] {
        assert_eq!((value(name).value, value(name).owner_story), (v, owner), "{name}");
    }
    assert_eq!(HOLD_LIMIT_DEFAULT_S, 250, "the same number as the catalogue default of hold_limit_s");
}

#[test]
fn tc03_hold_window_mature_turns_max_held_and_status_are_read_from_the_file() {
    let text = registry("  hold_limit_s: 280\n  protected_window_s: 200\n  probation_window_s: 40\n  mature_turns: 3\n  max_held: 32\n  hold_limit_status: 504\n");
    let (s, errors) = effective(&text);
    assert!(errors.is_empty(), "{errors:?}");
    let s = s.unwrap();
    assert_eq!((s.hold_limit_s, s.protected_window_s, s.probation_window_s, s.mature_turns, s.max_held), (280, 200, 40, 3, 32));
    assert_eq!(s.hold_limit_status(), Some(HoldLimitStatus::S504));
    assert_eq!(s.listen.as_deref(), Some("127.0.0.1:8080"));
    let views = setting_views(&s);
    assert_eq!(view(&views, "hold_limit_s").source, Source::File);
    assert_eq!(view(&views, "table_ttl_s").source, Source::Default);
}

#[test]
fn tc09_table_ttl_s_defaults_to_600_and_299_and_86401_are_refused() {
    assert_eq!(effective(&registry("")).0.unwrap().table_ttl_s, 600);
    assert_eq!(effective(&registry("  table_ttl_s: 300\n")).0.unwrap().table_ttl_s, 300);
    assert_eq!(effective(&registry("  table_ttl_s: 86400\n")).0.unwrap().table_ttl_s, 86_400);
    for bad in ["299", "86401"] {
        let (s, errors) = effective(&registry(&format!("  table_ttl_s: {bad}\n")));
        assert!(s.is_none());
        assert_eq!(errors, vec!["registry error bad_settings at settings.table_ttl_s: Value is outside the allowed range."], "{bad}");
    }
}

#[test]
fn tc11_key_text_limit_system_bounds() {
    assert_eq!(effective(&registry("")).0.unwrap().key_text_limit_system, 32_768);
    for ok in [256, 262_144] {
        assert_eq!(effective(&registry(&format!("  key_text_limit_system: {ok}\n"))).0.unwrap().key_text_limit_system, ok);
    }
    for bad in [255, 262_145] {
        assert_eq!(effective(&registry(&format!("  key_text_limit_system: {bad}\n"))).1, vec!["registry error bad_settings at settings.key_text_limit_system: Value is outside the allowed range."]);
    }
}

#[test]
fn tc12_key_text_limit_first_bounds() {
    assert_eq!(effective(&registry("")).0.unwrap().key_text_limit_first, 8_192);
    for ok in [256, 65_536] {
        assert_eq!(effective(&registry(&format!("  key_text_limit_first: {ok}\n"))).0.unwrap().key_text_limit_first, ok);
    }
    for bad in [255, 65_537] {
        assert_eq!(effective(&registry(&format!("  key_text_limit_first: {bad}\n"))).1.len(), 1, "{bad}");
    }
}

#[test]
fn tc13_both_limits_8192_load_and_the_system_limit_is_not_replaced_by_the_default() {
    let s = effective(&registry("  key_text_limit_system: 8192\n  key_text_limit_first: 8192\n")).0.unwrap();
    assert_eq!((s.key_text_limit_system, s.key_text_limit_first), (8192, 8192));
}

#[test]
fn tc14_wrapper_tags_default_list_replaces_and_a_string_is_bad_type() {
    assert_eq!(defaults().wrapper_tags, vec!["<environment_context>", "<system-reminder>", "<user_instructions>", "<skills_instructions>"]);
    let custom = effective(&registry("  wrapper_tags: [\"<my-tag>\"]\n")).0.unwrap();
    assert_eq!(custom.wrapper_tags, vec!["<my-tag>"]);
    assert_eq!(effective(&registry("  wrapper_tags: \"<my-tag>\"\n")).1, vec!["registry error bad_type at settings.wrapper_tags: Expected list of strings, found string."]);
    assert_eq!(effective(&registry("  wrapper_tags: [1, 2]\n")).1.len(), 1);
}

#[test]
fn tc16_a_misspelled_setting_is_unknown_field_a_wrong_kind_is_bad_type_and_off_is_accepted() {
    assert_eq!(effective(&registry("  hold_limt_s: 200\n")).1, vec!["registry error unknown_field at settings.hold_limt_s: Field is not part of the schema."]);
    assert_eq!(effective(&registry("  hold_limit_s: long\n")).1, vec!["registry error bad_type at settings.hold_limit_s: Expected whole number, found string."]);
    assert_eq!(effective(&registry("  hold_limit_s: 250.5\n")).1, vec!["registry error bad_type at settings.hold_limit_s: Expected whole number, found number."]);
    assert_eq!(effective(&registry("  spill_after_s: off\n")).0.unwrap().spill_after_s, None);
    assert_eq!(effective(&registry("  spill_after_s: 60\n")).0.unwrap().spill_after_s, Some(60));
    assert_eq!(effective(&registry("  spill_after_s: maybe\n")).1, vec!["registry error bad_type at settings.spill_after_s: Expected a whole number or off, found string."]);
    assert_eq!(effective(&registry("  hold_limit_s: off\n")).1.len(), 1, "off is only for settings whose default is off");
}

#[test]
fn tc16_a_settings_map_without_listen_is_missing_key_and_a_bad_address_is_refused() {
    let no_listen = "version: 1\nsettings:\n  hold_limit_s: 200\nnodes: {}\naliases: {}\n";
    assert_eq!(effective(no_listen).1, vec!["registry error missing_key at settings.listen: Required field is missing."]);
    assert_eq!(effective(&registry("").replace("127.0.0.1:8080", "nowhere")).1, vec!["registry error bad_settings at settings.listen: Address must be a host and port."]);
    assert_eq!(effective(&registry("  admin:\n    listen: 127.0.0.1:9000\n    token_file: /etc/legatus/admin.token\n")).0.unwrap().admin_listen, "127.0.0.1:9000");
    assert_eq!(effective(&registry("  admin:\n    bogus: 1\n")).1, vec!["registry error unknown_field at settings.admin.bogus: Field is not part of the schema."]);
}

#[test]
fn tc17_settings_with_a_range_that_is_not_set_check_only_the_kind() {
    for ok in [0, 2048, 1_000_000] {
        assert_eq!(effective(&registry(&format!("  feedback_min_tokens: {ok}\n"))).0.unwrap().feedback_min_tokens, ok);
    }
    assert_eq!(effective(&registry("  feedback_min_tokens: lots\n")).1, vec!["registry error bad_type at settings.feedback_min_tokens: Expected whole number, found string."]);
    let views = setting_views(&defaults());
    assert_eq!(view(&views, "feedback_min_tokens").range, "NOT SET");
    assert_eq!(view(&views, "probe_budget_s").range, "NOT SET");
    assert_eq!(view(&views, "protected_window_s").range, "10 to 3600");
    assert_eq!(view(&views, "key_text_limit_first").range, "256 to 65536");
}

#[test]
fn tc21_a_canary_in_a_wrong_kind_value_never_reaches_an_error_text() {
    let canary = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";
    let text = registry(&format!("  hold_limit_s: {canary}\n  max_held: [\"{canary}\"]\n  reuse_floor: \"{canary}\"\n"));
    let found = errors(&text);
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|e| !e.contains("sk-ant")), "{found:?}");
}

#[test]
fn tc23_state_dir_max_held_bytes_the_ollama_rows_and_the_restart_column() {
    let s = defaults();
    assert_eq!((s.state_dir.as_str(), s.max_held_bytes, s.ollama_truncation_limit_ratio, s.ollama_bytes_per_token), ("legatus-state", 268_435_456, 1.0, 3));
    for ok in [1_048_576u64, 17_179_869_184] {
        assert_eq!(effective(&registry(&format!("  max_held_bytes: {ok}\n"))).0.unwrap().max_held_bytes, ok);
    }
    for bad in [1_048_575u64, 17_179_869_185] {
        assert_eq!(effective(&registry(&format!("  max_held_bytes: {bad}\n"))).1.len(), 1, "{bad}");
    }
    for (text, ok) in [("0.1", true), ("1.0", true), ("0.09", false), ("1.01", false)] {
        assert_eq!(effective(&registry(&format!("  ollama_truncation_limit_ratio: {text}\n"))).1.is_empty(), ok, "{text}");
    }
    for (n, ok) in [(1, true), (16, true), (0, false), (17, false)] {
        assert_eq!(effective(&registry(&format!("  ollama_bytes_per_token: {n}\n"))).1.is_empty(), ok, "{n}");
    }
    let mut restart: Vec<&str> = CATALOGUE.iter().filter(|d| d.restart).map(|d| d.name).collect();
    restart.sort_unstable();
    assert_eq!(restart, vec!["admin.listen", "admin.token_file", "listen", "log_dir", "log_queue_capacity", "state_dir"]);
}

#[test]
fn tc24_cold_allowance_tokens_has_one_default_and_a_node_override_replaces_it_for_that_node_only() {
    assert_eq!(defaults().cold_allowance_tokens, 516);
    assert_eq!(effective(&registry("  cold_allowance_tokens: 1000\n")).0.unwrap().cold_allowance_tokens, 1000);
    let text = "version: 1\nnodes:\n  a:\n    engine: { name: ollama }\n    model: m\n    endpoints: [ { protocol: openai-chat, base_url: \"http://a\" } ]\n    cold_allowance_tokens: 900\n  b:\n    engine: { name: llama-server }\n    model: m\n    endpoints: [ { protocol: openai-chat, base_url: \"http://b\" } ]\naliases: {}\n";
    let doc = parse_registry_text(text, PATH_LABEL).unwrap();
    let nodes = legatus_proxy::config::node::read_nodes(&doc.0, &mut legatus_proxy::config::registry::ValidationReport::default());
    assert_eq!((nodes[0].cold_allowance_tokens, nodes[1].cold_allowance_tokens), (Some(900), None));
}

#[test]
fn the_hold_limit_accessor_gives_the_alias_value_else_the_global_one() {
    let text = registry_with("", ", hold_limit_s: 280");
    let doc = parse_registry_text(&text, PATH_LABEL).unwrap();
    let mut scratch = legatus_proxy::config::registry::ValidationReport::default();
    let typed = legatus_proxy::config::node::read_nodes(&doc.0, &mut scratch);
    let aliases = legatus_proxy::config::alias::read_aliases(&doc.0, &typed, &mut scratch).unwrap();
    let s = read_settings(doc.0.as_mapping().and_then(|r| r.get("settings")), &aliases, &mut scratch).unwrap();
    assert_eq!(s.hold_limit_s(&AliasName("x".into())), 280);
    assert_eq!(s.hold_limit_s(&AliasName("other".into())), 250);
    let views = setting_views(&s);
    assert_eq!(view(&views, "aliases.x.hold_limit_s").value, "280");
}
