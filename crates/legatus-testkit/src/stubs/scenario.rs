//! Scenario format and types of the stub kit (contract C03, story 162).
//!
//! Text format, one directive per line, `#` starts a comment:
//! ```text
//! scenario <name>
//! seed <u64>
//! stub <name> kind=<serial_ollama|multi_slot_llama|batching_mlx> [key=value ...]
//! fault <stub> action=<action> [path=<p>] [nth=<n>] [limit=count:<n>|window:<from>-<to>|always]
//! step <at_ms> <stub> <path> <body_tokens>
//! ```
use super::speed::SpeedModel;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StubKind {
    SerialOllama,
    MultiSlotLlama,
    BatchingMlx,
}

impl StubKind {
    pub fn name(self) -> &'static str {
        match self {
            StubKind::SerialOllama => "serial_ollama",
            StubKind::MultiSlotLlama => "multi_slot_llama",
            StubKind::BatchingMlx => "batching_mlx",
        }
    }
    fn parse(s: &str) -> Option<StubKind> {
        [StubKind::SerialOllama, StubKind::MultiSlotLlama, StubKind::BatchingMlx].into_iter().find(|k| k.name() == s)
    }
}

#[derive(Debug, Clone)]
pub struct StubSpec {
    pub name: String,
    pub kind: StubKind,
    pub model: String,
    pub slots: u32,
    pub ctx_total: u32,
    pub kv_unified: bool,
    /// 0 means unlimited (Ollama default: no error up to 8 queued, PROVEN spike s3).
    pub max_queue: u32,
    pub truncate_ratio: f32,
    pub keep_alive_s: u32,
    pub metrics_on: bool,
    pub sleep_on: bool,
    pub decode_concurrency: u32,
    pub cache_entries: u32,
    pub wedge_after: Option<u32>,
    pub idle_close_s: u32,
    pub speed: SpeedModel,
    /// The next five are accepted here and acted on by story 176.
    pub cache_policy: String,
    pub cache_ram_mib: u32,
    pub checkpoints: u32,
    pub capacity_tokens: u32,
    pub host_cache_mib: u32,
}

pub const DEFAULT_CTX_TOTAL: u32 = 8192;
pub const DEFAULT_KEEP_ALIVE_S: u32 = 300;
pub const DEFAULT_TRUNCATE_RATIO: f32 = 0.5;
pub const DEFAULT_DECODE_CONCURRENCY: u32 = 32;
pub const DEFAULT_CACHE_ENTRIES: u32 = 10;
pub const DEFAULT_IDLE_CLOSE_S: u32 = 5;
pub const DEFAULT_MODEL: &str = "stub-model";

impl StubSpec {
    pub fn new(name: &str, kind: StubKind) -> StubSpec {
        let speed = match kind {
            StubKind::SerialOllama => SpeedModel::ollama_default(),
            StubKind::MultiSlotLlama => SpeedModel::llama_4_slots(),
            StubKind::BatchingMlx => SpeedModel::mlx(),
        };
        StubSpec {
            name: name.to_string(),
            kind,
            model: DEFAULT_MODEL.to_string(),
            slots: if kind == StubKind::MultiSlotLlama { 4 } else { 1 },
            ctx_total: DEFAULT_CTX_TOTAL,
            kv_unified: false,
            max_queue: 0,
            truncate_ratio: DEFAULT_TRUNCATE_RATIO,
            keep_alive_s: DEFAULT_KEEP_ALIVE_S,
            metrics_on: false,
            sleep_on: false,
            decode_concurrency: DEFAULT_DECODE_CONCURRENCY,
            cache_entries: DEFAULT_CACHE_ENTRIES,
            wedge_after: None,
            idle_close_s: DEFAULT_IDLE_CLOSE_S,
            speed,
            cache_policy: "none".to_string(),
            cache_ram_mib: 0,
            checkpoints: 0,
            capacity_tokens: 0,
            host_cache_mib: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Match {
    pub path: Option<String>,
    /// 1-based: only the nth matching request.
    pub nth: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Limit {
    Count(u32),
    /// Half open: from_ms <= t < to_ms.
    Window { from_ms: u64, to_ms: u64 },
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkFault {
    DiskLow,
    Stall { ms: u64 },
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultAction {
    Refuse,
    Hang,
    DropAfterChunks { n: u32, clean_end: bool },
    SlowFirstByte { ms: u64 },
    Status { code: u16, body: String, retry_after_s: Option<u32> },
    EmptyOk,
    Reset { once: bool },
    SinkFault(SinkFault),
    ClockJump { delta_ms: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultRule {
    pub stub: String,
    pub when: Match,
    pub action: FaultAction,
    pub limit: Limit,
}

/// One client request at a virtual instant (test-local: token counts only, no text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub at_ms: u64,
    pub stub: String,
    pub request: StepRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRequest {
    pub path: String,
    pub body_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: String,
    pub seed: u64,
    pub stubs: Vec<StubSpec>,
    pub faults: Vec<FaultRule>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioError {
    UnknownKind(String),
    UnknownField(String),
    BadLimit(String),
}

impl std::fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioError::UnknownKind(s) => write!(f, "unknown kind: {s}"),
            ScenarioError::UnknownField(s) => write!(f, "unknown field: {s}"),
            ScenarioError::BadLimit(s) => write!(f, "bad limit: {s}"),
        }
    }
}

impl std::error::Error for ScenarioError {}

fn field_err(path: &str) -> ScenarioError {
    ScenarioError::UnknownField(path.to_string())
}

fn num<T: std::str::FromStr>(path: &str, v: &str) -> Result<T, ScenarioError> {
    v.parse().map_err(|_| field_err(path))
}

fn flag(path: &str, v: &str) -> Result<bool, ScenarioError> {
    match v {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(field_err(path)),
    }
}

fn apply_stub_field(spec: &mut StubSpec, key: &str, value: &str) -> Result<(), ScenarioError> {
    let path = format!("stub.{}.{key}", spec.name);
    match key {
        "kind" => {}
        "model" => spec.model = value.to_string(),
        "slots" => spec.slots = num(&path, value)?,
        "ctx_total" => spec.ctx_total = num(&path, value)?,
        "kv_unified" => spec.kv_unified = flag(&path, value)?,
        "max_queue" => spec.max_queue = num(&path, value)?,
        "truncate_ratio" => spec.truncate_ratio = num(&path, value)?,
        "keep_alive_s" => spec.keep_alive_s = num(&path, value)?,
        "metrics_on" => spec.metrics_on = flag(&path, value)?,
        "sleep_on" => spec.sleep_on = flag(&path, value)?,
        "decode_concurrency" => spec.decode_concurrency = num(&path, value)?,
        "cache_entries" => spec.cache_entries = num(&path, value)?,
        "wedge_after" => spec.wedge_after = Some(num(&path, value)?),
        "idle_close_s" => spec.idle_close_s = num(&path, value)?,
        "load_time_ms" => spec.speed.load_time_ms = num(&path, value)?,
        "speed" => spec.speed = SpeedModel::by_name(value).ok_or_else(|| field_err(&path))?,
        "cache_policy" => spec.cache_policy = value.to_string(),
        "cache_ram_mib" => spec.cache_ram_mib = num(&path, value)?,
        "checkpoints" => spec.checkpoints = num(&path, value)?,
        "capacity_tokens" => spec.capacity_tokens = num(&path, value)?,
        "host_cache_mib" => spec.host_cache_mib = num(&path, value)?,
        _ => return Err(field_err(&path)),
    }
    Ok(())
}

fn parse_limit(v: &str) -> Result<Limit, ScenarioError> {
    let bad = || ScenarioError::BadLimit(v.to_string());
    if v == "always" {
        return Ok(Limit::Always);
    }
    if let Some(n) = v.strip_prefix("count:") {
        let n: u32 = n.parse().map_err(|_| bad())?;
        return if n == 0 { Err(bad()) } else { Ok(Limit::Count(n)) };
    }
    if let Some(w) = v.strip_prefix("window:") {
        let (a, b) = w.split_once('-').ok_or_else(bad)?;
        let (from_ms, to_ms): (u64, u64) = (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?);
        return if from_ms < to_ms { Ok(Limit::Window { from_ms, to_ms }) } else { Err(bad()) };
    }
    Err(bad())
}

fn parse_action(v: &str) -> Result<FaultAction, ScenarioError> {
    let path = "fault.action";
    let mut parts = v.splitn(4, ':');
    let head = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    Ok(match head {
        "refuse" => FaultAction::Refuse,
        "hang" => FaultAction::Hang,
        "empty_ok" => FaultAction::EmptyOk,
        "reset" => FaultAction::Reset { once: rest.first() == Some(&"once") },
        "drop_after" => FaultAction::DropAfterChunks {
            n: num(path, rest.first().ok_or_else(|| field_err(path))?)?,
            clean_end: rest.get(1) == Some(&"clean"),
        },
        "slow_first_byte" => FaultAction::SlowFirstByte { ms: num(path, rest.first().ok_or_else(|| field_err(path))?)? },
        "status" => FaultAction::Status {
            code: num(path, rest.first().ok_or_else(|| field_err(path))?)?,
            retry_after_s: match rest.get(1) {
                Some(&"") | None => None,
                Some(s) => Some(num(path, s)?),
            },
            body: rest.get(2).map(|s| s.to_string()).unwrap_or_default(),
        },
        "sink_fault" => FaultAction::SinkFault(match rest.first().copied() {
            Some("disk_low") => SinkFault::DiskLow,
            Some("fail") => SinkFault::Fail,
            Some("stall") => SinkFault::Stall { ms: num(path, rest.get(1).ok_or_else(|| field_err(path))?)? },
            _ => return Err(field_err(path)),
        }),
        "clock_jump" => FaultAction::ClockJump { delta_ms: num(path, rest.first().ok_or_else(|| field_err(path))?)? },
        _ => return Err(field_err(path)),
    })
}

/// Parse and validate a scenario. Nothing starts when any error exists.
pub fn parse_scenario(text: &str) -> Result<Scenario, ScenarioError> {
    let mut scenario = Scenario { name: String::new(), seed: 0, stubs: vec![], faults: vec![], steps: vec![] };
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        match words[0] {
            "scenario" => scenario.name = words.get(1).unwrap_or(&"").to_string(),
            "seed" => scenario.seed = num("seed", words.get(1).unwrap_or(&""))?,
            "stub" => {
                let name = words.get(1).ok_or_else(|| field_err("stub.name"))?;
                let pairs: Vec<(&str, &str)> = words[2..].iter().filter_map(|w| w.split_once('=')).collect();
                let kind_text = pairs.iter().find(|(k, _)| *k == "kind").map(|(_, v)| *v).unwrap_or("");
                let kind = StubKind::parse(kind_text).ok_or_else(|| ScenarioError::UnknownKind(kind_text.to_string()))?;
                let mut spec = StubSpec::new(name, kind);
                for (k, v) in pairs {
                    apply_stub_field(&mut spec, k, v)?;
                }
                scenario.stubs.push(spec);
            }
            "fault" => {
                let stub = words.get(1).ok_or_else(|| field_err("fault.stub"))?.to_string();
                let mut rule = FaultRule { stub, when: Match::default(), action: FaultAction::Refuse, limit: Limit::Always };
                let mut have_action = false;
                for w in &words[2..] {
                    let (k, v) = w.split_once('=').ok_or_else(|| field_err("fault"))?;
                    match k {
                        "action" => { rule.action = parse_action(v)?; have_action = true; }
                        "path" => rule.when.path = Some(v.to_string()),
                        "nth" => rule.when.nth = Some(num("fault.nth", v)?),
                        "limit" => rule.limit = parse_limit(v)?,
                        _ => return Err(field_err(&format!("fault.{k}"))),
                    }
                }
                if !have_action {
                    return Err(field_err("fault.action"));
                }
                scenario.faults.push(rule);
            }
            "step" => {
                if words.len() != 5 {
                    return Err(field_err("step"));
                }
                scenario.steps.push(Step {
                    at_ms: num("step.at_ms", words[1])?,
                    stub: words[2].to_string(),
                    request: StepRequest { path: words[3].to_string(), body_tokens: num("step.body_tokens", words[4])? },
                });
            }
            other => return Err(field_err(other)),
        }
    }
    let names: BTreeSet<&str> = scenario.stubs.iter().map(|s| s.name.as_str()).collect();
    for rule in &scenario.faults {
        if !names.contains(rule.stub.as_str()) {
            return Err(field_err(&format!("fault.stub.{}", rule.stub)));
        }
    }
    for step in &scenario.steps {
        if !names.contains(step.stub.as_str()) {
            return Err(field_err(&format!("step.stub.{}", step.stub)));
        }
    }
    Ok(scenario)
}

impl Scenario {
    /// Render a single stub and its rules in the text format (used to start a process).
    pub fn render_stub(spec: &StubSpec, rules: &[FaultRule]) -> String {
        let mut out = format!("scenario process\nstub {} kind={}", spec.name, spec.kind.name());
        out += &format!(" model={} slots={} ctx_total={} kv_unified={} max_queue={} truncate_ratio={}", spec.model, spec.slots, spec.ctx_total, spec.kv_unified, spec.max_queue, spec.truncate_ratio);
        out += &format!(" keep_alive_s={} metrics_on={} sleep_on={} decode_concurrency={} cache_entries={} idle_close_s={} load_time_ms={}", spec.keep_alive_s, spec.metrics_on, spec.sleep_on, spec.decode_concurrency, spec.cache_entries, spec.idle_close_s, spec.speed.load_time_ms);
        if let Some(n) = spec.wedge_after {
            out += &format!(" wedge_after={n}");
        }
        out += "\n";
        for r in rules.iter().filter(|r| r.stub == spec.name) {
            out += &format!("fault {} action={}", r.stub, render_action(&r.action));
            if let Some(p) = &r.when.path { out += &format!(" path={p}"); }
            if let Some(n) = r.when.nth { out += &format!(" nth={n}"); }
            out += &match &r.limit {
                Limit::Always => " limit=always".to_string(),
                Limit::Count(n) => format!(" limit=count:{n}"),
                Limit::Window { from_ms, to_ms } => format!(" limit=window:{from_ms}-{to_ms}"),
            };
            out += "\n";
        }
        out
    }
}

fn render_action(a: &FaultAction) -> String {
    match a {
        FaultAction::Refuse => "refuse".into(),
        FaultAction::Hang => "hang".into(),
        FaultAction::EmptyOk => "empty_ok".into(),
        FaultAction::Reset { once } => if *once { "reset:once".into() } else { "reset".into() },
        FaultAction::DropAfterChunks { n, clean_end } => format!("drop_after:{n}{}", if *clean_end { ":clean" } else { "" }),
        FaultAction::SlowFirstByte { ms } => format!("slow_first_byte:{ms}"),
        FaultAction::Status { code, body, retry_after_s } => format!("status:{code}:{}:{body}", retry_after_s.map(|s| s.to_string()).unwrap_or_default()),
        FaultAction::SinkFault(SinkFault::DiskLow) => "sink_fault:disk_low".into(),
        FaultAction::SinkFault(SinkFault::Fail) => "sink_fault:fail".into(),
        FaultAction::SinkFault(SinkFault::Stall { ms }) => format!("sink_fault:stall:{ms}"),
        FaultAction::ClockJump { delta_ms } => format!("clock_jump:{delta_ms}"),
    }
}
