//! The known gaps in the Responses API support of each engine, copied from the engine notes
//! (read from source on 2026-10-07; no engine ran a live Codex conversation). The proxy states no
//! engine supports Codex before the real-session test. PROPOSED: DEC-065 pending the owner.
use legatus_common::engine::{GapLabel, ResponsesGap};

const fn gap(text: &'static str, label: GapLabel, source: &'static str) -> ResponsesGap {
    ResponsesGap { text, label, source }
}

const LLAMA_SERVER: &[ResponsesGap] = &[
    gap("A reasoning item with a null summary gives a 400.", GapLabel::Inferred, "llama.cpp issue 29159"),
    gap("Drops namespace and web_search tools.", GapLabel::Inferred, "llama.cpp issue 24295"),
    gap("Ignores prompt_cache_key, include and store.", GapLabel::Inferred, "engine notes R7, llama-server b11462"),
];
const OLLAMA: &[ResponsesGap] = &[
    gap("Drops developer items, custom tools, namespace and tool search.", GapLabel::Inferred, "engine notes R7, Ollama 0.40.0-rc6"),
    gap("Silently ignores previous_response_id.", GapLabel::Inferred, "engine notes R7, Ollama 0.40.0-rc6"),
];
const VLLM: &[ResponsesGap] = &[gap("Stateful only with the response store switched on.", GapLabel::Documented, "Codex issues 45273 and 55659")];
const SGLANG: &[ResponsesGap] = &[
    gap("previous_response_id needs the store flag.", GapLabel::Documented, "engine notes R7"),
    gap("cached_tokens needs the details flag.", GapLabel::Documented, "engine notes R7"),
];
const MLX_LM: &[ResponsesGap] = &[gap("Answers 404 on the Responses route and cannot serve Codex.", GapLabel::Inferred, "engine notes R7, mlx_lm.server")];
const GUFO: &[ResponsesGap] = &[
    gap("Stateless.", GapLabel::Documented, "engine notes R7"),
    gap("Flattens namespace tools.", GapLabel::Documented, "engine notes R7"),
    gap("Skips hosted tools.", GapLabel::Documented, "engine notes R7"),
];
const UNVERIFIED: &[ResponsesGap] = &[gap("Responses behaviour of this engine is unverified.", GapLabel::Unverified, "none recorded")];

/// The gaps of an engine by its registry name. An engine with none recorded gets the fixed
/// unverified note, never silence.
pub fn responses_gaps(engine_name: &str) -> &'static [ResponsesGap] {
    match engine_name {
        "llama-server" => LLAMA_SERVER,
        "ollama" => OLLAMA,
        "vllm" => VLLM,
        "sglang" => SGLANG,
        "mlx_lm" => MLX_LM,
        "gufo" => GUFO,
        _ => UNVERIFIED,
    }
}
