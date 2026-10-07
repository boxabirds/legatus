//! Shape check of the registry file: a schema table walked over a generic value tree, so every
//! unknown field is reported, not only the first (contract C10).
use crate::config::registry::*;
use legatus_common::ids::REGISTRY_SCHEMA_VERSION;
use yaml_serde::Value;

/// Path used for an error about the whole file.
pub const ROOT_PATH: &str = "(file)";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Map,
    List,
    Str,
    Int,
    Bool,
    Any,
}

impl Kind {
    fn expected_name(&self) -> &'static str {
        match self {
            Kind::Map => "map",
            Kind::List => "list",
            Kind::Str => "string",
            Kind::Int => "integer",
            Kind::Bool => "boolean",
            Kind::Any => "any value",
        }
    }
}

/// One row of the schema table. For a `Map`, `fields` lists the allowed keys; an empty list
/// means a later story owns the table and the children are not checked yet. For a `List`,
/// `fields` describes each item (a map); an empty list means the items are not checked yet.
#[derive(Clone, Copy)]
pub struct SchemaNode {
    pub kind: Kind,
    pub fields: &'static [(&'static str, SchemaNode)],
    pub required: bool,
    /// For a `Map` whose keys are names chosen by the owner (nodes, machines): the shape of
    /// every value. `fields` must be empty then.
    pub each: Option<&'static SchemaNode>,
}

const fn leaf(kind: Kind, required: bool) -> SchemaNode {
    SchemaNode { kind, fields: &[], required, each: None }
}

const fn table(kind: Kind, fields: &'static [(&'static str, SchemaNode)], required: bool) -> SchemaNode {
    SchemaNode { kind, fields, required, each: None }
}

const fn named(each: &'static SchemaNode, required: bool) -> SchemaNode {
    SchemaNode { kind: Kind::Map, fields: &[], required, each: Some(each) }
}

const ENDPOINT: &[(&str, SchemaNode)] = &[("protocol", leaf(Kind::Str, true)), ("base_url", leaf(Kind::Str, true))];

const ENGINE: &[(&str, SchemaNode)] = &[("name", leaf(Kind::Str, true)), ("version", leaf(Kind::Str, false))];

const CONTEXT: &[(&str, SchemaNode)] = &[("per_slot", leaf(Kind::Int, false)), ("on_overflow", leaf(Kind::Str, false))];

const CACHE: &[(&str, SchemaNode)] = &[("kind", leaf(Kind::Str, false)), ("prompt_tokens_details", leaf(Kind::Bool, false))];

const ENGINE_FLAGS: &[(&str, SchemaNode)] = &[
    ("jinja", leaf(Kind::Bool, false)),
    ("np", leaf(Kind::Int, false)),
    ("ctx_checkpoints", leaf(Kind::Int, false)),
    ("checkpoint_min_step", leaf(Kind::Int, false)),
    ("cache_ram_mib", leaf(Kind::Int, false)),
    ("num_parallel", leaf(Kind::Int, false)),
    ("keep_alive_s", leaf(Kind::Int, false)),
    ("kv_unified", leaf(Kind::Bool, false)),
    ("prefix_caching", leaf(Kind::Any, false)),
    ("speculative_decoding", leaf(Kind::Any, false)),
    ("ctx_size", leaf(Kind::Int, false)),
    ("vllm_block_size", leaf(Kind::Int, false)),
];

/// Fields of one node (contract C11). `patch` and `auth` are accepted as open maps; their
/// checks belong to stories 190 and 135.
const NODE: &[(&str, SchemaNode)] = &[
    ("engine", table(Kind::Map, ENGINE, true)),
    ("model", leaf(Kind::Str, true)),
    ("quantisation", leaf(Kind::Str, false)),
    ("machine", leaf(Kind::Str, false)),
    ("endpoints", table(Kind::List, ENDPOINT, true)),
    ("responses", leaf(Kind::Bool, false)),
    ("stateful_responses", leaf(Kind::Bool, false)),
    ("ignores_previous_response_id", leaf(Kind::Bool, false)),
    ("warm_capacity", leaf(Kind::Any, false)),
    ("paths", leaf(Kind::List, false)),
    ("slots", leaf(Kind::Any, false)),
    ("context", table(Kind::Map, CONTEXT, false)),
    ("cache", table(Kind::Map, CACHE, false)),
    ("engine_flags", table(Kind::Map, ENGINE_FLAGS, false)),
    ("patch", leaf(Kind::Map, false)),
    ("auth", leaf(Kind::Map, false)),
    ("always_on", leaf(Kind::Bool, false)),
    ("affinity_mode", leaf(Kind::Str, false)),
    ("cold_allowance_tokens", leaf(Kind::Int, false)),
];

const MACHINE: &[(&str, SchemaNode)] = &[("host", leaf(Kind::Str, false)), ("mem_gb", leaf(Kind::Any, false)), ("agent", leaf(Kind::Str, false))];

static NODE_ROW: SchemaNode = table(Kind::Map, NODE, false);
static MACHINE_ROW: SchemaNode = table(Kind::Map, MACHINE, false);

const HARNESS_ROW: &[(&str, SchemaNode)] = &[
    ("name", leaf(Kind::Str, true)),
    ("session_header", leaf(Kind::Str, false)),
    ("agent_header", leaf(Kind::Str, false)),
    ("never_key_headers", leaf(Kind::List, false)),
    ("title_alias", leaf(Kind::Str, false)),
    ("user_agent_prefix", leaf(Kind::Str, false)),
];

/// Top-level keys (contract C10). `version` is checked by `check_version`, so its row is `Any`.
/// Tables of settings, machines, nodes, aliases and routes are filled by stories 165, 136, 153.
pub const TOP_LEVEL: &[(&str, SchemaNode)] = &[
    ("version", leaf(Kind::Any, false)),
    ("settings", leaf(Kind::Map, false)),
    ("machines", named(&MACHINE_ROW, false)),
    ("nodes", named(&NODE_ROW, true)),
    ("aliases", leaf(Kind::Map, true)),
    ("routes", leaf(Kind::Any, false)),
    ("harnesses", table(Kind::List, HARNESS_ROW, false)),
];

pub fn join_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

fn kind_matches(kind: Kind, value: &Value) -> bool {
    match kind {
        Kind::Any => true,
        Kind::Map => matches!(value, Value::Mapping(_)),
        Kind::List => matches!(value, Value::Sequence(_)),
        Kind::Str => matches!(value, Value::String(_)),
        Kind::Int => matches!(value, Value::Number(n) if n.is_i64() || n.is_u64()),
        Kind::Bool => matches!(value, Value::Bool(_)),
    }
}

fn walk_map(map: &yaml_serde::Mapping, fields: &'static [(&'static str, SchemaNode)], path: &str, out: &mut ValidationReport) {
    for (position, (key, value)) in map.iter().enumerate() {
        let Value::String(name) = key else {
            out.errors.push(error_fixed(ErrorCode::UnknownField, &join_path(path, &format!("#{position}")), TEXT_UNKNOWN_FIELD));
            continue;
        };
        let child_path = join_path(path, name);
        match fields.iter().find(|(field, _)| field == name) {
            Some((_, node)) => walk(value, node, &child_path, out),
            None => out.errors.push(error_fixed(ErrorCode::UnknownField, &child_path, TEXT_UNKNOWN_FIELD)),
        }
    }
    for (field, node) in fields {
        if node.required && !map.contains_key(*field) {
            out.errors.push(error_fixed(ErrorCode::MissingKey, &join_path(path, field), TEXT_MISSING_KEY));
        }
    }
}

fn walk(value: &Value, node: &SchemaNode, path: &str, out: &mut ValidationReport) {
    if !kind_matches(node.kind, value) {
        out.errors.push(error_wrong_type(path, node.kind.expected_name(), FoundKind::of(value)));
        return;
    }
    match (node.kind, value) {
        (Kind::Map, Value::Mapping(map)) if !node.fields.is_empty() => walk_map(map, node.fields, path, out),
        (Kind::Map, Value::Mapping(map)) if node.each.is_some() => {
            let row = node.each.expect("checked by the guard");
            for (position, (key, entry)) in map.iter().enumerate() {
                let name = match key {
                    Value::String(name) => name.clone(),
                    _ => format!("#{position}"),
                };
                walk(entry, row, &join_path(path, &name), out);
            }
        }
        (Kind::List, Value::Sequence(items)) if !node.fields.is_empty() => {
            let item = table(Kind::Map, node.fields, false);
            for (index, entry) in items.iter().enumerate() {
                walk(entry, &item, &join_path(path, &index.to_string()), out);
            }
        }
        _ => {}
    }
}

/// Report every unknown field, missing required field and wrong type, at every depth.
/// An empty or comments-only file (a null value) is read as a map with no keys.
pub fn check_shape(tree: &Value, out: &mut ValidationReport) {
    match tree {
        Value::Mapping(map) => walk_map(map, TOP_LEVEL, "", out),
        Value::Null => walk_map(&yaml_serde::Mapping::new(), TOP_LEVEL, "", out),
        other => out.errors.push(error_wrong_type(ROOT_PATH, "map", FoundKind::of(other))),
    }
}

/// `bad_version` for an absent, non-number or not-1 version. One code answers PRX-REG-002.
pub fn check_version(tree: &Value, out: &mut ValidationReport) {
    let version = match tree {
        Value::Mapping(map) => map.get("version"),
        _ => None,
    };
    let ok = matches!(version, Some(Value::Number(n)) if n.as_u64() == Some(REGISTRY_SCHEMA_VERSION));
    if !ok && matches!(tree, Value::Mapping(_) | Value::Null) {
        out.errors.push(error_fixed(ErrorCode::BadVersion, "version", TEXT_BAD_VERSION));
    }
}
