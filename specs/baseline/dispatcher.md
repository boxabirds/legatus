# Dispatcher and router

A subagent states what it needs; Legatus chooses the **node** and, through a lease, holds it for the task. The selection logic lives in `legatus-router`, so it is shared by every harness and outlives any one harness process. The router does not decide which role to call or when; that is the harness's business.

## Selection, per call

Runs at acquire time, and for requests that carry no lease.

1. **Resolve needs.** A role gives its `requires`, endpoint kind and protocol; a caller may instead state capabilities directly.
2. **Filter.** First drop nodes whose capabilities or endpoint kind do not match `requires`, endpoint kind and protocol. Then drop nodes that fail health or lack the context for prompt plus expected output.
3. **Stick.** If this session already holds a lease or pin on a surviving candidate, use it so its prompt cache stays warm.
4. **Score by load.** Otherwise take the first candidate whose queue is under its threshold; if none, the least loaded.
5. **Take a slot.** An acquire takes one slot on the node, or all of them for an `exclusive` role. With no free slot the acquire is refused. See [LLM pool allocation](coordination.md).

Every decision is written to the decision log.

## One protocol per role

The router serves both OpenAI chat-completions and Anthropic Messages, per role type, with the role name in the `model` field. Each role has one protocol and one endpoint kind, never mixed: the router validates this at load and rejects a role whose candidates differ. It does not translate between protocols. Local roles use OpenAI chat-completions; frontier roles keep their own protocol as separate roles.

## Endpoint kinds

`chat`, `embeddings`, `transcription`, `speech`, `images`, `rerank`, `classify`, `decide` and `tool`. A role has one kind. Non-chat kinds are called by services rather than by the harness, so they need a router path and the node's own path but no harness-side protocol. See [registry](registry.md#two-role-tiers).

## Live signals

| Source | Signal | Polled |
| --- | --- | --- |
| llama-server `/slots`, `/metrics` | busy slots, queued requests, tokens/s | every `LOAD_POLL_INTERVAL_S` |
| vLLM `/metrics` | running and waiting requests | every `LOAD_POLL_INTERVAL_S` |
| Router health checks or `legatus-node` | node up or down | every `HEALTH_POLL_INTERVAL_S` |
| `legatus-node` | machine readings: power and thermal state | every `LOAD_POLL_INTERVAL_S` (PROPOSED) |
| Ollama `/api/ps` | loaded context (its only load signal; one request slot) | every `LOAD_POLL_INTERVAL_S` |

llama-server's `/slots` wakes a sleeping server, so poll `/health`, `/props` and `/metrics` instead. mlx_lm exposes no load signal; the router counts its own in-flight requests for it.

## Context estimate and empty candidate list

- **Estimator.** Required context is the prompt token count plus the role's expected output tokens, or `OUTPUT_TOKEN_ESTIMATE_DEFAULT` when the role gives none. Prompt tokens come from the engine's tokenizer endpoint where available, else a conservative character-based count.
- **No candidate survives.** If the filter step leaves no candidate, the router returns `NoCandidateError` naming each dropped node and why (missing capability, wrong endpoint kind, unhealthy, context too small). It does not guess or fall back outside the role's `candidates`. The acquire call carries the same reasons.

## Protecting against silent truncation

Some engines accept a prompt that does not fit and quietly cut it. Ollama 0.35.1 returned HTTP 200 with a prompt cut to about 2050 tokens against a 4096 default; llama-server returned an explicit 400 `exceed_context_size_error`; mlx_lm has no overflow behaviour and its memory grows.

- **Node declares behaviour.** Each node's `limits.on_overflow` is `error_400`, `error` or `silent_truncate` (see [registry](registry.md#fields)).
- **Router guard.** For a `silent_truncate` node, the router enforces its own prompt-token limit against the node's effective `max_ctx` before sending, and rejects a request that would not fit.
- **Streaming check for Messages.** Compare the input token count in the streamed `message_start` and the final usage with the router's own estimate; a large shortfall means the node truncated. The router logs it.
- **Rewriter is a last resort.** The Ollama `normalise_sse` rewriter stays off by default; it is a guard, not a fix.

## Request patches and failure classification

Engines differ, so per-node edits to the request body (`request_patch`, for example thinking controls) and the failure classification table are **data in the registry**, not code. The router applies the patch and records the outcome class of each request; in v1 it does not act on the class.

| Engine | Signal | Class |
| --- | --- | --- |
| llama-server | 400 with `exceed_context_size_error` | Request problem |
| llama-server | 500 for malformed JSON | Request problem, though pi will retry it |
| Ollama | 200 with a short input token count | Silent truncation |
| mlx_lm | Connection dropped on an invalid request | Request problem when the node is otherwise healthy |
| any | Connection refused, timeout, other 5xx | Node failure |

These shapes were seen on the spike versions (Ollama 0.35.1, llama-server 0.5.0, mlx_lm 0.32.0); the table is per engine and version and needs re-checking on upgrade.

## Affinity, session pin and lease stickiness

Agent requests resend a growing prefix every turn, and a prefix cache only helps if the request lands on the same server. The router therefore keys on a session id and keeps each session on its node. The lease is the explicit form of the pin.

- **The pin never moves mid-task.** There is no re-pin on failure and no avoid-entry in v1. A pinned or leased node that is unavailable returns its error.
- **Pin header.** The key is `x-session-affinity`, on both the OpenAI and Messages paths. In pi 1.0.3 it is sent when a provider sets `compat.sendSessionAffinityHeaders: true` with `sessionAffinityFormat: "openai-nosession"`; the generated `models.json` sets both. A lease adds `x-legatus-lease`.
- **Pins persist across router restarts**, in the same journal as leases; otherwise every restart would move sessions between nodes.
- **Sessionless calls are served.** pi's compaction and summary calls carry no session header. The router serves them without reading or creating a pin, on the least-loaded healthy candidate.
- **Other harnesses** send their own ids (Claude Code sends `x-claude-code-session-id`); each adapter maps its id to the pin key. DeepSeek Harness sends no session header on its chat and Anthropic routes.

## Why not an LLM router

It adds latency to every call, its mistakes are hard to trace, and the role already captures the nature of the task. Context length, tool needs and load are filters, not judgments.

## v2+ resilience

Timeouts, retries, circuit breakers, failover and health demotion are deferred to v2 or later, not rejected. For v1 an endpoint failure goes to the agent. The spike showed what that costs: a hung node holds pi for its 300 s request timeout and pi's retry returns to the same node, whereas a 3 s first-byte timeout let pi recover in 3.4 s. The full design is in [router-resilience-design](../archive/router-resilience-design.md).

Two seams keep the future work cheap without building it:

- **One upstream call site.** Every request to a node goes through a single function in the router, so a timeout, a retry or a breaker can be added in one place.
- **A recorded outcome for every request.** The router records the outcome class of each request (success, request problem, node failure, truncation) beside the decision log. It is observed, not acted on.
