# Role registry

The registry is one YAML file and the single source of truth. It generates the pi provider config (`models.json`), the pi-subagents agent definitions, and the router's routing table. It describes nodes (what each machine can do) and roles (what each kind of work needs, with nodes in priority order).

```yaml
nodes:
  m5max:    { url: http://m5max.local:8080/v1,   engine: mlx,            mem_gb: 128, max_ctx: 131072, always_on: false }
  rtx4090:  { url: http://rtx4090.local:8080/v1, engine: llama.cpp-cuda,   vram_gb: 24,  max_ctx: 65536,  always_on: true }
  strix:    { url: http://strix.local:8080/v1,   engine: llama.cpp-vulkan, mem_gb: 128, max_ctx: 131072, always_on: true }
  frontier: { provider: anthropic, model: frontier-main, max_ctx: 1000000, budget_gbp_day: 10 }

roles:
  architect:       { candidates: [frontier],               tools: read,          escalation_only: true }
  spec-writer:     { candidates: [frontier, m5max],        tools: read+write-docs }
  researcher:      { candidates: [strix, m5max, frontier], tools: read+web }
  tester:          { candidates: [rtx4090, strix],         tools: read+exec }
  tech-writer:     { candidates: [m5max, strix],           tools: read+write-docs }
  marcomms:        { candidates: [m5max, frontier],        tools: read+write-docs }
  release-manager: { candidates: [strix, m5max],           tools: read+git+exec, exclusive: release }
```

`engine` and `provider` take one value per node; changing engine is an edit to that field. Hostnames, memory sizes and context limits above are assumptions to confirm per machine. Every `tools` value is an allow-list: a role can use only what its profile names (least privilege).

## Fields

| Field | Purpose |
| --- | --- |
| `candidates` | Ordered preference; local first except where the role needs frontier depth |
| `max_ctx` | The router skips a node whose context is smaller than prompt plus expected output |
| `tools` | Permission profile handed to the subagent; the architect never edits |
| `escalation_only` | The orchestrator may call the role only on stated triggers (architecture, repeated failure, reviewer flag) |
| `exclusive` | A lease the role must hold before running, so two release managers never overlap |
| `budget_gbp_day` | Frontier spend cap enforced in the router; behaviour at exhaustion is defined in [dispatcher](dispatcher.md#frontier-budget-exhaustion) |
| `always_on` | Laptop nodes that may sleep are demoted quickly by health checks |

Each role also carries a one-paragraph description written around the work, not the hardware. Those descriptions are what the orchestrator reads when choosing a role.

## Smoke test

No model and engine pair joins a role until it passes the tool-call smoke test. The suite runs `SMOKE_CALLS` scripted calls against the pair, covering: a single tool call, parallel tool calls, a nested-argument call whose JSON must parse, a follow-up turn that consumes a tool result, a prompt that needs no tool (the model must not call one), and a call with a large argument.

- **Pass:** at least `SMOKE_MIN_PASS` of calls produce a valid result and the correct tool decision, and at least `SMOKE_MIN_VALID_JSON` of tool calls have arguments that parse and match the schema. Run at the sampling temperature used in production.
- **Fail:** the pair is excluded from every role's `candidates` at generation time, and the result is logged with the failing calls.
- **Rerun:** on any change to the model, quantisation or engine version.

Parameter values are in [architecture](architecture.md#parameters).

## Adding hardware

A new machine is one `nodes` entry plus its name in the `candidates` lists that should use it. Examples:

- **RTX 5060 Ti 12 GB:** small-model node for summaries, compaction, titles or embeddings.
- **DGX Spark:** large-model and batch node, good at prefill-heavy roles such as many parallel reviewers.

Every node joins through the same OpenAI-compatible contract, so the engine on it can change without touching routing.
