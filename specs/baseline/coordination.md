# LLM pool allocation (leases)

Status: PROPOSED. The owner described this in one line (a subagent gets a lease on an LLM endpoint and frees it at the end); the design below is the coordinator's proposal and the owner will iterate. Everything marked PROPOSED is open.

Legatus owns allocation of the LLM pool and nothing else. The filename stays `coordination.md` so existing links resolve. The earlier coordination design (file and branch leases, task board, messaging, negotiation, guard, event store) is archived in [coordination-full-design](../archive/coordination-full-design.md) and is not v1 scope; see the [scope reset](../decisions/2026-10-scope-reset.md).

## What a lease is

A **lease** is an explicit allocation of capacity on one node of the LLM pool for the duration of a subagent task. The holder asks for an LLM by its semantic needs, is told which node, uses it, and frees it.

- It lives inside the router process. There is no separate coordination server, no MCP, no Unix socket and no event-store service.
- It generalises the session pin ([dispatcher](dispatcher.md#affinity-session-pin-and-lease-stickiness)). A pin is an implicit lease; an explicit lease is the same record with an explicit holder and lifetime. One lease table replaces the pin store; `PinView::nodes_in_use()` stays.
- A lease never moves from one node to another in v1. A leased node that is unavailable returns its error to the agent.

## API (PROPOSED)

Served on the router's admin listener, HTTP and JSON.

| Call | Purpose |
| --- | --- |
| `POST /legatus/lease` | Acquire. Body: `needs` and `holder`. |
| `POST /legatus/lease/{id}/release` | Release. Idempotent. |
| `GET /legatus/leases` | List live leases, with node, holder, age and last request. |

`needs` is a role, or required capabilities, plus `endpoint_kind`, `protocol` and `context_tokens`. `holder` is a session id, an optional agent id and an optional label.

A grant returns `{lease_id, node, base_url, model_id, protocol, expires_at}`. A refusal is `no_capacity` or `no_candidate`; `no_candidate` carries the same per-node drop reasons as the router filter (see [dispatcher](dispatcher.md#selection-per-call)).

Requests then carry `x-legatus-lease: <id>`, with or without the session header, and the router sends them to the leased node. Requests with no lease keep working through the implicit session pin, which expires after an idle TTL.

## Capacity

Each node has a number of slots (concurrency), taken from the registry or from engine facts: Ollama is serial (one slot), llama-server has several (4 in the spike), and mlx_lm is treated as one slot by default although the spike saw continuous batching, so its slot count comes from the registry. A lease takes one slot, or all of them when the role is `exclusive`.

With no free slot the call is refused at once. There is no waiting queue in v1; the caller decides whether to retry, pick another need, or report.

## Lifetime

A lease ends on release, after `LEASE_IDLE_TTL_S` with no requests, or at `LEASE_MAX_LIFETIME_S`. Both are PROPOSED parameters with no value chosen yet; story 9 owns the catalogue entries.

There is no heartbeat, no renewal timer, no guard cache and no dual-clock machinery in the harness. The router observes the requests it already sees, so a harness that forgets to release is reclaimed by the idle TTL.

## Persistence

The lease table is an append-only JSONL journal inside the router, replayed at start. After a restart a surviving lease gets one fresh idle TTL. Nothing else is persisted for leases.

## What it does not do

- It does not lock files, branches, worktrees, GPUs or any resource other than LLM capacity.
- It does not spawn, supervise or collect results from agents, and it does not verify what an agent did. Agents lie, so `done` is never a Legatus claim.
- It does not move a lease on failure, retry, or queue.
- It does not need MCP, a blocking hook, a delegate tool or a background timer in the harness.

## Acceptance tests (real process)

Each runs the real router, with real engine processes where the behaviour depends on the engine, and records engine and harness versions beside the result. Scripted fakes are used only where a real failure cannot be produced on demand.

- **Grant and release.** A real client acquires a lease, sends requests with `x-legatus-lease` through the real router to a real engine, releases, and the lease is gone from `GET /legatus/leases`.
- **Capacity.** On a serial engine (Ollama) a second acquire is refused with `no_capacity` while the first is live and granted after release. An `exclusive` role takes every slot.
- **Refusal reasons.** An acquire whose needs no node meets returns `no_candidate` with a drop reason per node, matching the router filter.
- **Idle and maximum lifetime.** A lease with no requests ends at `LEASE_IDLE_TTL_S`; a lease kept busy ends at `LEASE_MAX_LIFETIME_S`.
- **Restart.** After SIGKILL and restart of the router, live leases are back with a fresh idle TTL and released leases stay released.
- **Implicit pin.** A real pi session with no explicit lease stays on one node across turns and across a router restart.
- **Real harness.** A real pi 1.0.3 process, through the adapter's acquire and release surface, runs a task on the leased node.

## Failure modes

| Failure | Effect | Behaviour |
| --- | --- | --- |
| No free slot | Acquire cannot be granted | Refused with `no_capacity`; the caller decides. |
| No node meets the needs | Acquire cannot be granted | Refused with `no_candidate` and per-node drop reasons. |
| Harness never releases | Capacity held | Reclaimed at the idle TTL or the maximum lifetime. |
| Router restarts | In-memory table lost | Journal replay; survivors get one fresh idle TTL. |
| Journal unreadable | Table cannot be rebuilt | Router starts with an empty table and says so; running agents' next request with an unknown lease id returns a clear error. PROPOSED. |
| Leased node down | Requests fail | The error goes to the agent; the lease does not move. |
| Unknown or released lease id on a request | Cannot route | Error naming the lease id; the router does not fall back to another node. PROPOSED. |

## Parameters (PROPOSED)

| Parameter | Value | Used for |
| --- | --- | --- |
| `LEASE_IDLE_TTL_S` | not chosen | Lease ends after this long with no requests |
| `LEASE_MAX_LIFETIME_S` | not chosen | Upper bound on any lease |
| `SESSION_PIN_IDLE_TTL_S` | not chosen | Idle expiry of the implicit pin, kept while the implicit pin remains |
| `LEASE_JOURNAL_*` | only if journal recovery needs them | Journal settings |

## Seams for later

- A waiting queue instead of immediate refusal, if callers need it.
- Moving a lease on node failure, with the resilience work in [dispatcher](dispatcher.md#v2-resilience).
- Leases on non-LLM resources, if that is ever reopened; the old design is in the [archive](../archive/coordination-full-design.md).
- Per-machine power and thermal readings as an input to the capacity decision (see the [roadmap](roadmap.md)).
