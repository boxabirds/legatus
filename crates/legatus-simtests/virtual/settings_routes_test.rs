//! Story 165 unit tests: key map source kinds, the 512-byte source and route aliases.
use super::settings_helpers::*;
use legatus_proxy::config::routes::*;

fn with_routes(routes: &str) -> String {
    format!("{}routes:\n{routes}", registry(""))
}

const TWO_ROUTES: &str = "  - path: /v1/messages\n    key_map:\n      - header_pair: [x-claude-code-session-id, x-claude-code-agent-id]\n      - header: x-session-affinity\n      - body_hash\n  - path: /v1/responses\n    alias: x\n    key_map:\n      - header: session-id\n      - header: thread-id\n      - body_field: prompt_cache_key\n      - body_hash\n";

fn read(text: &str) -> Vec<RouteSpec> {
    let doc = legatus_proxy::config::read::parse_registry_text(text, PATH_LABEL).unwrap();
    let root = doc.0.as_mapping().unwrap();
    let aliases: Vec<String> = root.get("aliases").and_then(|a| a.as_mapping()).map(|m| m.keys().filter_map(|k| k.as_str().map(String::from)).collect()).unwrap_or_default();
    read_routes(root.get("routes"), &aliases, &mut legatus_proxy::config::registry::ValidationReport::default())
}

#[test]
fn tc18_the_six_source_kinds_pass_and_are_read_in_order() {
    let all = "  - path: /p\n    key_map:\n      - header: h\n      - header_pair: [a, b]\n      - body_field: f\n      - body_hash\n      - credential\n      - none\n";
    assert!(errors(&with_routes(all)).is_empty(), "{:?}", errors(&with_routes(all)));
    let routes = read(&with_routes(all));
    let kinds: Vec<KeySourceKind> = routes[0].key_map.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, vec![KeySourceKind::Header, KeySourceKind::HeaderPair, KeySourceKind::BodyField, KeySourceKind::BodyHash, KeySourceKind::Credential, KeySourceKind::None]);
    assert_eq!(routes[0].key_map[1].names, vec!["a", "b"]);
    assert_eq!(routes[0].scope, RouteScope::Alias);
}

#[test]
fn tc18_the_two_example_routes_of_the_spec_load() {
    assert!(errors(&with_routes(TWO_ROUTES)).is_empty());
    let routes = read(&with_routes(TWO_ROUTES));
    assert_eq!(routes.len(), 2);
    assert_eq!((routes[0].path.as_str(), routes[0].alias.clone()), ("/v1/messages", None));
    assert_eq!(routes[1].alias.as_ref().map(|a| a.0.as_str()), Some("x"));
    assert_eq!(routes[1].key_map[2], KeyMapEntry { kind: KeySourceKind::BodyField, names: vec!["prompt_cache_key".into()] });
}

#[test]
fn tc18_the_kind_cookie_hash_is_unknown_field_with_the_source_in_the_path() {
    let text = with_routes("  - path: /p\n    key_map:\n      - cookie_hash: session\n");
    assert_eq!(errors(&text), vec!["registry error unknown_field at routes.0.key_map.0.cookie_hash: Source kind is not in the list."]);
    assert!(read(&text).is_empty(), "no route is returned on error");
}

#[test]
fn tc19_a_512_byte_prefix_source_is_refused_and_no_key_is_built() {
    for form in ["      - body_prefix: 512\n", "      - body_prefix_512: true\n", "      - first_512_bytes: {}\n"] {
        let text = with_routes(&format!("  - path: /p\n    key_map:\n{form}"));
        let found = errors(&text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("registry error unknown_field at routes.0.key_map.0.") && found[0].ends_with("Source kind is not in the list."), "{found:?}");
        assert!(read(&text).is_empty());
    }
    let word = with_routes("  - path: /p\n    key_map:\n      - body_prefix\n");
    assert_eq!(errors(&word), vec!["registry error unknown_field at routes.0.key_map.0: Source kind is not in the list."], "a plain word is not echoed in the path");
}

#[test]
fn tc18_wrong_shapes_are_bad_type() {
    for (entry, path) in [("      - header\n", "routes.0.key_map.0"), ("      - header: [a]\n", "routes.0.key_map.0.header"), ("      - header_pair: [a]\n", "routes.0.key_map.0.header_pair"), ("      - 5\n", "routes.0.key_map.0")] {
        let found = errors(&with_routes(&format!("  - path: /p\n    key_map:\n{entry}")));
        assert_eq!(found.len(), 1, "{entry}: {found:?}");
        assert!(found[0].starts_with(&format!("registry error bad_type at {path}:")), "{found:?}");
    }
    assert_eq!(errors(&with_routes("  - path: /p\n    scope: wide\n")), vec!["registry error bad_type at routes.0.scope: Value is not one of the allowed values."]);
    assert!(errors(&with_routes("  - path: /p\n    scope: alias_plus_credential\n")).is_empty());
    assert_eq!(read(&with_routes("  - path: /p\n    scope: alias_plus_credential\n"))[0].scope, RouteScope::AliasPlusCredential);
}

#[test]
fn tc20_a_route_naming_a_missing_alias_is_unknown_ref_and_a_route_without_alias_is_valid() {
    assert_eq!(errors(&with_routes("  - path: /p\n    alias: ghost\n")), vec!["registry error unknown_ref at routes.0.alias: Name does not exist."]);
    assert!(errors(&with_routes("  - path: /p\n")).is_empty());
    assert!(errors(&with_routes("  - path: /p\n    alias: x\n")).is_empty());
    assert_eq!(errors(&with_routes("  - alias: x\n")), vec!["registry error missing_key at routes.0.path: Required field is missing."]);
    assert_eq!(errors(&with_routes("  - path: /p\n    colour: red\n")), vec!["registry error unknown_field at routes.0.colour: Field is not part of the schema."]);
}
