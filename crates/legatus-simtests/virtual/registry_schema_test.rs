//! Story 125 unit tests: version, unknown fields at every depth, the collected sorted list.
use legatus_proxy::config::read::{parse_registry_text, print_errors};
use legatus_proxy::config::registry::*;
use legatus_proxy::config::validate::{validate_registry, validate_with, RegistryCheck};

const SPEC_EXAMPLE: &str = include_str!("../fixtures/registry/spec_example.yaml");
const PATH_LABEL: &str = "registry.yaml";

fn report(text: &str) -> ValidationReport {
    validate_registry(&parse_registry_text(text, PATH_LABEL).expect("valid structured text"))
}

fn lines(text: &str) -> Vec<String> {
    report(text).errors.iter().map(|e| e.to_string()).collect()
}

const MINIMAL: &str = "version: 1\nnodes: {}\naliases: {}\n";

#[test]
fn tc01_the_spec_example_loads_with_zero_errors_and_every_key() {
    let doc = parse_registry_text(SPEC_EXAMPLE, PATH_LABEL).unwrap();
    let report = validate_registry(&doc);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let keys: Vec<String> = match &doc.0 {
        yaml_serde::Value::Mapping(m) => m.keys().filter_map(|k| k.as_str().map(String::from)).collect(),
        _ => panic!("not a map"),
    };
    for key in ["version", "settings", "machines", "nodes", "aliases", "harnesses"] {
        assert!(keys.iter().any(|k| k == key), "missing {key}");
    }
}

#[test]
fn tc04_an_absent_version_gives_one_bad_version_at_version() {
    let errors = report("nodes: {}\naliases: {}\n").errors;
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::BadVersion);
    assert_eq!(errors[0].path, "version");
    assert_eq!(errors[0].text, "Schema version 1 is the only version.");
}

#[test]
fn tc05_versions_0_2_and_text_one_give_bad_version_and_1_gives_none() {
    for bad in ["0", "2", "\"1\"", "1.0", "true", "[1]"] {
        let errors = report(&format!("version: {bad}\nnodes: {{}}\naliases: {{}}\n")).errors;
        assert_eq!(errors.len(), 1, "version {bad}: {errors:?}");
        assert_eq!(errors[0].code, ErrorCode::BadVersion, "version {bad}");
    }
    assert!(report(MINIMAL).errors.is_empty());
}

#[test]
fn tc06_an_unknown_top_level_key_is_unknown_field_at_its_key() {
    let errors = report(&format!("{MINIMAL}extra_thing: 1\n")).errors;
    assert_eq!(errors.len(), 1);
    assert_eq!((errors[0].code, errors[0].path.as_str()), (ErrorCode::UnknownField, "extra_thing"));
    assert_eq!(errors[0].text, "Field is not part of the schema.");
}

#[test]
fn tc07_unknown_fields_are_reported_with_the_full_path_and_none_is_skipped() {
    // The ASSUMPTION of the design: the walk keeps nested paths and reports every unknown key.
    let text = format!(
        "{MINIMAL}harnesses:\n  - name: a\n    sesion_header: x\n    agent_headr: y\n  - name: b\n    bogus: 1\nextra_one: 1\nextra_two: 2\n"
    );
    let paths: Vec<(String, ErrorCode)> = report(&text).errors.iter().map(|e| (e.path.clone(), e.code)).collect();
    let unknown = |p: &str| (p.to_string(), ErrorCode::UnknownField);
    assert_eq!(
        paths,
        vec![unknown("extra_one"), unknown("extra_two"), unknown("harnesses.0.agent_headr"), unknown("harnesses.0.sesion_header"), unknown("harnesses.1.bogus")]
    );
}

#[test]
fn tc07_a_harness_row_needs_a_name_and_a_wrong_type_names_the_found_kind() {
    let errors = report(&format!("{MINIMAL}harnesses:\n  - session_header: 5\n  - just text\n")).errors;
    let shown: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
    assert_eq!(
        shown,
        vec![
            "registry error missing_key at harnesses.0.name: Required field is missing.",
            "registry error bad_type at harnesses.0.session_header: Expected string, found integer.",
            "registry error bad_type at harnesses.1: Expected map, found string.",
        ]
    );
}

#[test]
fn tc08_three_unrelated_errors_give_three_sorted_lines_and_a_count_and_repeat_exactly() {
    let text = "version: 3\nnodes: []\naliases: {}\nzzz: 1\n";
    let first = lines(text);
    assert_eq!(
        first,
        vec![
            "registry error bad_type at nodes: Expected map, found list.",
            "registry error bad_version at version: Schema version 1 is the only version.",
            "registry error unknown_field at zzz: Field is not part of the schema.",
        ]
    );
    assert_eq!(first, lines(text), "the same file gives the same text");
    let mut out = Vec::new();
    print_errors(&report(text).errors, &mut out);
    let printed = String::from_utf8(out).unwrap();
    assert!(printed.ends_with("registry rejected: 3 errors\n"), "{printed}");
}

#[test]
fn tc08_one_error_uses_the_singular() {
    let mut out = Vec::new();
    print_errors(&report("nodes: {}\naliases: {}\n").errors, &mut out);
    assert!(String::from_utf8(out).unwrap().ends_with("registry rejected: 1 error\n"));
}

#[test]
fn tc09_empty_and_comments_only_files_give_errors_not_a_panic_and_no_guessed_version() {
    for text in ["", "# nothing but a comment\n", "---\n"] {
        let r = report(text);
        let shown: Vec<String> = r.errors.iter().map(|e| e.to_string()).collect();
        assert_eq!(
            shown,
            vec![
                "registry error missing_key at aliases: Required field is missing.",
                "registry error missing_key at nodes: Required field is missing.",
                "registry error bad_version at version: Schema version 1 is the only version.",
            ],
            "{text:?}"
        );
        assert!(r.warnings.is_empty());
    }
}

#[test]
fn tc09_a_file_that_is_a_list_or_text_is_a_wrong_type_at_the_file() {
    let errors = report("- a\n- b\n").errors;
    assert_eq!(errors.len(), 1);
    assert_eq!((errors[0].code, errors[0].text.as_str()), (ErrorCode::BadType, "Expected map, found list."));
}

struct Failing;
impl RegistryCheck for Failing {
    fn check(&self, _doc: &RawRegistry, out: &mut ValidationReport) {
        out.errors.push(error_fixed(ErrorCode::DuplicateName, "nodes.n1", "Name is used twice."));
    }
}

struct Panicking;
impl RegistryCheck for Panicking {
    fn check(&self, _doc: &RawRegistry, out: &mut ValidationReport) {
        out.errors.push(error_fixed(ErrorCode::BadUrl, "nodes.n1", "half written"));
        panic!("a check defect");
    }
}

struct Secret;
impl RegistryCheck for Secret {
    fn check(&self, _doc: &RawRegistry, out: &mut ValidationReport) {
        out.errors.push(error_fixed(ErrorCode::InlineSecret, "nodes.n1.api_key", "A secret value is not allowed in the file."));
    }
}

#[test]
fn tc11_every_check_runs_even_after_an_error_and_there_is_no_partial_record() {
    // The shape check fails on the last section; the extra check still runs and both are listed.
    let doc = parse_registry_text("version: 1\nnodes: {}\naliases: 5\n", PATH_LABEL).unwrap();
    let list: Vec<Box<dyn RegistryCheck>> = vec![Box::new(Failing)];
    let r = validate_with(&doc, &list);
    let paths: Vec<&str> = r.errors.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, vec!["aliases", "nodes.n1"]);
}

#[test]
fn tc11_a_panicking_check_fails_closed_with_one_internal_line_and_no_half_result() {
    let doc = parse_registry_text(MINIMAL, PATH_LABEL).unwrap();
    let list: Vec<Box<dyn RegistryCheck>> = vec![Box::new(Panicking), Box::new(Failing)];
    let r = validate_with(&doc, &list);
    let shown: Vec<String> = r.errors.iter().map(|e| e.code.as_str().to_string()).collect();
    assert_eq!(shown, vec!["internal_check_failed", "duplicate_name"]);
    assert!(!r.errors.iter().any(|e| e.code == ErrorCode::BadUrl), "the partial output of the failed check is dropped");
}

#[test]
fn tc15_a_check_added_to_the_list_appears_in_the_same_sorted_list() {
    let doc = parse_registry_text("version: 1\nnodes: {}\naliases: {}\nzzz: 1\n", PATH_LABEL).unwrap();
    let list: Vec<Box<dyn RegistryCheck>> = vec![Box::new(Failing)];
    let shown: Vec<String> = validate_with(&doc, &list).errors.iter().map(|e| e.to_string()).collect();
    assert_eq!(shown, vec!["registry error duplicate_name at nodes.n1: Name is used twice.", "registry error unknown_field at zzz: Field is not part of the schema."]);
}

#[test]
fn tc10_an_inline_secret_error_replaces_the_unknown_field_error_at_the_same_path() {
    let doc = parse_registry_text("version: 1\nnodes:\n  n1:\n    api_key: x\naliases: {}\n", PATH_LABEL).unwrap();
    let mut seeded = validate_with(&doc, &[]);
    seeded.errors.push(error_fixed(ErrorCode::UnknownField, "nodes.n1.api_key", "Field is not part of the schema."));
    let list: Vec<Box<dyn RegistryCheck>> = vec![Box::new(Secret)];
    // Real flow: both a shape error and the secret error exist at one path.
    struct Unknown;
    impl RegistryCheck for Unknown {
        fn check(&self, _d: &RawRegistry, out: &mut ValidationReport) {
            out.errors.push(error_fixed(ErrorCode::UnknownField, "nodes.n1.api_key", "Field is not part of the schema."));
        }
    }
    let both: Vec<Box<dyn RegistryCheck>> = vec![Box::new(Unknown), Box::new(Secret)];
    let at_path = |l: &[Box<dyn RegistryCheck>]| validate_with(&doc, l).errors.iter().filter(|e| e.path == "nodes.n1.api_key").map(|e| e.code).collect::<Vec<_>>();
    assert_eq!(at_path(&both), vec![ErrorCode::InlineSecret]);
    assert_eq!(at_path(&list), vec![ErrorCode::InlineSecret]);
}

#[test]
fn every_error_and_warning_code_has_its_contract_name() {
    let errors = [
        "file_unreadable", "file_unparsable", "missing_key", "unknown_field", "bad_version", "bad_type", "duplicate_name", "unknown_ref", "empty_alias",
        "no_common_protocol", "bad_url", "bad_slots", "context_missing", "bad_patch", "inline_secret", "bad_key_ref", "secret_unresolved",
        "secret_permissions", "hosted_needs_auth", "hosted_needs_client_tokens", "bad_settings", "bad_responses_flag", "bad_warm_capacity",
    ];
    let codes = [
        ErrorCode::FileUnreadable, ErrorCode::FileUnparsable, ErrorCode::MissingKey, ErrorCode::UnknownField, ErrorCode::BadVersion, ErrorCode::BadType,
        ErrorCode::DuplicateName, ErrorCode::UnknownRef, ErrorCode::EmptyAlias, ErrorCode::NoCommonProtocol, ErrorCode::BadUrl, ErrorCode::BadSlots,
        ErrorCode::ContextMissing, ErrorCode::BadPatch, ErrorCode::InlineSecret, ErrorCode::BadKeyRef, ErrorCode::SecretUnresolved,
        ErrorCode::SecretPermissions, ErrorCode::HostedNeedsAuth, ErrorCode::HostedNeedsClientTokens, ErrorCode::BadSettings,
        ErrorCode::BadResponsesFlag, ErrorCode::BadWarmCapacity,
    ];
    assert_eq!(codes.iter().map(|c| c.as_str()).collect::<Vec<_>>(), errors);
    let warnings = [
        (WarningCode::EngineVersionUntested, "engine_version_untested"), (WarningCode::FlagMismatch, "flag_mismatch"),
        (WarningCode::SlotsAboveMeasured, "slots_above_measured"), (WarningCode::ResponsesEngineMismatch, "responses_engine_mismatch"),
        (WarningCode::HostedIgnoresSlots, "hosted_ignores_slots"), (WarningCode::VllmNoPromptTokensDetails, "vllm_no_prompt_tokens_details"),
        (WarningCode::HoldLimitAboveBudget, "hold_limit_above_budget"), (WarningCode::RestartRequired, "restart_required"),
        (WarningCode::ProfileDamaged, "profile_damaged"),
    ];
    for (code, name) in warnings {
        assert_eq!(code.as_str(), name);
    }
}

#[test]
fn a_duplicate_key_is_duplicate_name_with_a_place_and_the_text_does_not_repeat_the_key() {
    let error = parse_registry_text("version: 1\nversion: 2\n", PATH_LABEL).expect_err("duplicate key");
    assert_eq!(error.code, ErrorCode::DuplicateName);
    assert!(error.text.starts_with("A name is used twice. At line "), "{}", error.text);
    assert!(!error.text.contains("\"version\""), "{}", error.text);
}
