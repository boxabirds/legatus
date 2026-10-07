//! Story 165 unit tests: ranges, relations, hold limits, alias values, the 290 second warning.
use super::settings_helpers::*;
use legatus_proxy::config::settings::*;

const RANGE: &str = "Value is outside the allowed range.";

fn range_error(name: &str) -> String {
    format!("registry error bad_settings at settings.{name}: {RANGE}")
}

#[test]
fn tc04_protected_window_s_9_and_3601_are_refused_and_10_and_3600_pass() {
    for bad in [9, 3601] {
        let (s, e) = effective(&registry(&format!("  protected_window_s: {bad}\n")));
        assert!(s.is_none(), "no settings value on error");
        assert_eq!(e, vec![range_error("protected_window_s")], "{bad}");
    }
    assert!(effective(&registry("  protected_window_s: 10\n  probation_window_s: 5\n")).1.is_empty());
    assert_eq!(effective(&registry("  protected_window_s: 3600\n  hold_limit_s: 3600\n")).1, vec!["registry error bad_settings at settings.hold_limit_s: Value must be above protected_window_s."]);
}

#[test]
fn tc05_hold_limit_equal_to_the_window_is_refused_and_one_above_passes() {
    let (s, e) = effective(&registry("  protected_window_s: 180\n  hold_limit_s: 180\n"));
    assert!(s.is_none());
    assert_eq!(e, vec!["registry error bad_settings at settings.hold_limit_s: Value must be above protected_window_s."]);
    assert_eq!(effective(&registry("  protected_window_s: 180\n  hold_limit_s: 181\n")).0.unwrap().hold_limit_s, 181);
}

#[test]
fn tc05_a_window_set_above_the_default_hold_limit_fails_against_the_default() {
    assert_eq!(effective(&registry("  protected_window_s: 300\n")).1, vec!["registry error bad_settings at settings.hold_limit_s: Value must be above protected_window_s."]);
}

#[test]
fn tc06_hold_limit_290_loads_quietly_291_warns_3600_is_the_top_and_3601_is_refused() {
    assert!(warnings(&registry("  hold_limit_s: 290\n")).is_empty());
    assert!(errors(&registry("  hold_limit_s: 290\n")).is_empty());
    let report = report(&registry("  hold_limit_s: 291\n"));
    assert!(report.errors.is_empty());
    assert_eq!(report.warnings.iter().map(|w| w.to_string()).collect::<Vec<_>>(), vec!["warning hold_limit_above_budget at settings.hold_limit_s: Hold limit is above the budget; many harnesses give up near 300 seconds."]);
    assert!(errors(&registry("  hold_limit_s: 3600\n")).is_empty());
    assert_eq!(errors(&registry("  hold_limit_s: 3601\n")), vec![range_error("hold_limit_s")]);
    assert_eq!(HOLD_LIMIT_WARN_S, 290);
}

#[test]
fn tc07_an_alias_hold_limit_uses_the_same_range_and_relation_and_warns_above_290() {
    assert_eq!(warnings(&registry_with("", ", hold_limit_s: 291")), vec!["warning hold_limit_above_budget at aliases.x.hold_limit_s: Hold limit is above the budget; many harnesses give up near 300 seconds."]);
    assert!(warnings(&registry_with("", ", hold_limit_s: 290")).is_empty());
    assert_eq!(errors(&registry_with("", ", hold_limit_s: 100")), vec!["registry error bad_settings at aliases.x.hold_limit_s: Value must be above protected_window_s."]);
    assert_eq!(errors(&registry_with("", ", hold_limit_s: 180")), vec!["registry error bad_settings at aliases.x.hold_limit_s: Value must be above protected_window_s."]);
    assert_eq!(errors(&registry_with("", ", hold_limit_s: 3601")), vec!["registry error bad_settings at aliases.x.hold_limit_s: Value is outside the allowed range."]);
    assert_eq!(errors(&registry_with("", ", hold_limit_s: 99999999999")), vec!["registry error bad_settings at aliases.x.hold_limit_s: Value is outside the allowed range."]);
    assert!(errors(&registry_with("", ", hold_limit_s: 181")).is_empty());
}

#[test]
fn tc07_an_alias_value_is_checked_even_when_the_global_value_was_refused() {
    let found = errors(&registry_with("  hold_limit_s: 3601\n", ", hold_limit_s: 100"));
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].contains("aliases.x.hold_limit_s") && found[1].contains("settings.hold_limit_s"));
}

#[test]
fn tc08_hold_limit_status_503_504_529_pass_429_and_500_are_refused() {
    for ok in [503, 504, 529] {
        assert!(errors(&registry(&format!("  hold_limit_status: {ok}\n"))).is_empty(), "{ok}");
    }
    assert_eq!(errors(&registry("  hold_limit_status: 429\n")), vec!["registry error bad_settings at settings.hold_limit_status: Status 429 is refused; use 503, 504 or 529."]);
    assert_eq!(errors(&registry("  hold_limit_status: 500\n")), vec!["registry error bad_settings at settings.hold_limit_status: Status must be 503, 504 or 529."]);
    let (s, _) = effective(&registry("  hold_limit_status: 429\n"));
    assert!(s.is_none());
    assert_eq!(HOLD_STATUS_REFUSED, 429);
    assert_eq!(effective(&registry("  hold_limit_status: 529\n")).0.unwrap().hold_limit_status(), Some(HoldLimitStatus::S529));
}

#[test]
fn tc15_each_relation_is_checked_and_names_the_setting() {
    let cases = [
        ("  protected_window_s: 100\n  hold_limit_s: 200\n  probation_window_s: 101\n", "probation_window_s", "Value cannot be higher than protected_window_s."),
        ("  spill_after_s: 251\n", "spill_after_s", "Value cannot be higher than hold_limit_s."),
        ("  feedback_window: 8\n  feedback_min_turns: 9\n", "feedback_min_turns", "Value cannot be higher than feedback_window."),
        ("  log_pause_free_bytes: 5000\n  log_resume_free_bytes: 5000\n", "log_resume_free_bytes", "Value must be above log_pause_free_bytes."),
        ("  upstream_idle_reuse_max_s: 5\n", "upstream_idle_reuse_max_s", RANGE),
    ];
    for (lines, name, text) in cases {
        assert_eq!(errors(&registry(lines)), vec![format!("registry error bad_settings at settings.{name}: {text}")], "{name}");
    }
}

#[test]
fn tc15_equal_values_pass_where_the_relation_is_not_strict() {
    assert!(errors(&registry("  protected_window_s: 100\n  hold_limit_s: 200\n  probation_window_s: 100\n")).is_empty());
    assert!(errors(&registry("  spill_after_s: 250\n")).is_empty());
    assert!(errors(&registry("  feedback_window: 8\n  feedback_min_turns: 8\n")).is_empty());
    assert!(errors(&registry("  upstream_idle_reuse_max_s: 4\n")).is_empty());
}

#[test]
fn tc15_a_relation_is_not_reported_when_the_other_setting_is_itself_invalid() {
    assert_eq!(errors(&registry("  protected_window_s: 3601\n")), vec![range_error("protected_window_s")], "no cascade onto hold_limit_s");
}

#[test]
fn tc23_other_catalogue_ranges_are_inclusive_at_both_ends() {
    for (name, low_ok, high_ok, low_bad, high_bad) in [
        ("mature_turns", 1u64, 10u64, 0u64, 11u64),
        ("max_held", 1, 1024, 0, 1025),
        ("table_cap", 100, 1_000_000, 99, 1_000_001),
        ("reprobe_after_s", 60, 86_400, 59, 86_401),
        ("hold_retry_after_s", 0, 30, 31, 32),
    ] {
        for ok in [low_ok, high_ok] {
            assert!(errors(&registry(&format!("  {name}: {ok}\n"))).is_empty(), "{name} {ok}");
        }
        for bad in [low_bad, high_bad] {
            assert_eq!(errors(&registry(&format!("  {name}: {bad}\n"))), vec![range_error(name)], "{name} {bad}");
        }
    }
}

#[test]
fn tc23_feedback_window_bounds_are_checked_with_its_relation_satisfied() {
    // feedback_min_turns (default 10) must not exceed the window, so it is lowered to its own minimum.
    for ok in [5, 200] {
        assert!(errors(&registry(&format!("  feedback_window: {ok}\n  feedback_min_turns: 3\n"))).is_empty(), "{ok}");
    }
    for bad in [4, 201] {
        assert_eq!(errors(&registry(&format!("  feedback_window: {bad}\n  feedback_min_turns: 3\n"))), vec![range_error("feedback_window")], "{bad}");
    }
    assert_eq!(errors(&registry("  feedback_min_turns: 2\n")), vec![range_error("feedback_min_turns")]);
}

#[test]
fn tc23_reuse_floor_is_a_ratio_with_inclusive_ends() {
    assert!(errors(&registry("  reuse_floor: 0.05\n")).is_empty() && errors(&registry("  reuse_floor: 0.9\n")).is_empty());
    assert_eq!(errors(&registry("  reuse_floor: 0.04\n")), vec![range_error("reuse_floor")]);
    assert_eq!(errors(&registry("  reuse_floor: 0.91\n")), vec![range_error("reuse_floor")]);
}
