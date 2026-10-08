//! The settings catalogue (contract C15, story 165): the one place that defines a setting name,
//! unit, default, range, status and restart flag, and the code that reads the settings map,
//! checks it and exposes the effective values.
use crate::config::alias::AliasTable;
use crate::config::registry::*;
use crate::config::schema::join_path;
use crate::config::validate::RegistryCheck;
use std::collections::{BTreeMap, BTreeSet};
use yaml_serde::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Seconds,
    Milliseconds,
    Bytes,
    Tokens,
    Count,
    Ratio,
    HttpStatus,
    Address,
    Path,
    SecretRef,
    TagList,
}

/// A = approved by the owner (the value can still change after a measurement), P = PROPOSED,
/// PD = PENDING owner confirmation, NS = NOT SET in the spec and PROPOSED.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Approved,
    Proposed,
    Pending,
    NotSet,
}

impl Status {
    pub fn code(&self) -> &'static str {
        match self {
            Status::Approved => "A",
            Status::Proposed => "P",
            Status::Pending => "PD",
            Status::NotSet => "NS",
        }
    }
}

/// Where an effective value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    File,
    Default,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DefaultValue {
    Int(u64),
    Float(f64),
    Text(&'static str),
    List(&'static [&'static str]),
    Off,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bound {
    None,
    Int(u64),
    Float(f64),
    /// A relation to another setting.
    Setting(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct SettingDef {
    pub name: &'static str,
    pub unit: Unit,
    pub default: DefaultValue,
    pub min: Bound,
    pub max: Bound,
    pub min_exclusive: bool,
    pub max_exclusive: bool,
    pub status: Status,
    pub restart: bool,
    pub owner_story: &'static str,
    pub spec_ids: &'static str,
}

/// Warn when a hold limit is above this many seconds (PROPOSED, PRX-ADM-043). 290 loads quietly.
pub const HOLD_LIMIT_WARN_S: u32 = 290;
/// The hold-limit answer that is refused at load.
pub const HOLD_STATUS_REFUSED: u16 = 429;
pub const HOLD_STATUS_503: u16 = 503;
pub const HOLD_STATUS_504: u16 = 504;
pub const HOLD_STATUS_529: u16 = 529;
/// The answers a hold limit may use (chat and Messages; the Responses path always answers 503).
pub const HOLD_STATUS_ALLOWED: [u16; 3] = [HOLD_STATUS_503, HOLD_STATUS_504, HOLD_STATUS_529];

/// The allowed values of `hold_limit_status`; stories 160 and 187 import it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldLimitStatus {
    S503,
    S504,
    S529,
}

impl HoldLimitStatus {
    pub fn code(&self) -> u16 {
        match self {
            HoldLimitStatus::S503 => HOLD_STATUS_503,
            HoldLimitStatus::S504 => HOLD_STATUS_504,
            HoldLimitStatus::S529 => HOLD_STATUS_529,
        }
    }
    pub fn from_code(code: u64) -> Option<HoldLimitStatus> {
        [HoldLimitStatus::S503, HoldLimitStatus::S504, HoldLimitStatus::S529].into_iter().find(|s| u64::from(s.code()) == code)
    }
}

/// Fixed constants: not settings. Each has an owner story.
pub const ADMIN_TOKEN_POLL_S: u32 = 2;
pub const STATE_SOCKET_MAX_CONNS: u32 = 8;
pub const DASHBOARD_REFRESH_S: u32 = 5;
pub const DASHBOARD_WINDOW_S: u32 = 3600;
pub const DASHBOARD_RECENT_DECISIONS: u32 = 20;
pub const USAGE_TAIL_BYTES: u32 = 65536;
pub const AGENT_STALE_AFTER_INTERVALS: u32 = 3;
pub const VLLM_REUSE_MEASURE_FLOOR_TOKENS: u32 = 528;
pub const HOLD_LIMIT_DEFAULT_S: u32 = 250;
pub const MODEL_LIST_CREATED: u32 = 0;
pub const RESPONSE_ID_SCAN_LIMIT_BYTES: u32 = 16384;
pub const UNKNOWN_ENGINE_CAP: u32 = 1;
pub const LLAMA_SERVER_DEFAULT_SLOTS: u32 = 4;
pub const OLLAMA_DEFAULT_SLOTS: u32 = 1;
pub const MLX_LM_DEFAULT_SLOTS: u32 = 1;

pub struct FixedConstant {
    pub name: &'static str,
    pub value: u32,
    pub owner_story: &'static str,
}

pub const FIXED_CONSTANTS: &[FixedConstant] = &[
    FixedConstant { name: "ADMIN_TOKEN_POLL_S", value: ADMIN_TOKEN_POLL_S, owner_story: "159" },
    FixedConstant { name: "STATE_SOCKET_MAX_CONNS", value: STATE_SOCKET_MAX_CONNS, owner_story: "159" },
    FixedConstant { name: "DASHBOARD_REFRESH_S", value: DASHBOARD_REFRESH_S, owner_story: "173" },
    FixedConstant { name: "DASHBOARD_WINDOW_S", value: DASHBOARD_WINDOW_S, owner_story: "173" },
    FixedConstant { name: "DASHBOARD_RECENT_DECISIONS", value: DASHBOARD_RECENT_DECISIONS, owner_story: "173" },
    FixedConstant { name: "USAGE_TAIL_BYTES", value: USAGE_TAIL_BYTES, owner_story: "171" },
    FixedConstant { name: "AGENT_STALE_AFTER_INTERVALS", value: AGENT_STALE_AFTER_INTERVALS, owner_story: "181" },
    FixedConstant { name: "VLLM_REUSE_MEASURE_FLOOR_TOKENS", value: VLLM_REUSE_MEASURE_FLOOR_TOKENS, owner_story: "183" },
    FixedConstant { name: "HOLD_LIMIT_DEFAULT_S", value: HOLD_LIMIT_DEFAULT_S, owner_story: "119" },
    FixedConstant { name: "MODEL_LIST_CREATED", value: MODEL_LIST_CREATED, owner_story: "150" },
    FixedConstant { name: "RESPONSE_ID_SCAN_LIMIT_BYTES", value: RESPONSE_ID_SCAN_LIMIT_BYTES, owner_story: "137" },
    FixedConstant { name: "UNKNOWN_ENGINE_CAP", value: UNKNOWN_ENGINE_CAP, owner_story: "127" },
    FixedConstant { name: "LLAMA_SERVER_DEFAULT_SLOTS", value: LLAMA_SERVER_DEFAULT_SLOTS, owner_story: "151" },
    FixedConstant { name: "OLLAMA_DEFAULT_SLOTS", value: OLLAMA_DEFAULT_SLOTS, owner_story: "174" },
    FixedConstant { name: "MLX_LM_DEFAULT_SLOTS", value: MLX_LM_DEFAULT_SLOTS, owner_story: "183" },
];

// Defaults and bounds that carry meaning, named once.
const DEFAULT_WRAPPER_TAGS: &[&str] = &["<environment_context>", "<system-reminder>", "<user_instructions>", "<skills_instructions>"];
const COLD_ALLOWANCE_TOKENS_DEFAULT: u64 = 516;
const MAX_HELD_BYTES_DEFAULT: u64 = 268_435_456;
const MAX_HELD_BYTES_MIN: u64 = 1_048_576;
const MAX_HELD_BYTES_MAX: u64 = 17_179_869_184;
const BODY_LIMIT_BYTES_DEFAULT: u64 = 33_554_432;
const LOG_PAUSE_FREE_BYTES_DEFAULT: u64 = 1_073_741_824;
const LOG_RESUME_FREE_BYTES_DEFAULT: u64 = 2_147_483_648;
const SECONDS_PER_DAY: u64 = 86_400;
const SECONDS_PER_HOUR: u64 = 3_600;
const UPSTREAM_IDLE_REUSE_LIMIT_S: u64 = 5;
const STATE_DIR_DEFAULT: &str = "legatus-state";
const ADMIN_LISTEN_DEFAULT: &str = "127.0.0.1:8081";

const fn def(
    name: &'static str,
    unit: Unit,
    default: DefaultValue,
    range: (Bound, Bound),
    status: Status,
    restart: bool,
    origin: (&'static str, &'static str),
) -> SettingDef {
    let (owner_story, spec_ids) = origin;
    let (min, max) = range;
    SettingDef { name, unit, default, min, max, min_exclusive: false, max_exclusive: false, status, restart, owner_story, spec_ids }
}

const fn exclusive_min(mut d: SettingDef) -> SettingDef {
    d.min_exclusive = true;
    d
}

const fn exclusive_max(mut d: SettingDef) -> SettingDef {
    d.max_exclusive = true;
    d
}

/// Every setting of the registry, declared once. A change of a default is a change here only.
/// The catalogue default of a whole-number setting; 0 for a name that has none.
pub fn default_int(name: &str) -> u64 {
    match CATALOGUE.iter().find(|d| d.name == name).map(|d| &d.default) {
        Some(DefaultValue::Int(n)) => *n,
        _ => 0,
    }
}

/// The catalogue defaults of the affinity table: (`table_ttl_s`, `table_cap`, `mature_turns`).
pub fn table_defaults() -> (u64, u64, u64) {
    (default_int("table_ttl_s"), default_int("table_cap"), default_int("mature_turns"))
}

pub const CATALOGUE: &[SettingDef] = &[
    def("listen", Unit::Address, DefaultValue::None, (Bound::None, Bound::None), Status::Approved, true, ("121", "PRX-REG-048")),
    exclusive_min(def("hold_limit_s", Unit::Seconds, DefaultValue::Int(HOLD_LIMIT_DEFAULT_S as u64), (Bound::Setting("protected_window_s"), Bound::Int(SECONDS_PER_HOUR)), Status::Proposed, false, ("187", "PRX-ADM-014, PRX-ADM-043"))),
    def("protected_window_s", Unit::Seconds, DefaultValue::Int(180), (Bound::Int(10), Bound::Int(SECONDS_PER_HOUR)), Status::Proposed, false, ("124", "PRX-ADM-040")),
    def("probation_window_s", Unit::Seconds, DefaultValue::Int(30), (Bound::Int(0), Bound::Setting("protected_window_s")), Status::Proposed, false, ("143", "PRX-ADM-040")),
    def("mature_turns", Unit::Count, DefaultValue::Int(2), (Bound::Int(1), Bound::Int(10)), Status::Pending, false, ("143", "PRX-ADM-040")),
    def("max_held", Unit::Count, DefaultValue::Int(64), (Bound::Int(1), Bound::Int(1024)), Status::Proposed, false, ("178", "PRX-ADM-040")),
    def("max_held_bytes", Unit::Bytes, DefaultValue::Int(MAX_HELD_BYTES_DEFAULT), (Bound::Int(MAX_HELD_BYTES_MIN), Bound::Int(MAX_HELD_BYTES_MAX)), Status::Proposed, false, ("178", "PRX-ADM-040")),
    def("hold_limit_status", Unit::HttpStatus, DefaultValue::Int(HOLD_STATUS_503 as u64), (Bound::None, Bound::None), Status::Approved, false, ("187", "PRX-ADM-044")),
    def("hold_retry_after_s", Unit::Seconds, DefaultValue::Int(30), (Bound::Int(0), Bound::Int(30)), Status::Pending, false, ("187", "PRX-ADM-044")),
    def("table_ttl_s", Unit::Seconds, DefaultValue::Int(600), (Bound::Int(300), Bound::Int(SECONDS_PER_DAY)), Status::Proposed, false, ("143", "PRX-AFF-008, PRX-REG-048")),
    def("table_cap", Unit::Count, DefaultValue::Int(10_000), (Bound::Int(100), Bound::Int(1_000_000)), Status::Proposed, false, ("143", "PRX-AFF-008")),
    def("spill_after_s", Unit::Seconds, DefaultValue::Off, (Bound::Int(0), Bound::Setting("hold_limit_s")), Status::Proposed, false, ("178", "PRX-ADM-040")),
    def("feedback_window", Unit::Count, DefaultValue::Int(20), (Bound::Int(5), Bound::Int(200)), Status::Proposed, false, ("194", "PRX-ADM-040")),
    def("feedback_min_turns", Unit::Count, DefaultValue::Int(10), (Bound::Int(3), Bound::Setting("feedback_window")), Status::Proposed, false, ("194", "PRX-ADM-040")),
    def("feedback_min_tokens", Unit::Tokens, DefaultValue::Int(2048), (Bound::None, Bound::None), Status::Proposed, false, ("194", "PRX-ADM-040")),
    def("reuse_floor", Unit::Ratio, DefaultValue::Float(0.20), (Bound::Float(0.05), Bound::Float(0.9)), Status::Proposed, false, ("194", "PRX-ADM-040")),
    def("reprobe_after_s", Unit::Seconds, DefaultValue::Int(600), (Bound::Int(60), Bound::Int(SECONDS_PER_DAY)), Status::Proposed, false, ("194", "PRX-ADM-040")),
    def("key_text_limit_system", Unit::Bytes, DefaultValue::Int(32_768), (Bound::Int(256), Bound::Int(262_144)), Status::Proposed, false, ("126", "PRX-KEY-031, PRX-KEY-046")),
    def("key_text_limit_first", Unit::Bytes, DefaultValue::Int(8_192), (Bound::Int(256), Bound::Int(65_536)), Status::Proposed, false, ("126", "PRX-KEY-031")),
    def("wrapper_tags", Unit::TagList, DefaultValue::List(DEFAULT_WRAPPER_TAGS), (Bound::None, Bound::None), Status::Proposed, false, ("126", "PRX-KEY-054")),
    def("body_limit_bytes", Unit::Bytes, DefaultValue::Int(BODY_LIMIT_BYTES_DEFAULT), (Bound::None, Bound::None), Status::NotSet, false, ("160", "PRX-PROTO-065")),
    def("cold_allowance_tokens", Unit::Tokens, DefaultValue::Int(COLD_ALLOWANCE_TOKENS_DEFAULT), (Bound::Int(0), Bound::None), Status::Proposed, false, ("124", "PRX-ADM-040")),
    def("probe_budget_s", Unit::Seconds, DefaultValue::Int(600), (Bound::None, Bound::None), Status::Proposed, false, ("147", "PRX-ADM-040")),
    def("ready_health_wait_s", Unit::Seconds, DefaultValue::Int(5), (Bound::None, Bound::None), Status::Proposed, false, ("119", "PRX-ADM-040")),
    def("node_probe_interval_s", Unit::Seconds, DefaultValue::Int(2), (Bound::None, Bound::None), Status::Proposed, false, ("175", "PRX-ADM-040")),
    exclusive_max(def("upstream_idle_reuse_max_s", Unit::Seconds, DefaultValue::Int(4), (Bound::None, Bound::Int(UPSTREAM_IDLE_REUSE_LIMIT_S)), Status::Proposed, false, ("151", "PRX-ADM-040"))),
    def("truncation_report_ratio", Unit::Ratio, DefaultValue::Float(0.25), (Bound::None, Bound::None), Status::Proposed, false, ("174", "PRX-ADM-040")),
    def("ollama_truncation_limit_ratio", Unit::Ratio, DefaultValue::Float(1.0), (Bound::Float(0.1), Bound::Float(1.0)), Status::Proposed, false, ("174", "PRX-ADM-040")),
    def("ollama_bytes_per_token", Unit::Count, DefaultValue::Int(3), (Bound::Int(1), Bound::Int(16)), Status::Proposed, false, ("174", "PRX-ADM-040")),
    def("log_dir", Unit::Path, DefaultValue::None, (Bound::None, Bound::None), Status::NotSet, true, ("122", "PRX-ADM-040")),
    def("log_check_interval_s", Unit::Seconds, DefaultValue::Int(10), (Bound::None, Bound::None), Status::Proposed, false, ("130", "PRX-ADM-040")),
    def("log_pause_free_bytes", Unit::Bytes, DefaultValue::Int(LOG_PAUSE_FREE_BYTES_DEFAULT), (Bound::None, Bound::None), Status::Proposed, false, ("130", "PRX-ADM-040")),
    exclusive_min(def("log_resume_free_bytes", Unit::Bytes, DefaultValue::Int(LOG_RESUME_FREE_BYTES_DEFAULT), (Bound::Setting("log_pause_free_bytes"), Bound::None), Status::Proposed, false, ("130", "PRX-ADM-040"))),
    def("log_queue_capacity", Unit::Count, DefaultValue::Int(8192), (Bound::None, Bound::None), Status::NotSet, true, ("144", "PRX-ADM-040")),
    def("state_dir", Unit::Path, DefaultValue::Text(STATE_DIR_DEFAULT), (Bound::None, Bound::None), Status::Proposed, true, ("170", "PRX-ADM-040")),
    def("admin.listen", Unit::Address, DefaultValue::Text(ADMIN_LISTEN_DEFAULT), (Bound::None, Bound::None), Status::NotSet, true, ("159", "PRX-OBS-040")),
    def("admin.token_file", Unit::Path, DefaultValue::None, (Bound::None, Bound::None), Status::Approved, true, ("159", "PRX-OBS-040")),
    def("client_tokens_ref", Unit::SecretRef, DefaultValue::None, (Bound::None, Bound::None), Status::Approved, false, ("135", "PRX-SEC-001")),
];

pub const TEXT_OUT_OF_RANGE: &str = "Value is outside the allowed range.";
pub const TEXT_STATUS_REFUSED: &str = "Status 429 is refused; use 503, 504 or 529.";
pub const TEXT_STATUS_NOT_ALLOWED: &str = "Status must be 503, 504 or 529.";
pub const TEXT_BAD_ADDRESS: &str = "Address must be a host and port.";
pub const TEXT_HOLD_BUDGET: &str = "Hold limit is above the budget; many harnesses give up near 300 seconds.";
const TEXT_ABOVE: &str = "Value must be above";
const TEXT_AT_LEAST: &str = "Value cannot be lower than";
const TEXT_BELOW: &str = "Value must be below";
const TEXT_AT_MOST: &str = "Value cannot be higher than";

fn find(name: &str) -> Option<&'static SettingDef> {
    CATALOGUE.iter().find(|d| d.name == name)
}

/// What a catalogue row holds once read.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Int(u64),
    Float(f64),
    Text(String),
    List(Vec<String>),
    Off,
    Absent,
}

impl SettingValue {
    fn from_default(d: &DefaultValue) -> SettingValue {
        match d {
            DefaultValue::Int(n) => SettingValue::Int(*n),
            DefaultValue::Float(f) => SettingValue::Float(*f),
            DefaultValue::Text(t) => SettingValue::Text((*t).to_string()),
            DefaultValue::List(l) => SettingValue::List(l.iter().map(|s| (*s).to_string()).collect()),
            DefaultValue::Off => SettingValue::Off,
            DefaultValue::None => SettingValue::Absent,
        }
    }

    fn number(&self) -> Option<f64> {
        match self {
            SettingValue::Int(n) => Some(*n as f64),
            SettingValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn show(&self) -> String {
        match self {
            SettingValue::Int(n) => n.to_string(),
            SettingValue::Float(f) => format!("{f}"),
            SettingValue::Text(t) => t.clone(),
            SettingValue::List(l) => l.join(", "),
            SettingValue::Off => "off".to_string(),
            SettingValue::Absent => "none".to_string(),
        }
    }
}

/// The values in use, one typed field per catalogue row. Every `_s` field is `u32` seconds.
#[derive(Clone, Debug)]
pub struct EffectiveSettings {
    pub listen: Option<String>,
    pub hold_limit_s: u32,
    pub protected_window_s: u32,
    pub probation_window_s: u32,
    pub mature_turns: u32,
    pub max_held: u32,
    pub max_held_bytes: u64,
    hold_limit_status: Option<HoldLimitStatus>,
    pub hold_retry_after_s: u32,
    pub table_ttl_s: u32,
    pub table_cap: u32,
    /// `None` is the word `off`.
    pub spill_after_s: Option<u32>,
    pub feedback_window: u32,
    pub feedback_min_turns: u32,
    pub feedback_min_tokens: u32,
    pub reuse_floor: f64,
    pub reprobe_after_s: u32,
    pub key_text_limit_system: u32,
    pub key_text_limit_first: u32,
    pub wrapper_tags: Vec<String>,
    pub body_limit_bytes: u64,
    pub cold_allowance_tokens: u32,
    pub probe_budget_s: u32,
    pub ready_health_wait_s: u32,
    pub node_probe_interval_s: u32,
    pub upstream_idle_reuse_max_s: u32,
    pub truncation_report_ratio: f64,
    pub ollama_truncation_limit_ratio: f64,
    pub ollama_bytes_per_token: u32,
    pub log_dir: Option<String>,
    pub log_check_interval_s: u32,
    pub log_pause_free_bytes: u64,
    pub log_resume_free_bytes: u64,
    pub log_queue_capacity: u32,
    pub state_dir: String,
    pub admin_listen: String,
    pub admin_token_file: Option<String>,
    pub client_tokens_ref: Option<String>,
    alias_hold: BTreeMap<String, u32>,
    resolved: Vec<(&'static SettingDef, SettingValue, Source)>,
    /// Start-only settings whose declared value differs from the running one, as text.
    declared_pending: Vec<(&'static str, String)>,
}

impl EffectiveSettings {
    /// The value of every setting by name, for comparison across a reload.
    pub fn values(&self) -> Vec<(&'static str, String)> {
        self.resolved.iter().map(|(d, v, _)| (d.name, v.show())).collect()
    }

    /// Start-only settings whose new declared value waits for a restart (name, declared value).
    pub fn pending_restart(&self) -> &[(&'static str, String)] {
        &self.declared_pending
    }

    /// Keep the running value of every setting that binds at start. Returns the names whose
    /// declared value differs; the declared values are kept for the admin read.
    pub fn pin_restart_keys(&mut self, running: &EffectiveSettings) -> Vec<&'static str> {
        let mut changed = Vec::new();
        self.declared_pending.clear();
        for def in CATALOGUE.iter().filter(|d| d.restart) {
            let old = running.value(def.name).clone();
            let new = self.value(def.name).clone();
            if old == new {
                continue;
            }
            changed.push(def.name);
            self.declared_pending.push((def.name, new.show()));
            if let Some(entry) = self.resolved.iter_mut().find(|(d, _, _)| d.name == def.name) {
                entry.1 = old.clone();
            }
            self.restore_typed(def.name, running);
        }
        changed
    }

    fn restore_typed(&mut self, name: &str, running: &EffectiveSettings) {
        match name {
            "listen" => self.listen = running.listen.clone(),
            "log_dir" => self.log_dir = running.log_dir.clone(),
            "log_queue_capacity" => self.log_queue_capacity = running.log_queue_capacity,
            "state_dir" => self.state_dir = running.state_dir.clone(),
            "admin.listen" => self.admin_listen = running.admin_listen.clone(),
            "admin.token_file" => self.admin_token_file = running.admin_token_file.clone(),
            _ => {}
        }
    }

    /// The hold limit of an alias: its own value, else the global one.
    pub fn hold_limit_s(&self, alias: &legatus_common::ids::AliasName) -> u32 {
        self.alias_hold.get(&alias.0).copied().unwrap_or(self.hold_limit_s)
    }

    /// `None` when the file does not set it; the catalogue default 503 then applies.
    pub fn hold_limit_status(&self) -> Option<HoldLimitStatus> {
        self.hold_limit_status
    }

    fn value(&self, name: &str) -> &SettingValue {
        self.resolved.iter().find(|(d, _, _)| d.name == name).map(|(_, v, _)| v).unwrap_or(&SettingValue::Absent)
    }

    fn int(&self, name: &str) -> u64 {
        match self.value(name) {
            SettingValue::Int(n) => *n,
            _ => 0,
        }
    }
}

fn expected_kind(def: &SettingDef) -> &'static str {
    match def.unit {
        Unit::Ratio => "number",
        Unit::Address | Unit::Path | Unit::SecretRef => "string",
        Unit::TagList => "list of strings",
        _ if matches!(def.default, DefaultValue::Off) => "a whole number or off",
        _ => "whole number",
    }
}

const TEXT_WHOLE_NUMBER_32: &str = "a whole number of 0 or more that fits in 32 bits";

/// Read one file value against its row. `Err` has already pushed the error.
fn read_value(def: &SettingDef, value: &Value, path: &str, out: &mut ValidationReport) -> Result<SettingValue, ()> {
    let wrong = |out: &mut ValidationReport| {
        out.errors.push(error_wrong_type(path, expected_kind(def), FoundKind::of(value)));
        Err(())
    };
    match def.unit {
        Unit::Ratio => match value {
            Value::Number(n) => n.as_f64().map(SettingValue::Float).ok_or(()).or_else(|_| wrong(out)),
            _ => wrong(out),
        },
        Unit::Address | Unit::Path | Unit::SecretRef => match value {
            Value::String(s) => Ok(SettingValue::Text(s.clone())),
            _ => wrong(out),
        },
        Unit::TagList => match value {
            Value::Sequence(items) if items.iter().all(Value::is_string) => Ok(SettingValue::List(items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect())),
            _ => wrong(out),
        },
        _ => match value {
            Value::String(word) if word == "off" && matches!(def.default, DefaultValue::Off) => Ok(SettingValue::Off),
            Value::Number(n) if n.is_i64() || n.is_u64() => match n.as_u64() {
                Some(v) if def.unit == Unit::Bytes || u32::try_from(v).is_ok() => Ok(SettingValue::Int(v)),
                _ => {
                    out.errors.push(error_wrong_type(path, TEXT_WHOLE_NUMBER_32, FoundKind::Int));
                    Err(())
                }
            },
            _ => wrong(out),
        },
    }
}

fn bound_number(b: &Bound) -> Option<f64> {
    match b {
        Bound::Int(n) => Some(*n as f64),
        Bound::Float(f) => Some(*f),
        _ => None,
    }
}

/// Absolute bounds (numbers). Relations to other settings are checked later.
fn in_absolute_range(def: &SettingDef, v: f64) -> bool {
    let above_min = match bound_number(&def.min) {
        Some(min) => v > min || (!def.min_exclusive && v == min),
        None => true,
    };
    let below_max = match bound_number(&def.max) {
        Some(max) => v < max || (!def.max_exclusive && v == max),
        None => true,
    };
    above_min && below_max
}

fn setting_path(name: &str) -> String {
    join_path("settings", name)
}

/// Read, default and check the settings map. `None` when any error was raised.
pub fn read_settings(tree: Option<&Value>, aliases: &AliasTable, out: &mut ValidationReport) -> Option<EffectiveSettings> {
    let errors_before = out.errors.len();
    let map = tree.and_then(Value::as_mapping);
    let mut file: BTreeMap<&'static str, SettingValue> = BTreeMap::new();
    let mut invalid: BTreeSet<&'static str> = BTreeSet::new();
    if let Some(map) = map {
        for (key, value) in map {
            let Some(name) = key.as_str() else { continue };
            if name == "admin" {
                read_admin(value, &mut file, &mut invalid, out);
                continue;
            }
            match find(name) {
                Some(def) if !name.contains('.') => match read_value(def, value, &setting_path(name), out) {
                    Ok(v) => {
                        file.insert(def.name, v);
                    }
                    Err(()) => {
                        invalid.insert(def.name);
                    }
                },
                _ => out.errors.push(error_fixed(ErrorCode::UnknownField, &setting_path(name), TEXT_UNKNOWN_FIELD)),
            }
        }
        if !map.contains_key("listen") {
            out.errors.push(error_fixed(ErrorCode::MissingKey, &setting_path("listen"), TEXT_MISSING_KEY));
        }
    }
    let mut resolved: Vec<(&'static SettingDef, SettingValue, Source)> = Vec::new();
    for def in CATALOGUE {
        let (value, source) = match file.get(def.name) {
            Some(v) => (v.clone(), Source::File),
            None => (SettingValue::from_default(&def.default), Source::Default),
        };
        resolved.push((def, value, source));
    }
    for (def, value, source) in &resolved {
        if *source == Source::File && !invalid.contains(def.name) {
            check_file_value(def, value, &mut invalid, out);
        }
    }
    check_relations(&resolved, &invalid, out);
    if let Some(addr) = file.get("listen") {
        check_address("listen", addr, out, &mut invalid);
    }
    if let Some(addr) = file.get("admin.listen") {
        check_address("admin.listen", addr, out, &mut invalid);
    }
    let settings = build(resolved, aliases, out);
    (out.errors.len() == errors_before).then_some(settings)
}

fn read_admin(value: &Value, file: &mut BTreeMap<&'static str, SettingValue>, invalid: &mut BTreeSet<&'static str>, out: &mut ValidationReport) {
    let Some(map) = value.as_mapping() else {
        out.errors.push(error_wrong_type(&setting_path("admin"), "map", FoundKind::of(value)));
        return;
    };
    for (key, entry) in map {
        let Some(sub) = key.as_str() else { continue };
        let full = format!("admin.{sub}");
        match find(&full) {
            Some(def) => match read_value(def, entry, &join_path(&setting_path("admin"), sub), out) {
                Ok(v) => {
                    file.insert(def.name, v);
                }
                Err(()) => {
                    invalid.insert(def.name);
                }
            },
            None => out.errors.push(error_fixed(ErrorCode::UnknownField, &join_path(&setting_path("admin"), sub), TEXT_UNKNOWN_FIELD)),
        }
    }
}

fn check_address(name: &'static str, value: &SettingValue, out: &mut ValidationReport, invalid: &mut BTreeSet<&'static str>) {
    if let SettingValue::Text(text) = value {
        if text.parse::<std::net::SocketAddr>().is_err() {
            let path = match name.strip_prefix("admin.") {
                Some(sub) => join_path(&setting_path("admin"), sub),
                None => setting_path(name),
            };
            out.errors.push(error_fixed(ErrorCode::BadSettings, &path, TEXT_BAD_ADDRESS));
            invalid.insert(name);
        }
    }
}

fn check_file_value(def: &'static SettingDef, value: &SettingValue, invalid: &mut BTreeSet<&'static str>, out: &mut ValidationReport) {
    let path = match def.name.strip_prefix("admin.") {
        Some(sub) => join_path(&setting_path("admin"), sub),
        None => setting_path(def.name),
    };
    if def.name == "hold_limit_status" {
        if let SettingValue::Int(code) = value {
            if HoldLimitStatus::from_code(*code).is_none() {
                let text = if *code == u64::from(HOLD_STATUS_REFUSED) { TEXT_STATUS_REFUSED } else { TEXT_STATUS_NOT_ALLOWED };
                out.errors.push(error_fixed(ErrorCode::BadSettings, &path, text));
                invalid.insert(def.name);
            }
        }
        return;
    }
    if let Some(v) = value.number() {
        if !in_absolute_range(def, v) {
            out.errors.push(error_fixed(ErrorCode::BadSettings, &path, TEXT_OUT_OF_RANGE));
            invalid.insert(def.name);
        }
    }
}

/// Relations between settings, on the values in use (file values and defaults).
fn check_relations(resolved: &[(&'static SettingDef, SettingValue, Source)], invalid: &BTreeSet<&'static str>, out: &mut ValidationReport) {
    let number = |name: &str| resolved.iter().find(|(d, _, _)| d.name == name).and_then(|(_, v, _)| v.number());
    for (def, value, _) in resolved {
        if invalid.contains(def.name) {
            continue;
        }
        let Some(v) = value.number() else { continue };
        for (bound, is_min) in [(&def.min, true), (&def.max, false)] {
            let Bound::Setting(other) = bound else { continue };
            if invalid.contains(other) {
                continue;
            }
            let Some(limit) = number(other) else { continue };
            let exclusive = if is_min { def.min_exclusive } else { def.max_exclusive };
            let holds = match (is_min, exclusive) {
                (true, true) => v > limit,
                (true, false) => v >= limit,
                (false, true) => v < limit,
                (false, false) => v <= limit,
            };
            if !holds {
                let text = match (is_min, exclusive) {
                    (true, true) => TEXT_ABOVE,
                    (true, false) => TEXT_AT_LEAST,
                    (false, true) => TEXT_BELOW,
                    (false, false) => TEXT_AT_MOST,
                };
                out.errors.push(error_names_setting(ErrorCode::BadSettings, &setting_path(def.name), text, other));
            }
        }
    }
}

fn build(resolved: Vec<(&'static SettingDef, SettingValue, Source)>, aliases: &AliasTable, out: &mut ValidationReport) -> EffectiveSettings {
    let mut s = EffectiveSettings {
        listen: None,
        hold_limit_s: 0,
        protected_window_s: 0,
        probation_window_s: 0,
        mature_turns: 0,
        max_held: 0,
        max_held_bytes: 0,
        hold_limit_status: None,
        hold_retry_after_s: 0,
        table_ttl_s: 0,
        table_cap: 0,
        spill_after_s: None,
        feedback_window: 0,
        feedback_min_turns: 0,
        feedback_min_tokens: 0,
        reuse_floor: 0.0,
        reprobe_after_s: 0,
        key_text_limit_system: 0,
        key_text_limit_first: 0,
        wrapper_tags: Vec::new(),
        body_limit_bytes: 0,
        cold_allowance_tokens: 0,
        probe_budget_s: 0,
        ready_health_wait_s: 0,
        node_probe_interval_s: 0,
        upstream_idle_reuse_max_s: 0,
        truncation_report_ratio: 0.0,
        ollama_truncation_limit_ratio: 0.0,
        ollama_bytes_per_token: 0,
        log_dir: None,
        log_check_interval_s: 0,
        log_pause_free_bytes: 0,
        log_resume_free_bytes: 0,
        log_queue_capacity: 0,
        state_dir: String::new(),
        admin_listen: String::new(),
        admin_token_file: None,
        client_tokens_ref: None,
        alias_hold: BTreeMap::new(),
        resolved,
        declared_pending: Vec::new(),
    };
    let u32_of = |s: &EffectiveSettings, name: &str| u32::try_from(s.int(name)).unwrap_or(u32::MAX);
    let text_of = |s: &EffectiveSettings, name: &str| match s.value(name) {
        SettingValue::Text(t) => Some(t.clone()),
        _ => None,
    };
    let float_of = |s: &EffectiveSettings, name: &str| s.value(name).number().unwrap_or(0.0);
    s.listen = text_of(&s, "listen");
    s.hold_limit_s = u32_of(&s, "hold_limit_s");
    s.protected_window_s = u32_of(&s, "protected_window_s");
    s.probation_window_s = u32_of(&s, "probation_window_s");
    s.mature_turns = u32_of(&s, "mature_turns");
    s.max_held = u32_of(&s, "max_held");
    s.max_held_bytes = s.int("max_held_bytes");
    s.hold_limit_status = s.resolved.iter().find(|(d, _, src)| d.name == "hold_limit_status" && *src == Source::File).and_then(|(_, v, _)| match v {
        SettingValue::Int(code) => HoldLimitStatus::from_code(*code),
        _ => None,
    });
    s.hold_retry_after_s = u32_of(&s, "hold_retry_after_s");
    s.table_ttl_s = u32_of(&s, "table_ttl_s");
    s.table_cap = u32_of(&s, "table_cap");
    s.spill_after_s = match s.value("spill_after_s") {
        SettingValue::Int(n) => u32::try_from(*n).ok(),
        _ => None,
    };
    s.feedback_window = u32_of(&s, "feedback_window");
    s.feedback_min_turns = u32_of(&s, "feedback_min_turns");
    s.feedback_min_tokens = u32_of(&s, "feedback_min_tokens");
    s.reuse_floor = float_of(&s, "reuse_floor");
    s.reprobe_after_s = u32_of(&s, "reprobe_after_s");
    s.key_text_limit_system = u32_of(&s, "key_text_limit_system");
    s.key_text_limit_first = u32_of(&s, "key_text_limit_first");
    s.wrapper_tags = match s.value("wrapper_tags") {
        SettingValue::List(l) => l.clone(),
        _ => Vec::new(),
    };
    s.body_limit_bytes = s.int("body_limit_bytes");
    s.cold_allowance_tokens = u32_of(&s, "cold_allowance_tokens");
    s.probe_budget_s = u32_of(&s, "probe_budget_s");
    s.ready_health_wait_s = u32_of(&s, "ready_health_wait_s");
    s.node_probe_interval_s = u32_of(&s, "node_probe_interval_s");
    s.upstream_idle_reuse_max_s = u32_of(&s, "upstream_idle_reuse_max_s");
    s.truncation_report_ratio = float_of(&s, "truncation_report_ratio");
    s.ollama_truncation_limit_ratio = float_of(&s, "ollama_truncation_limit_ratio");
    s.ollama_bytes_per_token = u32_of(&s, "ollama_bytes_per_token");
    s.log_dir = text_of(&s, "log_dir");
    s.log_check_interval_s = u32_of(&s, "log_check_interval_s");
    s.log_pause_free_bytes = s.int("log_pause_free_bytes");
    s.log_resume_free_bytes = s.int("log_resume_free_bytes");
    s.log_queue_capacity = u32_of(&s, "log_queue_capacity");
    s.state_dir = text_of(&s, "state_dir").unwrap_or_default();
    s.admin_listen = text_of(&s, "admin.listen").unwrap_or_default();
    s.admin_token_file = text_of(&s, "admin.token_file");
    s.client_tokens_ref = text_of(&s, "client_tokens_ref");
    check_alias_hold(&mut s, aliases, out);
    warn_hold_budget(&s, out);
    s
}

/// The hold limit of each alias: same range and relation as the global value.
fn check_alias_hold(s: &mut EffectiveSettings, aliases: &AliasTable, out: &mut ValidationReport) {
    let max = SECONDS_PER_HOUR as u32;
    for name in aliases.names() {
        let Ok(alias) = aliases.resolve(&name.0) else { continue };
        let Some(v) = alias.hold_limit_s else { continue };
        let path = join_path(&join_path("aliases", &name.0), "hold_limit_s");
        if v > s.protected_window_s && v <= max {
            s.alias_hold.insert(name.0.clone(), v);
        } else if v <= s.protected_window_s {
            out.errors.push(error_names_setting(ErrorCode::BadSettings, &path, TEXT_ABOVE, "protected_window_s"));
        } else {
            out.errors.push(error_fixed(ErrorCode::BadSettings, &path, TEXT_OUT_OF_RANGE));
        }
    }
}

/// The single rule for the hold budget warning (story 187 cites it).
fn warn_hold_budget(s: &EffectiveSettings, out: &mut ValidationReport) {
    if s.hold_limit_s > HOLD_LIMIT_WARN_S {
        out.warnings.push(warning_fixed(WarningCode::HoldLimitAboveBudget, &setting_path("hold_limit_s"), TEXT_HOLD_BUDGET));
    }
    for (name, v) in &s.alias_hold {
        if *v > HOLD_LIMIT_WARN_S {
            out.warnings.push(warning_fixed(WarningCode::HoldLimitAboveBudget, &join_path(&join_path("aliases", name), "hold_limit_s"), TEXT_HOLD_BUDGET));
        }
    }
}

/// What the admin read shows of one setting.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingView {
    pub name: String,
    pub value: String,
    pub default: String,
    pub unit: Unit,
    pub status: Status,
    pub range: String,
    pub source: Source,
    pub restart: bool,
    /// For a start-only setting changed by a reload: the declared value that waits for a restart.
    pub declared: Option<String>,
}

const RANGE_NOT_SET: &str = "NOT SET";

fn range_text(def: &SettingDef) -> String {
    if def.name == "hold_limit_status" {
        return "503, 504, 529".to_string();
    }
    let show = |b: &Bound| match b {
        Bound::Int(n) => Some(n.to_string()),
        Bound::Float(f) => Some(format!("{f}")),
        Bound::Setting(s) => Some((*s).to_string()),
        Bound::None => None,
    };
    match (show(&def.min), show(&def.max)) {
        (Some(min), Some(max)) => format!("{}{min} to {max}{}", if def.min_exclusive { "above " } else { "" }, if def.max_exclusive { " (exclusive)" } else { "" }),
        (Some(min), None) => format!("{} {min} and up", if def.min_exclusive { "above" } else { "at least" }),
        (None, Some(max)) => format!("below {max}"),
        (None, None) => match def.unit {
            Unit::Address => "address".to_string(),
            Unit::Path => "path".to_string(),
            Unit::SecretRef => "secret reference".to_string(),
            Unit::TagList => "list".to_string(),
            _ => RANGE_NOT_SET.to_string(),
        },
    }
}

pub fn setting_views(s: &EffectiveSettings) -> Vec<SettingView> {
    let mut views: Vec<SettingView> = s
        .resolved
        .iter()
        .map(|(def, value, source)| SettingView {
            name: def.name.to_string(),
            value: value.show(),
            default: SettingValue::from_default(&def.default).show(),
            unit: def.unit,
            status: def.status,
            range: range_text(def),
            source: *source,
            restart: def.restart,
            declared: s.declared_pending.iter().find(|(n, _)| *n == def.name).map(|(_, v)| v.clone()),
        })
        .collect();
    for (alias, v) in &s.alias_hold {
        views.push(SettingView {
            name: format!("aliases.{alias}.hold_limit_s"),
            value: v.to_string(),
            default: s.hold_limit_s.to_string(),
            unit: Unit::Seconds,
            status: Status::Proposed,
            range: range_text(find("hold_limit_s").unwrap_or(&CATALOGUE[0])),
            source: Source::File,
            restart: false,
            declared: None,
        });
    }
    views
}

/// The settings check of the registry check list.
pub struct SettingsCheck;

impl RegistryCheck for SettingsCheck {
    fn check(&self, doc: &RawRegistry, out: &mut ValidationReport) {
        let mut scratch = ValidationReport::default();
        let typed = crate::config::node::read_nodes(&doc.0, &mut scratch);
        let aliases = crate::config::alias::read_aliases(&doc.0, &typed, &mut scratch).unwrap_or_default();
        let settings = doc.0.as_mapping().and_then(|root| root.get("settings"));
        read_settings(settings, &aliases, out);
    }
}
