# Dispatcher and router

The orchestrator chooses the **role**; Legatus chooses the **node**. The `delegate(role, task, hints)` tool is a thin pi extension. The selection logic lives in `legatus-router`, so it is shared by every agent and outlives any one pi process.

## Selection, per call

1. **Resolve role.** Look up the role's candidates, permissions and leases. If the role is `escalation_only`, require a stated trigger in `hints`.
2. **Filter.** First drop nodes whose capabilities or endpoint kind do not match the role's `requires`, endpoint kind and protocol. Then drop nodes that fail health, have an open circuit breaker, lack the context for prompt plus expected output, or are over budget (frontier or any hosted node).
3. **Stick.** If this session already ran on a surviving candidate, reuse it so its prompt cache stays warm. A session's retry after a failure must avoid the node that just failed, even if its breaker is still closed.
4. **Score by load.** Otherwise take the first candidate whose queue is under its threshold; if none, the least loaded.
5. **Acquire leases.** Claim any `exclusive` resource and the worktree through `legatus-coord`; on failure, queue and report back.
6. **Spawn.** Launch the subagent via pi-subagents with the chosen model and permission profile, and record the run on the task board.

## One protocol per role

The router serves both OpenAI chat-completions and Anthropic Messages, per role type, with the role name in the `model` field. Each role has one protocol and one endpoint kind, never mixed: the router validates this at load and rejects a role whose candidates differ. It does not translate between protocols. Local roles use OpenAI chat-completions; frontier roles keep their own protocol as separate roles.

## Live signals

| Source | Signal | Polled |
| --- | --- | --- |
| llama-server `/slots`, `/metrics` | busy slots, queued requests, tokens/s | every `LOAD_POLL_INTERVAL_S` |
| vLLM `/metrics` | running and waiting requests | every `LOAD_POLL_INTERVAL_S` |
| Router health checks or `legatus-node` | node up or down | every `HEALTH_POLL_INTERVAL_S` |
| Ollama `/api/ps` | loaded context (its only load signal; one request slot) | every `LOAD_POLL_INTERVAL_S` |
| Router spend tracking | budget remaining for frontier and hosted nodes | per call |

llama-server's `/slots` wakes a sleeping server, so poll `/health`, `/props` and `/metrics` instead. mlx_lm exposes no load signal; the router counts its own in-flight requests for it.

## Context estimate and empty candidate list

- **Estimator.** Required context is the prompt token count plus the role's expected output tokens, or `OUTPUT_TOKEN_ESTIMATE_DEFAULT` when the role gives none. Prompt tokens come from the engine's tokenizer endpoint where available, else a conservative character-based count.
- **No candidate survives.** If the filter step leaves no candidate, `delegate` returns an error naming each dropped node and why (missing capability, wrong endpoint kind, unhealthy, breaker open, context too small, over budget). It does not guess or fall back outside the role's `candidates`. The task is posted as `blocked` with that reason.

## Protecting against silent truncation

Some engines accept a prompt that does not fit and quietly cut it. Ollama 0.35.1 returned HTTP 200 with a prompt cut to about 2050 tokens against a 4096 default; llama-server returned an explicit 400 `exceed_context_size_error`; mlx_lm has no overflow behaviour and its memory grows.

- **Node declares behaviour.** Each node's `limits.on_overflow` is `error_400`, `error` or `silent_truncate` (see [registry](registry.md#fields)).
- **Router guard.** For a `silent_truncate` node, the router enforces its own prompt-token limit against the node's effective `max_ctx` before sending, and rejects or reroutes a request that would not fit.
- **Streaming check for Messages.** Compare the input token count in the streamed `message_start` and the final usage with the router's own estimate; a large shortfall means the node truncated. The router logs it and treats it as a node fault for that session.
- **Rewriter is a last resort.** The Ollama `normalise_sse` rewriter stays off by default; it is a guard, not a fix.

## Circuit breaker

Each node (the frontier API included) has its own circuit breaker in the router, so a failing node stops receiving traffic quickly instead of every request waiting out a timeout.

| State | Behaviour | Moves to |
| --- | --- | --- |
| Closed | Traffic flows normally. Failures are counted; any success resets the count. | Open after `BREAKER_FAILURE_THRESHOLD` consecutive failures, or a failed health check that exceeds `FAILOVER_MAX_S` |
| Open | No requests are sent. The node is filtered out at the Filter step. | Half-open after the cooldown (`BREAKER_COOLDOWN_S`, multiplied by `BREAKER_COOLDOWN_BACKOFF_FACTOR` on each repeat opening, up to `BREAKER_COOLDOWN_MAX_S`) |
| Half-open | One probe at a time: a health call or a one-token completion, never a real agent request. The probe timeout allows for model load time on nodes behind llama-swap. | Closed after `BREAKER_PROBE_SUCCESSES` successes; open again, with the cooldown restarted, on any failure |

A failure is a connection error, a timeout (connect, first-byte or stream-idle; see [parameters](architecture.md#parameters)), or a 5xx response. Request outcomes are the primary signal; the periodic health check is the backstop. A 4xx caused by the request itself is not a node failure and does not count.

**Timeouts are mandatory.** The router enforces a first-byte timeout and an idle timeout on every node. Without them a hung backend stalls the harness for its own request timeout: in the spike pi recovered in 3.4 s with a 3 s first-byte timeout and waited 300 s without one.

**Per-engine failure classification.** Status codes alone are not enough. Classify by engine and body shape:

| Engine | Signal | Class |
| --- | --- | --- |
| llama-server | 400 with `exceed_context_size_error` | Request problem; reroute to a node with more context, not a node failure |
| llama-server | 500 for malformed JSON | Request problem, though pi will retry it; do not charge the breaker |
| Ollama | 200 with a short input token count | Silent truncation; see above |
| mlx_lm | Connection dropped on an invalid request | Request problem when the node is otherwise healthy |
| any | Connection refused, timeout, other 5xx | Node failure |

These shapes were seen on the spike versions (Ollama 0.35.1, llama-server 0.5.0, mlx_lm 0.32.0); the classifier is per engine and version, and the table needs re-checking on upgrade.

**Requests in flight.** A failed request is retried at most `REQUEST_RETRY_LIMIT` times, on the next candidate, and only if the call is safe to repeat: the model call itself has no side effects. Tool calls the agent already completed are never replayed. Moving a session to another node loses its warm prompt cache; that cost is accepted and logged.

**Failover only before the first byte.** Once any response byte has reached the harness, the router does not retry on another node. A mid-stream failure ends the stream with a recognised error or an abrupt abort, and the harness's own retry returns as a new request. That retry avoids the node that just failed, even when its breaker has not yet opened. On the OpenAI path pi retries on error wording its regex recognises or on an abrupt abort (the recommended default, as it needs no wording coupling). On the Messages path an `overloaded_error` (HTTP 529 before the first byte, or an SSE event mid-stream) makes pi and Claude Code retry; an `api_error` mid-stream makes Claude Code fall back to a non-streaming call.

**Stickiness.** A session whose node's breaker opens is re-pinned to the next surviving candidate and stays there. It does not return to the original node when the breaker closes, so the cache is not lost twice. Pins persist across router restarts; otherwise every restart would move sessions between nodes.

## Frontier budget exhaustion

The same custody and budget rules apply to every hosted pay-per-token node, not only the frontier: the credential is held by the router, the node has a `budget_gbp_day`, and the rules below apply, including no silent downgrade. Each session may also spend at most `SESSION_BUDGET_FRACTION` of the daily cap, so one runaway loop cannot starve other roles; hitting it is treated like exhaustion for that session. The daily cap is checked before dispatch, so a request already in flight when the cap is reached completes; spend can overshoot by at most one request.

- A role with another candidate falls to it. The switch breaks stickiness for that session and is logged as a cost.
- A role with no other candidate (such as `architect`, whose only candidate is `frontier`) does not run. The task is posted as `blocked` with reason `budget_exhausted`, and the orchestrator is told. Nothing is silently downgraded to a weaker model.

## Affinity

Agent requests resend a growing prefix every turn, and a prefix cache only helps if the request lands on the same server. The router therefore keys on a session ID and keeps each session on its node, spilling to the next candidate only on failure or when the preferred node's queue passes its threshold.

- **Pin header.** The key is `x-session-affinity`, on both the OpenAI and Messages paths. In pi 1.0.3 it is sent when a provider sets `compat.sendSessionAffinityHeaders: true` with `sessionAffinityFormat: "openai-nosession"`; the generated `models.json` sets both.
- **Sessionless calls are served.** pi's compaction and summary calls carry no session header. The router serves them without reading or creating a pin, on the least-loaded healthy candidate.
- **Other harnesses** send their own ids (Claude Code sends `x-claude-code-session-id`); each adapter maps its id to the pin key. DeepSeek Harness sends no session header on its chat and Anthropic routes.

## Why not an LLM router

It adds latency to every call, its mistakes are hard to trace, and the role already captures the nature of the task. Context length, tool needs and load are filters, not judgments. The one judgment worth automating is frontier-versus-local escalation; a small classifier (the technique behind vLLM Semantic Router) is a candidate for that, borrowed as a technique rather than a dependency.

## Frontier versus local orchestration

- **Frontier orchestrator (default):** better judgment about when to escalate; the main session carries growing context but each turn is short because reading and editing happen locally.
- **Local orchestrator:** cheaper, but a weaker model under-escalates. If used, rely on hard triggers: architecture keywords, repeated failure, explicit reviewer flags.
