//! Story 126 unit tests: header reading, the keyed digest, the map walk, the default orders, the
//! never-key headers, the harness table and the map choice. Pure functions over headers.
use http::{HeaderMap, HeaderName, HeaderValue};
use legatus_common::protocol::Protocol;
use legatus_proxy::config::alias::AliasTable;
use legatus_proxy::config::routes::{KeyMapEntry, KeySourceKind};
use legatus_proxy::config::typed::{HarnessRowSpec, Registry};
use legatus_proxy::key::choose_map;
use legatus_proxy::key::harness::*;
use legatus_proxy::key::hasher::*;
use legatus_proxy::key::sources::*;
use legatus_proxy::obs::log_sink::{DiscardSink, LogRecord};
use legatus_testkit::virt::MemorySink;
use std::cell::Cell;
use std::sync::Arc;

const SINK_CAPACITY: usize = 64;
const LONG_HEADER_BYTES: usize = 10 * 1024;
const SECRET_A: [u8; SECRET_LEN_BYTES] = [1; SECRET_LEN_BYTES];
const SECRET_B: [u8; SECRET_LEN_BYTES] = [2; SECRET_LEN_BYTES];

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (n, v) in pairs {
        h.append(HeaderName::from_bytes(n.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap());
    }
    h
}

fn hasher() -> KeyHasher {
    KeyHasher::new(&KeySecret::from_bytes(SECRET_A))
}

/// A body that counts how often it is asked and offers fields.
#[derive(Default)]
struct CountingBody {
    asked: Cell<usize>,
    fields: Vec<(&'static str, &'static str)>,
}

impl BodyKeySource for CountingBody {
    fn body_field(&self, name: &str) -> Option<Vec<u8>> {
        self.asked.set(self.asked.get() + 1);
        self.fields.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_bytes().to_vec())
    }
    fn derived(&self) -> Option<ConversationKey> {
        self.asked.set(self.asked.get() + 1);
        None
    }
}

fn resolve(map: &KeyMap, h: &HeaderMap) -> KeyResolution {
    let table = HarnessTable::default();
    resolve_key(map, h, &CountingBody::default(), &hasher(), &WarnOnce::new(Arc::new(DiscardSink)), &table)
}

fn default_chat() -> KeyMap {
    KeyMap::default_for(Protocol::OpenAiChat, &HarnessTable::default())
}

#[test]
fn tc01_x_session_affinity_gives_a_key_and_the_body_is_not_asked() {
    let body = CountingBody::default();
    let table = HarnessTable::default();
    let r = resolve_key(&default_chat(), &headers(&[("x-session-affinity", "abc")]), &body, &hasher(), &WarnOnce::new(Arc::new(DiscardSink)), &table);
    assert!(r.key.is_some());
    assert_eq!(r.source, SourceUsed::Header("x-session-affinity".into()));
    assert_eq!(r.source.label(), "header:x-session-affinity");
    assert_eq!(body.asked.get(), 0);
}

#[test]
fn tc02_and_tc17_an_empty_or_white_space_only_header_is_absent_and_the_next_source_is_used() {
    for empty in ["", " ", " \t ", "\t"] {
        let h = headers(&[("x-session-affinity", empty), ("x-session-id", "ses_1")]);
        assert_eq!(read_header(&h, "x-session-affinity"), HeaderRead::Absent, "{empty:?}");
        assert_eq!(resolve(&default_chat(), &h).source, SourceUsed::Header("x-session-id".into()));
    }
    assert_eq!(read_header(&headers(&[]), "x-session-id"), HeaderRead::Absent);
    assert_eq!(read_header(&headers(&[("x", "a")]), "x"), HeaderRead::Present { value: b"a".to_vec(), repeated_differs: false }, "a single character");
}

#[test]
fn tc03_a_ten_kib_header_value_gives_a_key_of_the_same_length_and_the_same_digest_size() {
    let long = "s".repeat(LONG_HEADER_BYTES);
    let r = resolve(&default_chat(), &headers(&[("x-session-affinity", &long)]));
    let short = resolve(&default_chat(), &headers(&[("x-session-affinity", "s")]));
    assert_eq!(r.key.unwrap().0.len(), KEY_LEN_BYTES);
    assert_eq!(short.key.unwrap().0.len(), KEY_LEN_BYTES);
    assert_ne!(r.key, short.key);
}

#[test]
fn tc04_the_header_name_matches_in_any_case() {
    let upper = HeaderMap::from_iter([(HeaderName::from_static("x-session-affinity"), HeaderValue::from_static("abc"))]);
    let a = resolve(&default_chat(), &upper);
    let mut mixed = HeaderMap::new();
    mixed.insert(HeaderName::from_bytes(b"X-Session-Affinity").unwrap(), HeaderValue::from_static("abc"));
    assert_eq!(resolve(&default_chat(), &mixed).key, a.key);
    // A map that spells the name in capitals finds the same header.
    let map = KeyMap { sources: vec![KeySource::Header("X-SESSION-AFFINITY".into())], warn_missing: true };
    assert!(resolve(&map, &mixed).key.is_some());
}

#[test]
fn tc05_a_repeated_header_uses_the_first_line_and_counts_one_warning_without_the_value() {
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let warn = WarnOnce::new(sink.clone());
    let table = HarnessTable::default();
    let h = headers(&[("x-session-affinity", "first-value"), ("x-session-affinity", "second-value")]);
    assert_eq!(read_header(&h, "x-session-affinity"), HeaderRead::Present { value: b"first-value".to_vec(), repeated_differs: true });
    let one = resolve_key(&default_chat(), &h, &CountingBody::default(), &hasher(), &warn, &table);
    let alone = resolve(&default_chat(), &headers(&[("x-session-affinity", "first-value")]));
    assert_eq!(one.key, alone.key, "the second line plays no part");
    assert_eq!((warn.repeat_count(), sink.len()), (1, 1));
    resolve_key(&default_chat(), &h, &CountingBody::default(), &hasher(), &warn, &table);
    assert_eq!((warn.repeat_count(), sink.len()), (2, 1), "the counter moves, no second event");
    let shown = format!("{:?}", sink.take());
    assert!(!shown.contains("first-value") && !shown.contains("second-value") && shown.contains("key_header_repeat"), "{shown}");
    // Equal lines are not a repeat that differs.
    let same = headers(&[("x-session-affinity", "v"), ("x-session-affinity", "v")]);
    assert_eq!(read_header(&same, "x-session-affinity"), HeaderRead::Present { value: b"v".to_vec(), repeated_differs: false });
}

#[test]
fn tc06_spaces_around_the_value_are_trimmed_and_give_the_same_key_as_the_bare_value() {
    let bare = resolve(&default_chat(), &headers(&[("x-session-affinity", "abc")]));
    let padded = resolve(&default_chat(), &headers(&[("x-session-affinity", "  abc\t")]));
    assert_eq!(bare.key, padded.key);
}

#[test]
fn tc07_and_tc16_two_sources_present_the_earlier_in_the_map_decides_and_nothing_is_merged() {
    let both = headers(&[("x-session-affinity", "a"), ("x-claude-code-session-id", "b")]);
    let r = resolve(&default_chat(), &both);
    assert_eq!(r.source, SourceUsed::Header("x-session-affinity".into()));
    assert_eq!(r.key, resolve(&default_chat(), &headers(&[("x-session-affinity", "a")])).key);
    let reversed = KeyMap { sources: vec![KeySource::Header("x-claude-code-session-id".into()), KeySource::Header("x-session-affinity".into())], warn_missing: true };
    assert_eq!(resolve(&reversed, &both).source, SourceUsed::Header("x-claude-code-session-id".into()));
}

#[test]
fn tc08_the_pair_source_joins_session_and_agent_and_uses_the_session_alone_when_the_agent_is_absent() {
    let one = |agent: Option<&str>| {
        let mut pairs = vec![("x-claude-code-session-id", "S1")];
        if let Some(a) = agent {
            pairs.push(("x-claude-code-agent-id", a));
        }
        resolve(&default_chat(), &headers(&pairs))
    };
    let (main, sub_a, sub_b) = (one(None), one(Some("agent-a")), one(Some("agent-b")));
    assert_ne!(sub_a.key, sub_b.key, "two agents of one session are two conversations");
    assert_ne!(main.key, sub_a.key);
    assert_eq!(main.key, one(None).key);
    assert!(matches!(main.source, SourceUsed::HeaderPair(..)));
}

#[test]
fn tc09_a_map_that_names_a_header_no_request_carries_falls_back_and_warns_once_per_start() {
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let warn = WarnOnce::new(sink.clone());
    let table = HarnessTable::default();
    let map = KeyMap::from_alias(&["x-my-session".to_string()], true);
    let body = CountingBody::default();
    for _ in 0..5 {
        let r = resolve_key(&map, &headers(&[]), &body, &hasher(), &warn, &table);
        assert_eq!(r, KeyResolution { key: None, source: SourceUsed::None });
    }
    assert_eq!(sink.len(), 1, "one warning for five requests");
    assert!(format!("{:?}", sink.take()).contains("key_header_missing"));
    let other = KeyMap::from_alias(&["x-other".to_string()], false);
    resolve_key(&other, &headers(&[]), &body, &hasher(), &warn, &table);
    assert_eq!(sink.len(), 1, "a second header warns once for itself");
    // A new start (a new set) warns again.
    let fresh = WarnOnce::new(sink.clone());
    resolve_key(&map, &headers(&[]), &body, &hasher(), &fresh, &table);
    assert_eq!(sink.len(), 2);
    // The default map names many headers nobody sends and warns for none of them.
    let quiet = Arc::new(MemorySink::new(SINK_CAPACITY));
    resolve_key(&default_chat(), &headers(&[]), &body, &hasher(), &WarnOnce::new(quiet.clone()), &table);
    assert!(quiet.is_empty());
}

#[test]
fn tc11_and_tc13_a_responses_request_reads_session_id_then_thread_id_then_the_body_field_and_the_window_id_never_matters() {
    let map = KeyMap::default_for(Protocol::OpenAiResponses, &HarnessTable::default());
    let body = CountingBody { fields: vec![("prompt_cache_key", "pck")], ..Default::default() };
    let table = HarnessTable::default();
    let go = |h: &HeaderMap| resolve_key(&map, h, &body, &hasher(), &WarnOnce::new(Arc::new(DiscardSink)), &table);
    let with_both = go(&headers(&[("session-id", "S"), ("thread-id", "T")]));
    assert_eq!(with_both.source, SourceUsed::Header("session-id".into()));
    assert_eq!(go(&headers(&[("thread-id", "T")])).source, SourceUsed::Header("thread-id".into()));
    assert_eq!(go(&headers(&[])).source, SourceUsed::BodyField("prompt_cache_key".into()));
    let a = go(&headers(&[("session-id", "S"), ("x-codex-window-id", "w1")]));
    let b = go(&headers(&[("session-id", "S"), ("x-codex-window-id", "w2")]));
    assert_eq!(a.key, b.key, "a window id that changes every request changes nothing");
}

#[test]
fn tc14_when_a_header_gives_the_key_the_body_is_asked_zero_times() {
    let map = KeyMap::default_for(Protocol::OpenAiResponses, &HarnessTable::default());
    let body = CountingBody { fields: vec![("prompt_cache_key", "pck")], ..Default::default() };
    resolve_key(&map, &headers(&[("session-id", "S")]), &body, &hasher(), &WarnOnce::new(Arc::new(DiscardSink)), &HarnessTable::default());
    assert_eq!(body.asked.get(), 0);
    resolve_key(&map, &headers(&[]), &body, &hasher(), &WarnOnce::new(Arc::new(DiscardSink)), &HarnessTable::default());
    assert!(body.asked.get() >= 1, "it is asked when no header gives a key");
}

#[test]
fn tc12_and_tc15_the_never_key_headers_sent_alone_give_no_key_and_a_copy_of_the_session_in_a_request_id_is_not_read() {
    let table = HarnessTable::default();
    let never = table.never_key_headers();
    for header in FIXED_NEVER_KEY_HEADERS {
        assert!(never.contains(&header.to_string()), "{header}");
        for protocol in [Protocol::OpenAiChat, Protocol::AnthropicMessages, Protocol::OpenAiResponses] {
            let map = KeyMap::default_for(protocol, &table);
            assert_eq!(resolve(&map, &headers(&[(header, "value")])).key, None, "{header} alone must not be a key");
        }
    }
    let pi = headers(&[("x-session-affinity", "abc"), ("x-client-request-id", "abc")]);
    assert_eq!(resolve(&default_chat(), &pi).source, SourceUsed::Header("x-session-affinity".into()));
    // An operator may list one explicitly.
    let listed = KeyMap { sources: vec![KeySource::Header("x-client-request-id".into())], warn_missing: true };
    assert!(resolve(&listed, &headers(&[("x-client-request-id", "r")])).key.is_some());
}

#[test]
fn tc19_the_digest_has_a_fixed_length_is_stable_changes_with_one_byte_and_with_the_secret() {
    let h = hasher();
    let k = h.hash(b"domain", b"value");
    assert_eq!(k.0.len(), KEY_LEN_BYTES);
    assert_eq!(k, h.hash(b"domain", b"value"));
    assert_ne!(k, h.hash(b"domain", b"valuf"));
    assert_ne!(k, h.hash(b"domaim", b"value"));
    assert_ne!(k, KeyHasher::new(&KeySecret::from_bytes(SECRET_B)).hash(b"domain", b"value"));
    assert_eq!(format!("{k:?}"), "ConversationKey(..)");
    assert_eq!(format!("{:?}", KeySecret::from_bytes(SECRET_A)), "KeySecret(..)");
    assert_eq!((KEY_LEN_BYTES, SECRET_LEN_BYTES, KEY_VERSION_TAG, FIELD_SEPARATOR), (16, 32, "v1", 0x1F));
}

#[test]
fn tc20_a_map_that_lists_thread_id_and_prompt_cache_key_reads_them_in_order() {
    let map = KeyMap { sources: vec![KeySource::Header("thread-id".into()), KeySource::BodyField("prompt_cache_key".into())], warn_missing: true };
    let table = HarnessTable::default();
    let body = CountingBody { fields: vec![("prompt_cache_key", "pck")], ..Default::default() };
    let go = |h: &HeaderMap| resolve_key(&map, h, &body, &hasher(), &WarnOnce::new(Arc::new(DiscardSink)), &table);
    assert_eq!(go(&headers(&[("thread-id", "T")])).source, SourceUsed::Header("thread-id".into()));
    assert_eq!(go(&headers(&[])).source, SourceUsed::BodyField("prompt_cache_key".into()));
}

#[test]
fn tc21_each_rank_of_the_default_order_sent_alone_gives_its_key_the_lower_rank_wins_and_the_credential_is_last() {
    let ranks = [("x-session-affinity", "x-session-affinity"), ("x-session-id", "x-session-id"), ("x-claude-code-session-id", "x-claude-code-session-id"), ("session-id", "session-id"), ("x-openwebui-chat-id", "x-openwebui-chat-id")];
    for (i, (header, expected)) in ranks.iter().enumerate() {
        let alone = resolve(&default_chat(), &headers(&[(header, "v")]));
        assert!(alone.source.label().ends_with(expected), "{header}: {:?}", alone.source);
        if let Some((lower, _)) = ranks.get(i + 1) {
            let both = resolve(&default_chat(), &headers(&[(header, "v"), (lower, "w")]));
            assert_eq!(both.key, alone.key, "{header} beats {lower}");
        }
    }
    for protocol in [Protocol::OpenAiChat, Protocol::AnthropicMessages, Protocol::OpenAiResponses] {
        let map = KeyMap::default_for(protocol, &HarnessTable::default());
        assert_eq!(map.sources.last(), Some(&KeySource::Credential), "{protocol:?}");
        assert_eq!(map.sources[map.sources.len() - 2], KeySource::BodyHash);
    }
    assert_eq!(resolve(&default_chat(), &headers(&[])), KeyResolution { key: None, source: SourceUsed::Credential }, "the walk hands over to the credential source");
}

#[test]
fn tc22_the_harness_table_prefers_a_session_header_then_a_user_agent_prefix_then_unknown() {
    let table = HarnessTable::default();
    let known = |s: &str| HarnessLabel::Known(s.to_string());
    assert_eq!(table.classify(&headers(&[("x-claude-code-session-id", "s"), ("user-agent", "pi/1.0")])), known("claude-code"), "the session header wins over the user agent");
    assert_eq!(table.classify(&headers(&[("user-agent", "codex_cli_rs/0.1")])), known("codex"));
    assert_eq!(table.classify(&headers(&[("user-agent", "CLAUDE-CLI/2.0")])), known("claude-code"), "case is ignored");
    assert_eq!(table.classify(&headers(&[("user-agent", "curl/8")])), HarnessLabel::Unknown);
    assert_eq!(table.classify(&headers(&[])), HarnessLabel::Unknown);
    // The prefix is looked for in the first 256 bytes only.
    let far = format!("{}pi", "x".repeat(USER_AGENT_PREFIX_SCAN_BYTES));
    assert_eq!(table.classify(&headers(&[("user-agent", &far)])), HarnessLabel::Unknown);
    // Rows from the registry replace the built-in ones.
    let custom = HarnessTable::from_specs(&[HarnessRowSpec { name: "mine".into(), session_header: Some("x-mine".into()), agent_header: None, never_key_headers: vec!["X-Noise".into()], title_alias: Some("title".into()), user_agent_prefix: None }]);
    assert_eq!(custom.classify(&headers(&[("x-mine", "1")])), known("mine"));
    assert!(custom.never_key_headers().contains(&"x-noise".to_string()));
    assert!(custom.is_title_alias(&known("mine"), &legatus_common::ids::AliasName("title".into())));
    assert!(!custom.is_title_alias(&HarnessLabel::Unknown, &legatus_common::ids::AliasName("title".into())));
    assert_eq!(HarnessLabel::Unknown.as_str(), "unknown");
}

fn registry(alias_extra: &str, route: &str) -> Registry {
    let text = format!("version: 1\nsettings:\n  listen: 127.0.0.1:8080\nnodes:\n  a:\n    engine: {{ name: ollama }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://a.invalid\" }} ]\naliases:\n  x: {{ nodes: [a]{alias_extra} }}\n{route}");
    Registry::from_text(&text).expect("valid registry")
}

fn alias_of<'a>(r: &'a Registry) -> &'a legatus_proxy::config::alias::AliasSpec {
    let _: &AliasTable = &r.aliases;
    r.aliases.iter().next().expect("one alias")
}

#[test]
fn the_route_map_wins_over_the_alias_shorthand_and_the_shorthand_over_the_default() {
    let table = HarnessTable::default();
    let plain = registry("", "");
    assert_eq!(choose_map(&plain.routes, "/v1/chat/completions", alias_of(&plain), Protocol::OpenAiChat, &table), default_chat());
    let short = registry(", affinity: { key_headers: [x-mine], hash_fallback: true }", "");
    let m = choose_map(&short.routes, "/v1/chat/completions", alias_of(&short), Protocol::OpenAiChat, &table);
    assert_eq!(m.sources, vec![KeySource::Header("x-mine".into()), KeySource::BodyHash]);
    let no_fallback = registry(", affinity: { key_headers: [x-mine], hash_fallback: false }", "");
    assert_eq!(choose_map(&no_fallback.routes, "/v1/chat/completions", alias_of(&no_fallback), Protocol::OpenAiChat, &table).sources, vec![KeySource::Header("x-mine".into())]);
    let routed = registry(", affinity: { key_headers: [x-mine] }", "routes:\n  - path: /v1/chat/completions\n    alias: x\n    key_map: [ { header: x-route } ]\n");
    let m = choose_map(&routed.routes, "/v1/chat/completions", alias_of(&routed), Protocol::OpenAiChat, &table);
    assert_eq!(m.sources, vec![KeySource::Header("x-route".into())]);
    // A path with no route falls through to the alias.
    let other = choose_map(&routed.routes, "/other", alias_of(&routed), Protocol::OpenAiChat, &table);
    assert_eq!(other.sources[0], KeySource::Header("x-mine".into()));
    // The kinds a route can name are all carried.
    let all = KeyMap::from_route(&[
        KeyMapEntry { kind: KeySourceKind::HeaderPair, names: vec!["a".into(), "b".into()] },
        KeyMapEntry { kind: KeySourceKind::BodyField, names: vec!["f".into()] },
        KeyMapEntry { kind: KeySourceKind::BodyHash, names: vec![] },
        KeyMapEntry { kind: KeySourceKind::Credential, names: vec![] },
        KeyMapEntry { kind: KeySourceKind::None, names: vec![] },
    ]);
    assert_eq!(all.sources.len(), 5);
}

#[test]
fn an_empty_map_and_no_headers_leave_the_key_empty_without_an_error_and_a_none_source_stops_the_walk() {
    let empty = KeyMap { sources: vec![], warn_missing: true };
    assert_eq!(resolve(&empty, &headers(&[("x-session-affinity", "a")])), KeyResolution { key: None, source: SourceUsed::None });
    let stop = KeyMap { sources: vec![KeySource::None, KeySource::Header("x-session-affinity".into())], warn_missing: true };
    assert_eq!(resolve(&stop, &headers(&[("x-session-affinity", "a")])).key, None);
    assert_eq!(SourceUsed::None.label(), "none");
    assert_eq!(SourceUsed::BodyHash.label(), "derived");
    let _ = LogRecord::System;
}
