//! Typed nodes and machines read from the registry value tree (contract C11, story 136).
//! The shape walk of `schema.rs` already reports unknown fields, missing required fields and
//! values of the wrong kind in listed fields. The functions here report only what the walk
//! cannot: values outside a list of words, ranges and cross-field rules.
use crate::config::registry::*;
use crate::config::schema::join_path;
use crate::engine::family_of;
use crate::engine::version::version_status;
use legatus_common::engine::EngineVersionStatus;
use legatus_common::ids::NodeId;
use yaml_serde::{Mapping, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineName {
    LlamaServer,
    Ollama,
    MlxLm,
    Vllm,
    Sglang,
    Gufo,
    Lmstudio,
    OpenaiCompatible,
    OpenaiHosted,
    AnthropicHosted,
}

impl EngineName {
    pub const ALL: [EngineName; 10] = [
        EngineName::LlamaServer,
        EngineName::Ollama,
        EngineName::MlxLm,
        EngineName::Vllm,
        EngineName::Sglang,
        EngineName::Gufo,
        EngineName::Lmstudio,
        EngineName::OpenaiCompatible,
        EngineName::OpenaiHosted,
        EngineName::AnthropicHosted,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            EngineName::LlamaServer => "llama-server",
            EngineName::Ollama => "ollama",
            EngineName::MlxLm => "mlx_lm",
            EngineName::Vllm => "vllm",
            EngineName::Sglang => "sglang",
            EngineName::Gufo => "gufo",
            EngineName::Lmstudio => "lmstudio",
            EngineName::OpenaiCompatible => "openai-compatible",
            EngineName::OpenaiHosted => "openai-hosted",
            EngineName::AnthropicHosted => "anthropic-hosted",
        }
    }

    pub fn parse(text: &str) -> Option<EngineName> {
        EngineName::ALL.into_iter().find(|e| e.as_str() == text)
    }

    /// The single test for a hosted engine (story 135 and admission use it).
    pub fn is_hosted(&self) -> bool {
        matches!(self, EngineName::OpenaiHosted | EngineName::AnthropicHosted)
    }
}

/// Engines whose slot count `auto` can read (PROPOSED; engine adapters may extend the list).
pub const ENGINES_WITH_SLOT_SIGNAL: &[EngineName] = &[EngineName::LlamaServer];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointProtocol {
    OpenaiChat,
    OpenaiResponses,
    AnthropicMessages,
}

impl EndpointProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            EndpointProtocol::OpenaiChat => "openai-chat",
            EndpointProtocol::OpenaiResponses => "openai-responses",
            EndpointProtocol::AnthropicMessages => "anthropic-messages",
        }
    }
    pub fn parse(text: &str) -> Option<EndpointProtocol> {
        [EndpointProtocol::OpenaiChat, EndpointProtocol::OpenaiResponses, EndpointProtocol::AnthropicMessages].into_iter().find(|p| p.as_str() == text)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub protocol: EndpointProtocol,
    pub base_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slots {
    Auto,
    Count(u32),
}

/// The declared setting only; the computed warm capacity is story 141's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarmCapacitySetting {
    Auto,
    Count(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriState {
    True,
    False,
    Unknown,
}

impl TriState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriState::True => "true",
            TriState::False => "false",
            TriState::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AffinityModeSetting {
    Auto,
    On,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnOverflow {
    Error400,
    SilentTruncate,
    Unbounded,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheKind {
    Dense,
    Hybrid,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineFlags {
    pub jinja: Option<bool>,
    pub np: Option<u32>,
    pub ctx_checkpoints: Option<u32>,
    pub checkpoint_min_step: Option<u32>,
    pub cache_ram_mib: Option<u32>,
    pub num_parallel: Option<u32>,
    pub keep_alive_s: Option<u32>,
    pub kv_unified: Option<bool>,
    pub prefix_caching: TriState,
    pub speculative_decoding: TriState,
    /// llama-server total context (`-c`).
    pub ctx_size: Option<u32>,
    pub vllm_block_size: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSpec {
    pub name: NodeId,
    pub engine: EngineName,
    pub engine_version: Option<String>,
    pub model: String,
    pub quantisation: Option<String>,
    pub machine: Option<String>,
    pub endpoints: Vec<Endpoint>,
    pub responses: bool,
    pub stateful_responses: bool,
    pub ignores_previous_response_id: bool,
    pub warm_capacity: WarmCapacitySetting,
    pub paths: Vec<String>,
    pub slots: Option<Slots>,
    pub context_per_slot: Option<u32>,
    pub on_overflow: Option<OnOverflow>,
    pub cache_kind: CacheKind,
    pub prompt_tokens_details: Option<bool>,
    pub engine_flags: EngineFlags,
    pub affinity_mode: AffinityModeSetting,
    pub cold_allowance_tokens: Option<u32>,
    pub always_on: bool,
    /// The request edits of the node as canonical JSON text (applied by story 190); kept so a
    /// reload can tell that they changed.
    pub patch: Option<String>,
    /// The reference name of the node key (`auth.key_ref`); never a value. Story 135 checks it.
    pub auth_key_ref: Option<RefText>,
}

/// A secret reference as written in the file (for example `env:NAME`). Its Debug output hides the
/// text, so a registry printed in a log cannot show it. Story 135 owns the grammar.
#[derive(Clone, PartialEq, Eq)]
pub struct RefText(String);

impl RefText {
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for RefText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RefText(..)")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MachineSpec {
    pub name: String,
    pub host: Option<String>,
    pub mem_gb: Option<f64>,
    pub agent: Option<String>,
}

pub const TEXT_NOT_ALLOWED_VALUE: &str = "Value is not one of the allowed values.";
pub const TEXT_BAD_URL: &str = "Address is not a valid http or https URL.";
pub const TEXT_BAD_SLOTS: &str = "Slots must be a whole number of 1 or more, or auto for an engine that reports its slots.";
pub const TEXT_UNKNOWN_MACHINE: &str = "Machine is not defined.";
pub const TEXT_CONTEXT_MISSING: &str = "Context size per slot is required when the engine silently truncates.";
pub const TEXT_BAD_WARM_CAPACITY: &str = "Warm capacity must be auto or a whole number of 1 or more.";
pub const TEXT_NEGATIVE_OR_LARGE: &str = "a whole number of 0 or more that fits in 32 bits";
pub const TEXT_DUPLICATE_NAME: &str = "A name is used twice.";

const ENGINE_NAME_FIELD: &str = "engine.name";

fn text<'a>(map: &'a Mapping, key: &str) -> Option<&'a str> {
    map.get(key).and_then(Value::as_str)
}

fn sub<'a>(map: &'a Mapping, key: &str) -> Option<&'a Mapping> {
    map.get(key).and_then(Value::as_mapping)
}

fn flag_value(map: &Mapping, key: &str) -> Option<bool> {
    map.get(key).and_then(Value::as_bool)
}

fn whole(map: &Mapping, key: &str, path: &str, out: &mut ValidationReport) -> Option<u32> {
    let value = map.get(key)?;
    let number = match value {
        Value::Number(n) if n.is_i64() || n.is_u64() => n,
        _ => return None, // the shape walk reported the kind
    };
    match number.as_u64().and_then(|n| u32::try_from(n).ok()) {
        Some(n) => Some(n),
        None => {
            out.errors.push(error_wrong_type(&join_path(path, key), TEXT_NEGATIVE_OR_LARGE, FoundKind::Int));
            None
        }
    }
}

/// A boolean flag; false when absent. A wrong kind is reported by the shape walk.
pub fn parse_flag(v: Option<&Value>, _path: &str, _out: &mut ValidationReport) -> bool {
    v.and_then(Value::as_bool).unwrap_or(false)
}

/// `true`, `false` or `unknown`; `Unknown` when absent. Any other value is a `bad_type`.
pub fn parse_tri_state(v: Option<&Value>, path: &str, out: &mut ValidationReport) -> TriState {
    match v {
        None => TriState::Unknown,
        Some(Value::Bool(true)) => TriState::True,
        Some(Value::Bool(false)) => TriState::False,
        Some(Value::String(word)) if word == "unknown" => TriState::Unknown,
        Some(other) => {
            out.errors.push(error_wrong_type(path, "true, false or unknown", FoundKind::of(other)));
            TriState::Unknown
        }
    }
}

/// A whole number of 1 or more, or `auto`; `Auto` when absent.
pub fn parse_warm_capacity(v: Option<&Value>, path: &str, out: &mut ValidationReport) -> WarmCapacitySetting {
    match v {
        None => WarmCapacitySetting::Auto,
        Some(Value::String(word)) if word == "auto" => WarmCapacitySetting::Auto,
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => match n.as_u64().and_then(|x| u32::try_from(x).ok()) {
            Some(count) if count >= 1 => WarmCapacitySetting::Count(count),
            _ => {
                out.errors.push(error_fixed(ErrorCode::BadWarmCapacity, path, TEXT_BAD_WARM_CAPACITY));
                WarmCapacitySetting::Auto
            }
        },
        Some(other) => {
            out.errors.push(error_wrong_type(path, "a whole number or auto", FoundKind::of(other)));
            WarmCapacitySetting::Auto
        }
    }
}

fn parse_slots(v: Option<&Value>, engine: EngineName, path: &str, out: &mut ValidationReport) -> Option<Slots> {
    match v? {
        Value::String(word) if word == "auto" => {
            if ENGINES_WITH_SLOT_SIGNAL.contains(&engine) {
                Some(Slots::Auto)
            } else {
                out.errors.push(error_fixed(ErrorCode::BadSlots, path, TEXT_BAD_SLOTS));
                None
            }
        }
        Value::Number(n) if n.is_i64() || n.is_u64() => match n.as_u64().and_then(|x| u32::try_from(x).ok()) {
            Some(count) if count >= 1 => Some(Slots::Count(count)),
            _ => {
                out.errors.push(error_fixed(ErrorCode::BadSlots, path, TEXT_BAD_SLOTS));
                None
            }
        },
        other => {
            out.errors.push(error_wrong_type(path, "a whole number or auto", FoundKind::of(other)));
            None
        }
    }
}

fn parse_word<T: Copy>(map: &Mapping, key: &str, path: &str, words: &[(&str, T)], out: &mut ValidationReport) -> Option<T> {
    let value = map.get(key)?;
    let word = value.as_str()?; // a non-string is reported by the shape walk
    match words.iter().find(|(w, _)| *w == word) {
        Some((_, found)) => Some(*found),
        None => {
            out.errors.push(error_fixed(ErrorCode::BadType, &join_path(path, key), TEXT_NOT_ALLOWED_VALUE));
            None
        }
    }
}

fn parse_url_ok(url: &str) -> bool {
    match url.parse::<http::Uri>() {
        Ok(uri) => matches!(uri.scheme_str(), Some("http") | Some("https")) && uri.authority().is_some(),
        Err(_) => false,
    }
}

fn read_endpoints(node: &Mapping, path: &str, out: &mut ValidationReport) -> Option<Vec<Endpoint>> {
    let list = node.get("endpoints")?.as_sequence()?;
    let mut endpoints = Vec::new();
    let mut ok = true;
    for (index, entry) in list.iter().enumerate() {
        let Some(map) = entry.as_mapping() else {
            ok = false;
            continue;
        };
        let entry_path = join_path(&join_path(path, "endpoints"), &index.to_string());
        let protocol = parse_word(
            map,
            "protocol",
            &entry_path,
            &[("openai-chat", EndpointProtocol::OpenaiChat), ("openai-responses", EndpointProtocol::OpenaiResponses), ("anthropic-messages", EndpointProtocol::AnthropicMessages)],
            out,
        );
        let base_url = text(map, "base_url");
        if let Some(url) = base_url {
            if !parse_url_ok(url) {
                out.errors.push(error_fixed(ErrorCode::BadUrl, &join_path(&entry_path, "base_url"), TEXT_BAD_URL));
                ok = false;
            }
        }
        match (protocol, base_url) {
            (Some(protocol), Some(url)) if parse_url_ok(url) => endpoints.push(Endpoint { protocol, base_url: url.to_string() }),
            _ => ok = false,
        }
    }
    ok.then_some(endpoints)
}

fn read_engine_flags(node: &Mapping, path: &str, out: &mut ValidationReport) -> EngineFlags {
    let flags_path = join_path(path, "engine_flags");
    let empty = Mapping::new();
    let map = sub(node, "engine_flags").unwrap_or(&empty);
    EngineFlags {
        jinja: flag_value(map, "jinja"),
        np: whole(map, "np", &flags_path, out),
        ctx_checkpoints: whole(map, "ctx_checkpoints", &flags_path, out),
        checkpoint_min_step: whole(map, "checkpoint_min_step", &flags_path, out),
        cache_ram_mib: whole(map, "cache_ram_mib", &flags_path, out),
        num_parallel: whole(map, "num_parallel", &flags_path, out),
        keep_alive_s: whole(map, "keep_alive_s", &flags_path, out),
        kv_unified: flag_value(map, "kv_unified"),
        prefix_caching: parse_tri_state(map.get("prefix_caching"), &join_path(&flags_path, "prefix_caching"), out),
        speculative_decoding: parse_tri_state(map.get("speculative_decoding"), &join_path(&flags_path, "speculative_decoding"), out),
        ctx_size: whole(map, "ctx_size", &flags_path, out),
        vllm_block_size: whole(map, "vllm_block_size", &flags_path, out),
    }
}

fn read_node(name: &str, map: &Mapping, machines: &[String], out: &mut ValidationReport) -> Option<NodeSpec> {
    let errors_before = out.errors.len();
    let path = join_path("nodes", name);
    let engine_map = sub(map, "engine");
    let engine_name = engine_map.and_then(|e| text(e, "name")).and_then(|word| match EngineName::parse(word) {
        Some(engine) => Some(engine),
        None => {
            out.errors.push(error_fixed(ErrorCode::BadType, &join_path(&path, ENGINE_NAME_FIELD), TEXT_NOT_ALLOWED_VALUE));
            None
        }
    });
    let endpoints = read_endpoints(map, &path, out);
    let context = sub(map, "context");
    let per_slot = context.and_then(|c| whole(c, "per_slot", &join_path(&path, "context"), out));
    let on_overflow = context.and_then(|c| {
        parse_word(
            c,
            "on_overflow",
            &join_path(&path, "context"),
            &[("error_400", OnOverflow::Error400), ("silent_truncate", OnOverflow::SilentTruncate), ("unbounded", OnOverflow::Unbounded), ("unknown", OnOverflow::Unknown)],
            out,
        )
    });
    if on_overflow == Some(OnOverflow::SilentTruncate) && per_slot.is_none() {
        out.errors.push(error_fixed(ErrorCode::ContextMissing, &join_path(&join_path(&path, "context"), "per_slot"), TEXT_CONTEXT_MISSING));
    }
    let cache = sub(map, "cache");
    let cache_kind = cache
        .and_then(|c| parse_word(c, "kind", &join_path(&path, "cache"), &[("dense", CacheKind::Dense), ("hybrid", CacheKind::Hybrid), ("unknown", CacheKind::Unknown)], out))
        .unwrap_or(CacheKind::Unknown);
    let machine = text(map, "machine").map(str::to_string);
    if let Some(m) = &machine {
        if !machines.contains(m) {
            out.errors.push(error_fixed(ErrorCode::UnknownRef, &join_path(&path, "machine"), TEXT_UNKNOWN_MACHINE));
        }
    }
    let affinity_mode = parse_word(map, "affinity_mode", &path, &[("auto", AffinityModeSetting::Auto), ("on", AffinityModeSetting::On), ("off", AffinityModeSetting::Off)], out)
        .unwrap_or(AffinityModeSetting::Auto);
    let engine = engine_name;
    let slots = engine.and_then(|e| parse_slots(map.get("slots"), e, &join_path(&path, "slots"), out));
    let warm_capacity = parse_warm_capacity(map.get("warm_capacity"), &join_path(&path, "warm_capacity"), out);
    let flags = read_engine_flags(map, &path, out);
    let cold_allowance_tokens = whole(map, "cold_allowance_tokens", &path, out);
    let mut paths = Vec::new();
    if let Some(list) = map.get("paths").and_then(Value::as_sequence) {
        for (index, item) in list.iter().enumerate() {
            match item.as_str() {
                Some(p) => paths.push(p.to_string()),
                None => out.errors.push(error_wrong_type(&join_path(&join_path(&path, "paths"), &index.to_string()), "string", FoundKind::of(item))),
            }
        }
    }
    let model = text(map, "model")?.to_string();
    let engine = engine?;
    let endpoints = endpoints?;
    if out.errors.len() != errors_before {
        return None;
    }
    Some(NodeSpec {
        name: NodeId(name.to_string()),
        engine,
        engine_version: engine_map.and_then(|e| text(e, "version")).map(str::to_string),
        model,
        quantisation: text(map, "quantisation").map(str::to_string),
        machine,
        endpoints,
        responses: parse_flag(map.get("responses"), &path, out),
        stateful_responses: parse_flag(map.get("stateful_responses"), &path, out),
        ignores_previous_response_id: parse_flag(map.get("ignores_previous_response_id"), &path, out),
        warm_capacity,
        paths,
        slots,
        context_per_slot: per_slot,
        on_overflow,
        cache_kind,
        prompt_tokens_details: cache.and_then(|c| flag_value(c, "prompt_tokens_details")),
        engine_flags: flags,
        affinity_mode,
        cold_allowance_tokens,
        always_on: flag_value(map, "always_on").unwrap_or(true),
        patch: map.get("patch").and_then(|p| serde_json::to_string(p).ok()),
        auth_key_ref: sub(map, "auth").and_then(|a| text(a, "key_ref")).map(|t| RefText(t.to_string())),
    })
}

/// Read every node. A node with an error is not returned. Errors are appended to `out`.
pub fn read_nodes(tree: &Value, out: &mut ValidationReport) -> Vec<NodeSpec> {
    let Some(nodes) = tree.as_mapping().and_then(|root| sub(root, "nodes")) else {
        return Vec::new();
    };
    let machines: Vec<String> = tree.as_mapping().and_then(|root| sub(root, "machines")).map(|m| m.keys().filter_map(|k| k.as_str().map(str::to_string)).collect()).unwrap_or_default();
    nodes
        .iter()
        .filter_map(|(key, value)| {
            let name = key.as_str()?;
            let map = value.as_mapping()?;
            read_node(name, map, &machines, out)
        })
        .collect()
}

/// Read every machine. A machine with a wrong-typed `mem_gb` is reported and not returned.
pub fn read_machines(tree: &Value, out: &mut ValidationReport) -> Vec<MachineSpec> {
    let Some(machines) = tree.as_mapping().and_then(|root| sub(root, "machines")) else {
        return Vec::new();
    };
    let mut list = Vec::new();
    for (key, value) in machines {
        let (Some(name), Some(map)) = (key.as_str(), value.as_mapping()) else { continue };
        let path = join_path("machines", name);
        let mem_gb = match map.get("mem_gb") {
            None => None,
            Some(Value::Number(n)) => n.as_f64(),
            Some(other) => {
                out.errors.push(error_wrong_type(&join_path(&path, "mem_gb"), "number", FoundKind::of(other)));
                continue;
            }
        };
        list.push(MachineSpec { name: name.to_string(), host: text(map, "host").map(str::to_string), mem_gb, agent: text(map, "agent").map(str::to_string) });
    }
    list
}

/// What the admin read shows of one node (the wire `NodeView` is story 159). No auth value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeConfigView {
    pub name: NodeId,
    pub engine: &'static str,
    pub engine_version: Option<String>,
    /// Tested, untested or unknown. An untested node still serves.
    pub engine_version_status: EngineVersionStatus,
    pub responses: bool,
    pub stateful_responses: bool,
    pub ignores_previous_response_id: bool,
    pub warm_capacity: WarmCapacitySetting,
    pub prefix_caching: TriState,
    pub speculative_decoding: TriState,
    pub warnings: Vec<WarningCode>,
}

pub fn node_config_views(nodes: &[NodeSpec], warnings: &[RegistryWarning]) -> Vec<NodeConfigView> {
    nodes
        .iter()
        .map(|n| {
            let prefix = format!("nodes.{}.", n.name.0);
            NodeConfigView {
                name: n.name.clone(),
                engine: n.engine.as_str(),
                engine_version: n.engine_version.clone(),
                engine_version_status: version_status(family_of(n.engine.as_str()), n.engine_version.as_deref()),
                responses: n.responses,
                stateful_responses: n.stateful_responses,
                ignores_previous_response_id: n.ignores_previous_response_id,
                warm_capacity: n.warm_capacity,
                prefix_caching: n.engine_flags.prefix_caching,
                speculative_decoding: n.engine_flags.speculative_decoding,
                warnings: warnings.iter().filter(|w| w.path.starts_with(&prefix)).map(|w| w.code).collect(),
            }
        })
        .collect()
}
