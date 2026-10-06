# Router resilience design (archived, v2+)

> **ARCHIVED DESIGN. NOT v1 SCOPE.** Owner decision 2026-10-06, recorded in the [scope reset](../decisions/2026-10-scope-reset.md).
>
> **Why it is not v1:** For v1 an endpoint failure goes to the agent, which deals with it or not. Resilience is deferred, not rejected.
>
> **What would trigger revisiting:** v2 planning, or evidence from recorded request outcomes (the v1 seam) that endpoint failures cost real work. The spike evidence that a hung node holds pi for 300 s and its retry returns to the same node is the reason this is expected to return.
>
> This text is the design as it stood before the reset, kept so it can be picked up later. It is not maintained, and parameters and defaults named here are not in the live parameter table. Facts about pi, engines and the spikes are in the [decision record](../decisions/2026-10-spike-decisions.md).

## Circuit breaker, timeouts, failover and stickiness on failure

Each node (the frontier API included) has its own circuit breaker in the router, so a failing node stops receiving traffic quickly instead of every request waiting out a timeout.

| State | Behaviour | Moves to |
| --- | --- | --- |
| Closed | Traffic flows normally. Failures are counted; any success resets the count. | Open after `BREAKER_FAILURE_THRESHOLD` consecutive failures, or a failed health check that exceeds `FAILOVER_MAX_S` |
| Open | No requests are sent. The node is filtered out at the Filter step. | Half-open after the cooldown (`BREAKER_COOLDOWN_S`, multiplied by `BREAKER_COOLDOWN_BACKOFF_FACTOR` on each repeat opening, up to `BREAKER_COOLDOWN_MAX_S`) |
| Half-open | One probe at a time: a health call or a one-token completion, never a real agent request. The probe timeout allows for model load time on nodes behind llama-swap. | Closed after `BREAKER_PROBE_SUCCESSES` successes; open again, with the cooldown restarted, on any failure |

A failure is a connection error, a timeout (connect, first-byte or stream-idle; see [parameters](#parameters-that-belonged-to-this-design)), or a 5xx response. Request outcomes are the primary signal; the periodic health check is the backstop. A 4xx caused by the request itself is not a node failure and does not count.

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

## Parameters that belonged to this design

| Parameter | Value used in the design | Used for |
| --- | --- | --- |
| `HEALTH_POLL_INTERVAL_S` | 10 | Active node up/down polling. Secondary: request outcomes (passive detection) trip the breaker first. |
| `FAILOVER_MAX_S` | 30 | Longest allowed time from node failure to no new traffic reaching it |
| `CONNECT_TIMEOUT_S` | 5 | Connection to a node |
| `FIRST_TOKEN_TIMEOUT_BASE_S` | 30 | Fixed part of the time-to-first-token timeout |
| `FIRST_TOKEN_TIMEOUT_PER_1K_PROMPT_S` | 1 | Added per 1,000 prompt tokens, for slow prefill |
| `STREAM_IDLE_TIMEOUT_S` | 60 | Longest silence mid-stream before the request fails |
| `BREAKER_FAILURE_THRESHOLD` | 3 | Consecutive failures that open a node's breaker; a timeout counts as a failure |
| `BREAKER_COOLDOWN_S` | 30 | First wait before a probe |
| `BREAKER_COOLDOWN_BACKOFF_FACTOR` | 2 | Cooldown multiplier on each repeat opening |
| `BREAKER_COOLDOWN_MAX_S` | 300 | Cooldown ceiling |
| `BREAKER_PROBE_SUCCESSES` | 2 | Probe successes needed to close the breaker |
| `REQUEST_RETRY_LIMIT` | 1 | Retries of a failed request, on the next candidate |

## Acceptance tests that belonged to this design

   - **Accept (failover):** with a node killed mid-session, no new request reaches it within `FAILOVER_MAX_S`, and the session continues on the next candidate. A session retry after a mid-stream failure avoids the node that failed. Real-process test: real router and pi with a real engine process killed with SIGKILL.
   - **Accept (breaker):** after `BREAKER_FAILURE_THRESHOLD` consecutive failures the node's breaker opens; after `BREAKER_COOLDOWN_S` and `BREAKER_PROBE_SUCCESSES` probes it closes. Real-process test: real router with a real engine process that is stopped and restarted.
   - **Accept (timeouts):** a node that hangs before its first byte is abandoned at the first-byte timeout, and the session continues, well inside the harness's own request timeout. Real-process test: real pi through the real router to a node process that accepts and then stalls.

## Failure rows

| Failure | Effect | Behaviour |
| --- | --- | --- |
| Node down or unhealthy | Requests to it fail | Per-node circuit breaker; see [dispatcher](#circuit-breaker-timeouts-failover-and-stickiness-on-failure). A hung node is caught by the mandatory first-byte and idle timeouts. |
| Node dies mid-stream | Stream ends part way | No failover after the first byte; the harness retry avoids that node. |
| Frontier API down | Frontier calls fail | Treated as a node failure through the same circuit breaker. Roles with no other candidate block and report. |
| Client machine sleeps | Orchestration stops | All leases lapse by TTL. On wake, coord expires stale leases before any agent resumes. |

The pin-follows-drop rule in the old pin story (re-pin to the next survivor when a breaker opens, avoid the failed node on retry) belongs here too. In v1 a pinned node that is unavailable returns its error.
