# Dispatcher and router

A subagent states what it needs; Legatus chooses the **node** and, through a lease, holds it for the task. The selection logic lives in `legatus-router`, so it is shared by every harness and outlives any one harness process. The router does not decide which role to call or when; that is the harness's business.

## Selection, per call

Runs at acquire time (and for the advisory preflight, which stops before taking a slot), and for requests that carry no lease. An acquire is explicit: the main agent calls it, the router does not intercept a spawn.

1. **Resolve needs.** A role gives its `requires`, endpoint kind and protocol; a caller may instead state capabilities directly.
2. **Filter.** First drop nodes whose capabilities, endpoint kind or protocol do not match, or whose credential is missing. Then drop nodes that fail health or lack the context for prompt plus expected output. Last, drop nodes with no free slot.
3. **Stick.** A request that is bound (it carries a live lease, or its session holds a pin) goes to its bound node alone, skipping steps 2 and 4. If that node is unavailable or no longer fits the request, the router returns the error; it never falls through to another candidate. See [affinity](#affinity-session-pin-and-lease-stickiness).
4. **Score by load.** Only an unbound request is scored. This is the single selection rule: fewest requests in flight, then fewest leased slots, then first in registry order. Sessionless calls use the same rule. There are no queue thresholds and no overflow to a next candidate; a request is not moved because a node looks busy.
5. **Take a slot.** An acquire takes one slot on the node, or all of them for an `exclusive` role. With no free slot the acquire is refused, and the pool view says every resource is busy. See [LLM pool allocation](coordination.md).

Every decision is written to the decision log.

## What the router logs

The router logs metadata only (decision, node, role, lease kind and id, sizes, timings, outcome class, drop reasons). It writes no prompt text and no completion text to any log. Capturing conversations is a separate product, not v1. The event log and the telemetry records keep everything they record in v1: there is no stale limit on records, and a fixed free-disk guard stops writes before the disk fills (what the guard does when it trips is set in the event log story).

## One protocol per role

The router serves both OpenAI chat-completions and Anthropic Messages, per role type, with the role name in the `model` field. Each role has one protocol and one endpoint kind, never mixed: the router validates this at load and rejects a role whose candidates differ. It does not translate between protocols. Local roles use OpenAI chat-completions; frontier roles keep their own protocol as separate roles.

## Endpoint kinds

`chat`, `embeddings`, `transcription`, `speech`, `images`, `rerank`, `classify`, `decide` and `tool`. A role has one kind. Non-chat kinds are called by services rather than by the harness, so they need a router path and the node's own path but no harness-side protocol. See [registry](registry.md#two-role-tiers).

## Live signals

| Source | Signal | Polled |
| --- | --- | --- |
| llama-server `/health`, `/props`, `/metrics` | busy slots, queued requests, tokens/s | every `LOAD_POLL_INTERVAL_S` |
| vLLM `/metrics` | running and waiting requests | every `LOAD_POLL_INTERVAL_S` |
| Router health checks or `legatus-node` | node up or down | every `HEALTH_POLL_INTERVAL_S` |
| `legatus-node` | machine readings (power, temperature, throttling), owned by the node agent per machine; the behavioural-telemetry stories record and report them, and the router does not act on them in v1 | every `LOAD_POLL_INTERVAL_S` (PROPOSED) |
| Ollama `/api/ps` | loaded context (its only load signal; one request slot) | every `LOAD_POLL_INTERVAL_S` |

llama-server's `/slots` wakes a sleeping server, so it is never polled. mlx_lm exposes no load signal; the router counts its own in-flight requests for it. A node's slot count is a registry field, not a polled signal (see [LLM pool allocation](coordination.md#capacity)).

## Context estimate and empty candidate list

- **Estimator.** Required context is the prompt token count plus the role's expected output tokens, or `OUTPUT_TOKEN_ESTIMATE_DEFAULT` when the role gives none. Prompt tokens come from a conservative character-based count. No engine tokenizer endpoint has been proven for this; using one is a later option and NOT TESTED.
- **No candidate survives.** If the filter step leaves no candidate, the router returns `NoCandidateError` naming each dropped node and why (missing capability, wrong endpoint kind, `protocol_mismatch`, `credential_missing`, unhealthy, context too small, `no_free_slot`). It does not guess or fall back outside the role's `candidates`. The acquire call carries the same reasons.

## Protecting against silent truncation

Some engines accept a prompt that does not fit and quietly cut it. Ollama 0.35.1 returned HTTP 200 with a prompt cut to about 2050 tokens against a 4096 default; llama-server returned an explicit 400 `exceed_context_size_error`; mlx_lm has no overflow behaviour and its memory grows.

- **Node declares behaviour.** Each node's `limits.on_overflow` is `error_400`, `silent_truncate` or `unbounded` (see [registry](registry.md#fields)).
- **Router guard.** For a `silent_truncate` node, the router enforces its own prompt-token limit against the node's effective `max_ctx` before sending, and rejects a request that would not fit with a 400. This pre-send check is the only place truncation changes a response.
- **Streaming check for Messages.** Compare the input token count in the streamed `message_start` and the final usage with the router's own estimate; a large shortfall means the node truncated. Detection only logs and records it. It does not mark the node, lower its ceiling, replace the answer or reroute, and a bound request returns the error described under [affinity](#affinity-session-pin-and-lease-stickiness).
- **No stream rewriting.** v1 injects no keep-alive pings into streams and runs no SSE normaliser; the router passes the stream through byte for byte (the spike measured byte-identical output).

## Request patches and failure classification

Engines differ, so per-node edits to the request body (`request_patch`, for example thinking controls) and the failure classification table are **data in the registry**, not code. The router applies the patch and records one outcome for every request. The upstream call site produces a transport outcome (success, client error, server error, connect failed, stream broken, caller gone); the classification table maps it to one of five classes: node failure, caller error, config error, credential unavailable, caller gone. In v1 nothing acts on the class. A router-refused request (malformed body, missing credential) also gets an outcome, so there is exactly one per request.

| Engine | Signal | Class |
| --- | --- | --- |
| llama-server | 400 with `exceed_context_size_error` | Caller error |
| llama-server | 500 for malformed JSON | Caller error, though pi will retry it |
| mlx_lm | Connection dropped on an invalid request | Caller error when the node is otherwise healthy |
| any | Connection refused, timeout, other 5xx | Node failure |
| hosted | Key missing or rejected | Credential unavailable |

Silent truncation (Ollama: 200 with a short input token count) is a detection, logged beside the outcome; it is not a class.

These shapes were seen on the spike versions (Ollama 0.35.1, llama-server 0.5.0, mlx_lm 0.32.0); the table is per engine and version and needs re-checking on upgrade.

## Affinity, session pin and lease stickiness

Agent requests resend a growing prefix every turn, and a prefix cache only helps if the request lands on the same server. The router therefore keys on a session id and keeps each session on its node. The lease is the explicit form of the pin.

- **The pin never moves mid-task.** There is no re-pin on failure and no avoid-entry in v1. A pinned or leased node that is unavailable returns its error.
- **A bound request has one node.** A request that carries a live lease, or whose session holds a pin, goes to that node alone. If the node is unavailable the router returns `bound_node_unavailable` (503); if the lease ended it returns `lease_ended` (409); if the request no longer fits the node's context it returns the 400 path. It never picks another node. A truncation detection on a bound request is logged and the error returned.
- **Pin header.** The key is `x-session-affinity`, on both the OpenAI and Messages paths. In pi 1.0.3 it is sent when a provider sets `compat.sendSessionAffinityHeaders: true` with `sessionAffinityFormat: "openai-nosession"`; the generated `models.json` sets both. A lease adds `x-legatus-lease`; that it can be set per session on pi 1.0.3 is an ASSUMPTION (see [LLM pool allocation](coordination.md#api-proposed)). The router strips every `x-legatus-*` header, and `x-session-affinity`, before a request goes to a hosted node, so none reaches a provider. It returns `x-legatus-request-id` on every response.
- **Pins persist across router restarts**, in the same journal as leases; otherwise every restart would move sessions between nodes.
- **Sessionless calls are served.** pi's compaction and summary calls carry no session header. The router serves them without reading or creating a pin, using the selection rule in step 4 above.
- **Identity is the adapter's job.** The adapter produces one session value (`session#agent` when the harness has agent ids) and sends it in `x-session-affinity`. The router keys only on `x-session-affinity` and `x-legatus-lease` and reads no harness-specific header; Claude Code sends `x-claude-code-session-id`, which its adapter maps. DeepSeek Harness sends no session header on its chat and Anthropic routes.

## Why not an LLM router

It adds latency to every call, its mistakes are hard to trace, and the role already captures the nature of the task. Context length, tool needs and load are filters, not judgments.

## v2+ resilience

Timeouts, retries, circuit breakers, failover and health demotion are deferred to v2 or later, not rejected. For v1 an endpoint failure goes to the agent. The spike showed what that costs: a hung node holds pi for its 300 s request timeout and pi's retry returns to the same node, whereas a 3 s first-byte timeout let pi recover in 3.4 s. The full design is in [router-resilience-design](../archive/router-resilience-design.md).

Two seams keep the future work cheap without building it:

- **One upstream call site.** Every request to a node goes through a single function in the router, so a timeout, a retry or a breaker can be added in one place.
- **A recorded outcome for every request.** The router records one outcome for every request, with its class (node failure, caller error, config error, credential unavailable, caller gone), beside the decision log, through one tap at the call site. It is observed, not acted on.
