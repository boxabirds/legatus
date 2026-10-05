# Dispatcher and router

The orchestrator chooses the **role**; Legatus chooses the **node**. The `delegate(role, task, hints)` tool is a thin pi extension. The selection logic lives in `legatus-router`, so it is shared by every agent and outlives any one pi process.

## Selection, per call

1. **Resolve role.** Look up the role's candidates, permissions and leases. If the role is `escalation_only`, require a stated trigger in `hints`.
2. **Filter.** Drop nodes that fail health, have an open circuit breaker, lack the context for prompt plus expected output, or are over budget (frontier).
3. **Stick.** If this session already ran on a surviving candidate, reuse it so its prompt cache stays warm.
4. **Score by load.** Otherwise take the first candidate whose queue is under its threshold; if none, the least loaded.
5. **Acquire leases.** Claim any `exclusive` resource and the worktree through `legatus-coord`; on failure, queue and report back.
6. **Spawn.** Launch the subagent via pi-subagents with the chosen model and permission profile, and record the run on the task board.

## Live signals

| Source | Signal | Polled |
| --- | --- | --- |
| llama-server `/slots`, `/metrics` | busy slots, queued requests, tokens/s | every `LOAD_POLL_INTERVAL_S` |
| vLLM `/metrics` | running and waiting requests | every `LOAD_POLL_INTERVAL_S` |
| Router health checks or `legatus-node` | node up or down | every `HEALTH_POLL_INTERVAL_S` |
| Router spend tracking | frontier budget remaining | per call |

## Context estimate and empty candidate list

- **Estimator.** Required context is the prompt token count plus the role's expected output tokens, or `OUTPUT_TOKEN_ESTIMATE_DEFAULT` when the role gives none. Prompt tokens come from the engine's tokenizer endpoint where available, else a conservative character-based count.
- **No candidate survives.** If the filter step leaves no candidate, `delegate` returns an error naming each dropped node and why (unhealthy, breaker open, context too small, over budget). It does not guess or fall back outside the role's `candidates`. The task is posted as `blocked` with that reason.

## Circuit breaker

Each node (the frontier API included) has its own circuit breaker in the router, so a failing node stops receiving traffic quickly instead of every request waiting out a timeout.

| State | Behaviour | Moves to |
| --- | --- | --- |
| Closed | Traffic flows normally. Failures are counted; any success resets the count. | Open after `BREAKER_FAILURE_THRESHOLD` consecutive failures, or a failed health check that exceeds `FAILOVER_MAX_S` |
| Open | No requests are sent. The node is filtered out at the Filter step. | Half-open after the cooldown (`BREAKER_COOLDOWN_S`, multiplied by `BREAKER_COOLDOWN_BACKOFF_FACTOR` on each repeat opening, up to `BREAKER_COOLDOWN_MAX_S`) |
| Half-open | One probe at a time: a health call or a one-token completion, never a real agent request. The probe timeout allows for model load time on nodes behind llama-swap. | Closed after `BREAKER_PROBE_SUCCESSES` successes; open again, with the cooldown restarted, on any failure |

A failure is a connection error, a timeout (connect, first-token or stream-idle; see [parameters](architecture.md#parameters)), or a 5xx response. Request outcomes are the primary signal; the periodic health check is the backstop. A 4xx caused by the request itself is not a node failure and does not count.

**Requests in flight.** A failed request is retried at most `REQUEST_RETRY_LIMIT` times, on the next candidate, and only if the call is safe to repeat: the model call itself has no side effects. Tool calls the agent already completed are never replayed. Moving a session to another node loses its warm prompt cache; that cost is accepted and logged.

**Stickiness.** A session whose node's breaker opens is re-pinned to the next surviving candidate and stays there. It does not return to the original node when the breaker closes, so the cache is not lost twice.

## Frontier budget exhaustion

Each session may also spend at most `SESSION_BUDGET_FRACTION` of the daily cap, so one runaway loop cannot starve other roles; hitting it is treated like exhaustion for that session. The daily cap is checked before dispatch, so a request already in flight when the cap is reached completes; spend can overshoot by at most one request.

- A role with another candidate falls to it. The switch breaks stickiness for that session and is logged as a cost.
- A role with no other candidate (such as `architect`, whose only candidate is `frontier`) does not run. The task is posted as `blocked` with reason `budget_exhausted`, and the orchestrator is told. Nothing is silently downgraded to a weaker model.

## Affinity

Agent requests resend a growing prefix every turn, and a prefix cache only helps if the request lands on the same server. The router therefore keys on a session ID and keeps each session on its node, spilling to the next candidate only on failure or when the preferred node's queue passes its threshold. pi's `sessionAffinityFormat` already emits session headers the router can use.

## Why not an LLM router

It adds latency to every call, its mistakes are hard to trace, and the role already captures the nature of the task. Context length, tool needs and load are filters, not judgments. The one judgment worth automating is frontier-versus-local escalation; a small classifier (the technique behind vLLM Semantic Router) is a candidate for that, borrowed as a technique rather than a dependency.

## Frontier versus local orchestration

- **Frontier orchestrator (default):** better judgment about when to escalate; the main session carries growing context but each turn is short because reading and editing happen locally.
- **Local orchestrator:** cheaper, but a weaker model under-escalates. If used, rely on hard triggers: architecture keywords, repeated failure, explicit reviewer flags.
