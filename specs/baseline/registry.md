# Role registry

The registry is one YAML file and the single source of truth. There is no embedded database; machine-written records (verdicts, calibration, journals, logs) are JSONL or JSON files. It generates the pi provider config (`models.json`) and the router's routing table. It describes nodes (what each machine can do) and roles (what each kind of work needs, with nodes in priority order). Routing parameters, such as per-node request patches, live in the registry, not in the engines (D1).

## Nodes: three layers

A node is described in three layers. Roles are not a layer; they sit above and declare what they require.

| Layer | Answers | Source |
| --- | --- | --- |
| **Model card** | What can the weights do? Modalities, trained context, structured output, tool calling, reasoning style | Declared |
| **Server card** | What does the engine expose? Endpoints and their kinds, engine and version, features, limits, overflow behaviour, auth | Declared, checked by the calibration probe |
| **Measured profile** | How does it behave here? Time to first token, decode and prefill rate, useful concurrency, effective context | Measured by the calibration probe |

```yaml
router:                          # where agents and adapters reach the router
  addr: http://client.local:8000
  admin_addr: http://127.0.0.1:8001   # the router admin API: preflight, lease acquire, release, list; PROPOSED
  admin_ui_addr: http://127.0.0.1:8002   # legatus-admin: dashboard and MCP; PROPOSED

machines:                        # PROPOSED; readings come from legatus-node
  m5-laptop: { mem_gb: 128 }
  rtx-box:   { mem_gb: 64 }

nodes:
  rtx4090:                       # llama-server chat node
    model_card:
      id: qwen3-1.7b-q4km
      quantisation: q4_k_m
      modalities: { in: [text], out: [text] }
      context_trained: 40960
      structured_output: [json_schema, grammar]
      tool_calling: native-jinja
      reasoning: { style: reasoning_content, controllable: true }
    server_card:
      engine: { name: llama-server, version: b11146 }
      base_url: http://rtx4090.local:8080
      endpoints:
        - { kind: chat,       protocol: openai-chat,       path: /v1/chat/completions }
        - { kind: embeddings, protocol: openai-embeddings, path: /v1/embeddings, enabled: false }
      limits: { n_ctx_per_slot: 4096, on_overflow: error_400 }
      request_patch: { }
    machine: rtx-box             # which physical host runs this node
    slots: 4                     # registry value; default per engine; capped by measured concurrency
    mem_gb: 3                    # memory the loaded model needs; illustrative
    temperature: 0.2             # sampling temperature used in production and in the smoke test
    measured:                    # written by the calibration probe
      ttft_ms: { p50: 138, p99: 159 }
      decode_tok_s: 66
      concurrency_useful: 4
      effective_ctx: 4096
    always_on: true

  m5max:                         # Ollama chat node
    model_card: { id: qwen3:1.7b, modalities: { in: [text], out: [text] }, context_trained: 40960 }
    server_card:
      engine: { name: ollama, version: 0.35.1 }
      base_url: http://m5max.local:11434
      endpoints: [ { kind: chat, protocol: openai-chat, path: /v1/chat/completions } ]
      limits: { n_ctx: 4096, on_overflow: silent_truncate }
      request_patch: { }         # per-node: Ollama needs different thinking controls than llama-server
    machine: m5-laptop
    slots: 1
    measured: { concurrency_useful: 1, effective_ctx: 4096 }
    always_on: false

  apple:                         # optional on-device node, behind a Swift shim
    model_card: { id: apple-on-device, modalities: { in: [text], out: [text] }, context_trained: 4096 }
    server_card:
      engine: { name: legatus-apple-shim, version: pinned }
      endpoints: [ { kind: chat, protocol: openai-chat, path: /v1/chat/completions } ]
      limits: { n_ctx: 4096, on_overflow: error_400, max_output_override: 512 }   # tokens; PROPOSED placeholder, read from the table by the shim
    slots: 1
    measured: { concurrency_useful: 1 }

  system-one:                    # hosted node: key held by the router
    model_card: { id: jev-latest, modalities: { in: [text], out: [decision] } }
    server_card:
      provider: typesafe
      endpoints: [ { kind: decide, protocol: decide-v1 } ]

  frontier: { provider: anthropic, model: frontier-main, max_ctx: 1000000, slots: 4 }   # hosted node: key held by the router; slots PROPOSED

roles:
  # work roles: require capabilities
  architect:       { candidates: [frontier],        requires: [tool-calling],                   endpoint_kind: chat, protocol: anthropic-messages }
  tester:          { candidates: [rtx4090, m5max],  requires: [tool-calling, structured-output], endpoint_kind: chat, protocol: openai-chat }
  bulk-review:     { candidates: [rtx4090],         requires: [tool-calling],                   endpoint_kind: chat, protocol: openai-chat, exclusive: true }
  # capability roles: one capability each, called by services rather than by pi
  summarise:       { candidates: [apple, m5max],    requires: [text-in, text-out],              endpoint_kind: chat,   protocol: openai-chat }
  triage:          { candidates: [system-one],      requires: [decide],                         endpoint_kind: decide, protocol: decide-v1 }
```

Protocol spellings are `openai-chat` and `anthropic-messages`; the endpoint kind field is `endpoint_kind`. `decide-v1` is the proposed `decide` request and response contract from the capability research (typed questions in, typed answers with probabilities out), not an existing standard. Hostnames, versions and numbers above are illustrative; model and engine values come from the spike machine (Qwen3 1.7B, Apple M2) and the rest are assumed; confirm per machine. The Apple node and the hosted `decide` node are examples of the descriptor, not commitments (proposed default P-5 in the [decision record](../decisions/2026-10-spike-decisions.md)). The full set of work roles (spec-writer, researcher, tech-writer, marcomms) follows the same shape as `tester`. Price and budget fields are not part of the descriptor; the old design is in the [archive](../archive/budget-design.md).

## Role rules

- **One protocol and one endpoint kind per role.** Every candidate of a role must share both. The router rejects a role that mixes them at load, so a frontier role keeps its own protocol as a separate role rather than sharing a candidate list with local nodes.
- **`requires` is a capability list.** A node is a candidate only if its model card and server card cover every entry. Declared capabilities are never trusted once a canary for them has failed (see [smoke test](#smoke-test)).
- **`exclusive` is a boolean and takes every slot.** An acquire for an `exclusive` role takes all the node's slots, so nothing else is leased on it for the task. It names no resource and nothing but the node's own slots is locked (see [LLM pool allocation](coordination.md#capacity)).

## Two role tiers

| Tier | Role | Endpoint kind | Requires |
| --- | --- | --- | --- |
| Capability | `vision-in` | `chat` | `vision-in` |
| Capability | `audio-in` | `chat` | `audio-in` |
| Capability | `transcribe` | `transcription` | `transcribe` |
| Capability | `audio-out` | `chat` | `audio-out` |
| Capability | `speak` | `speech` | `speak` |
| Capability | `image-out` | `images` | `image-out` |
| Capability | `embed` | `embeddings` | `embed` |
| Capability | `rerank` | `rerank` | `rerank` |
| Capability | `classify` | `classify` | `classify` |
| Capability | `decide` (typed triage, intent routing, guardrail check) | `decide` | `decide` |
| Capability | `summarise`, `structured-extract` | `chat` | `text-in`, `text-out`, and `structured-output` for extraction |
| Work | `architect`, `spec-writer`, `researcher`, `tester`, `tech-writer`, `marcomms` | `chat` | `tool-calling`, `structured-output`, and a context floor |

Capability roles are written around one capability; work roles keep their one-paragraph description written around the work, not the hardware. Those descriptions are what the orchestrator reads when choosing a role. Non-chat capability roles are called by services, not by pi, so they need a router path and the node's own path but no pi-side protocol. Web search, document conversion and git are tool services, not model candidates.

## Fields

| Field | Purpose |
| --- | --- |
| `candidates` | Ordered preference; local first except where the role needs frontier depth |
| `requires` | Capabilities a node must have; the router filters on this before health and context |
| `endpoint_kind`, `protocol` | The endpoint kind and the wire protocol (`openai-chat`, `anthropic-messages`, ...); one of each per role |
| `max_ctx` | The effective loaded context, per request slot, not the trained context. The router skips a node whose `max_ctx` is smaller than prompt plus expected output |
| `exclusive` | Boolean role flag. When true, a lease on this role takes every slot of its node. See [LLM pool allocation](coordination.md#capacity) |
| `slots` | Node field: how many leases the node can hold at once. The registry value, else the engine default (Ollama 1, llama-server 4 as run, mlx_lm 1, hosted 4 PROPOSED, Apple shim 1), capped by the measured concurrency |
| `machine` | Node field: the machine that runs the node, a key in `machines` (see [Machines](#machines-proposed)) |
| `quantisation`, `temperature`, `mem_gb` | Descriptive node fields: weight quantisation, the sampling temperature used in production and in the smoke test, and the memory the loaded model needs. PROPOSED |
| `request_patch` | Per-node edits to the request body (remove or set fields), because engines differ, for example in thinking controls |
| `limits.on_overflow` | What the node does with a prompt that does not fit: `error_400`, `silent_truncate`, or `unbounded` (no limit, memory grows; mlx_lm). Drives the router's truncation guard in [dispatcher](dispatcher.md#protecting-against-silent-truncation) |
| `always_on` | Laptop nodes that may sleep; the health filter drops them when they are down |

## Machines (PROPOSED)

A **machine** entity, separate from a node, describes the physical host: its power and thermal behaviour and the regimes it runs in (for example plugged in, on battery, thermally throttled). One machine can host several nodes, and a node names its machine in the `machine` field. The `machines` section and the `machine` field are accepted; the machine's fields beyond `mem_gb` and the regime list are not settled.

The node agent (`legatus-node`) owns the readings (power, temperature, throttling) for its machine. The behavioural-telemetry stories consume them and in v1 only record and report them; the router does not act on them. The thermal band type (`Normal`, `Warm`, `Hot`, `Throttling`, `Unknown`) is defined once, in `legatus-common`, by the thermal story. Power and thermal readings are in v1 scope, per machine (see the [scope reset](../decisions/2026-10-scope-reset.md)).

## Router

The `router` section of the registry carries the router's own address (`addr`), its admin address (`admin_addr`), which the adapter reads to call the lease routes, and the address of the dashboard and MCP process (`admin_ui_addr`). The router process takes settings in a `router_settings` block, `admin_addr` and `lease_journal_path` (owned by the router config story) and, for the admin surface, the paths of the token file and the audit log (PROPOSED names `admin_token_file`, `admin_audit_path`). The admin surface never edits the registry. Field names inside `router` are PROPOSED.

## Declared versus measured

- The stricter value wins. If measured effective context is below declared, the measured value is used and the node is flagged. Measured concurrency is a hard cap. Measured speed replaces declared speed.
- Declared context must come from the loaded context, not the trained one. In the spikes both engines probed (Ollama and llama-server) loaded a context far below the trained one (4096 against 40960), and llama-server reports context per slot, so the effective value is not the `-c` flag.
- Every difference between declared and measured is logged as a calibration event.

## Calibration

A probe measures each node and writes the `measured` layer.

- **When:** on node join, and on any change of model, quantisation, engine version or flags. A light probe (one time-to-first-token request, one decode request, and a diff of the server's own properties) runs at `CALIBRATION_LIGHT_INTERVAL_S` and when the router records repeated node failures (resilience itself is v2+).
- **What:** time to first token, decode rate, prefill rate at several prompt sizes, useful concurrency, effective context with a sentinel check for silent truncation, cache counters, and the shape of four error responses. Sizes and levels are in the [parameters](architecture.md#parameters).
- **Route existence** is probed: a bad-body request gives 404 for a missing route and 400, 422 or 501 for a present one. llama-server returned 501 for embeddings and rerank when started without the matching flag, so the descriptor lists enabled endpoints, not model capabilities alone.
- **Caveat.** The probe was run only against Ollama and llama-server with a 1.7B model; the Ollama numbers may include contention from other load. Thinking models spend tokens on reasoning, so the sentinel check needs a larger output limit.

## Smoke test

No model and engine pair joins a role until it passes the tool-call smoke test. The suite runs `SMOKE_CALLS` scripted calls against the pair, covering: a single tool call, parallel tool calls, a nested-argument call whose JSON must parse, a follow-up turn that consumes a tool result, a prompt that needs no tool (the model must not call one), and a call with a large argument.

- **Per-capability canaries.** Each capability a role requires gets a one-shot canary request on the node's own endpoint (an embedding request, a transcription of a short clip, a `decide` question, and so on). The canary library has a single home, the canary story (60); the smoke test carries no second copy and reuses it. A node that fails a canary loses that capability until the next passing run, whatever it declares.
- **Pass:** at least `SMOKE_MIN_PASS` of calls produce a valid result and the correct tool decision, and at least `SMOKE_MIN_VALID_JSON` of tool calls have arguments that parse and match the schema. Run at the sampling temperature used in production.
- **Fail:** the pair is excluded from every role's `candidates` at generation time, and the result is logged with the failing calls.
- **Rerun:** on any change to the model, quantisation or engine version.
- **Verdicts are files.** The suite writes its verdicts to `smoke-results.json` and the failing calls to `smoke-failures.jsonl`; config generation reads the verdicts.

Parameter values are in [architecture](architecture.md#parameters).

## Adding hardware

A new machine is one `nodes` entry plus its name in the `candidates` lists that should use it. It joins by calibration, then the smoke test. Examples:

- **RTX 5060 Ti 12 GB:** small-model node for summaries, compaction, titles or embeddings.
- **DGX Spark:** large-model and batch node, good at prefill-heavy roles such as many parallel reviewers.

Every node joins through the same contract per endpoint kind (OpenAI-compatible for chat), so the engine on it can change without touching routing.
