//! The three node checks (story 136). Each reads the typed nodes again through `read_nodes`
//! into a scratch report, so an error is reported once, by `NodeCheckShape`.
use crate::config::node::*;
use crate::config::registry::*;
use crate::config::schema::join_path;
use crate::config::validate::RegistryCheck;

pub const TEXT_RESPONSES_FLAGS: &str = "Responses flags do not agree.";
pub const TEXT_WARM_BELOW_SLOTS: &str = "Warm capacity cannot be lower than the slots.";
pub const TEXT_MLX_RESPONSES: &str = "The engine answers 404 on the Responses path.";
pub const TEXT_VLLM_DETAILS: &str = "The engine does not report cached prompt tokens unless prompt_tokens_details is declared.";
pub const TEXT_HOSTED_LIMITS: &str = "A hosted node ignores slots and warm capacity.";
pub const TEXT_FLAG_MISMATCH: &str = "Speculative decoding with prefix caching is untested.";

fn typed_nodes(doc: &RawRegistry) -> Vec<NodeSpec> {
    let mut scratch = ValidationReport::default();
    read_nodes(&doc.0, &mut scratch)
}

fn node_path(n: &NodeSpec, field: &str) -> String {
    join_path(&join_path("nodes", &n.name.0), field)
}

/// Field values, enums, URLs, machine references, slots and context (node.schema.fields).
pub struct NodeCheckShape;

impl RegistryCheck for NodeCheckShape {
    fn check(&self, doc: &RawRegistry, out: &mut ValidationReport) {
        read_machines(&doc.0, out);
        read_nodes(&doc.0, out);
    }
}

fn check_responses_flags(n: &NodeSpec, out: &mut ValidationReport) {
    if n.endpoints.iter().any(|e| e.protocol == EndpointProtocol::OpenaiResponses) && !n.responses {
        out.errors.push(error_fixed(ErrorCode::BadResponsesFlag, &node_path(n, "responses"), TEXT_RESPONSES_FLAGS));
    }
    if n.stateful_responses && n.ignores_previous_response_id {
        out.errors.push(error_fixed(ErrorCode::BadResponsesFlag, &node_path(n, "stateful_responses"), TEXT_RESPONSES_FLAGS));
    }
}

fn check_warm_capacity(n: &NodeSpec, out: &mut ValidationReport) {
    if n.engine.is_hosted() {
        return;
    }
    if let (WarmCapacitySetting::Count(warm), Some(Slots::Count(slots))) = (n.warm_capacity, n.slots) {
        if warm < slots {
            out.errors.push(error_fixed(ErrorCode::BadWarmCapacity, &node_path(n, "warm_capacity"), TEXT_WARM_BELOW_SLOTS));
        }
    }
}

/// Responses flags that disagree and a warm capacity below the slots (node.flags.reject).
pub struct NodeCheckFlags;

impl RegistryCheck for NodeCheckFlags {
    fn check(&self, doc: &RawRegistry, out: &mut ValidationReport) {
        for n in typed_nodes(doc) {
            check_responses_flags(&n, out);
            check_warm_capacity(&n, out);
        }
    }
}

fn warn_mlx_responses(n: &NodeSpec, out: &mut ValidationReport) {
    if n.engine == EngineName::MlxLm && n.responses {
        out.warnings.push(warning_fixed(WarningCode::ResponsesEngineMismatch, &node_path(n, "responses"), TEXT_MLX_RESPONSES));
    }
}

fn warn_vllm_details(n: &NodeSpec, out: &mut ValidationReport) {
    if n.engine == EngineName::Vllm && n.prompt_tokens_details.is_none() {
        out.warnings.push(warning_fixed(WarningCode::VllmNoPromptTokensDetails, &join_path(&node_path(n, "cache"), "prompt_tokens_details"), TEXT_VLLM_DETAILS));
    }
}

fn warn_hosted_limits(n: &NodeSpec, out: &mut ValidationReport) {
    if !n.engine.is_hosted() {
        return;
    }
    if n.slots.is_some() {
        out.warnings.push(warning_fixed(WarningCode::HostedIgnoresSlots, &node_path(n, "slots"), TEXT_HOSTED_LIMITS));
    } else if n.warm_capacity != WarmCapacitySetting::Auto {
        out.warnings.push(warning_fixed(WarningCode::HostedIgnoresSlots, &node_path(n, "warm_capacity"), TEXT_HOSTED_LIMITS));
    }
}

fn warn_flag_mismatch(n: &NodeSpec, out: &mut ValidationReport) {
    if n.engine_flags.speculative_decoding == TriState::True && n.engine_flags.prefix_caching == TriState::True {
        out.warnings.push(warning_fixed(WarningCode::FlagMismatch, &join_path(&node_path(n, "engine_flags"), "speculative_decoding"), TEXT_FLAG_MISMATCH));
    }
}

/// Warnings that never stop the load (node.flags.warn).
pub struct NodeCheckWarnings;

impl RegistryCheck for NodeCheckWarnings {
    fn check(&self, doc: &RawRegistry, out: &mut ValidationReport) {
        for n in typed_nodes(doc) {
            warn_mlx_responses(&n, out);
            warn_vllm_details(&n, out);
            warn_hosted_limits(&n, out);
            warn_flag_mismatch(&n, out);
        }
    }
}
