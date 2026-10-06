# LLM pool allocation (leases)

Status: PROPOSED. The owner described this in one line (a subagent gets a lease on an LLM endpoint and frees it at the end); the design below is the coordinator's proposal, refined by the owner's decisions of 2026-10-06. Everything marked PROPOSED is open. The decisions and the open items are in the [decision record](../decisions/2026-10-scope-reset.md#decisions-after-the-first-cross-check).

Legatus owns allocation of the LLM pool and nothing else. The filename stays `coordination.md` so existing links resolve. The earlier coordination design (file and branch leases, task board, messaging, negotiation, guard, event store) is archived in [coordination-full-design](../archive/coordination-full-design.md) and is not v1 scope; see the [scope reset](../decisions/2026-10-scope-reset.md).

## What a lease is

A **lease** is an explicit allocation of capacity on one node of the LLM pool for the duration of a subagent task. The holder asks for an LLM by its semantic needs, is told which node, uses it, and frees it.

- **A lease limits concurrency.** Each node has a number of slots; a lease takes slots, and when every slot of every candidate is taken the pool is busy and the next acquire is refused. The pool exposes that state (see [Preflight](#preflight-and-pool-state)).
- It lives inside the router process. There is no separate coordination server, no MCP, no Unix socket and no event-store service.
- It generalises the session pin ([dispatcher](dispatcher.md#affinity-session-pin-and-lease-stickiness)). A pin is an implicit lease that takes no slot; an explicit lease is the same record with an explicit holder and lifetime. One lease table replaces the pin store; `PinView::nodes_in_use()` stays.
- A lease never moves from one node to another in v1. A leased node that is unavailable returns its error to the agent (`bound_node_unavailable`, HTTP 503).
- **Acquire is explicit in v1.** The main agent calls `can_spawn` and `request_llm` itself before it spawns a subagent, and `release_llm` when the subagent ends. The adapter supplies those tools. Automatic interception of the harness's spawn call is post-v1 (a future option, recorded in the pi adapter story's out-of-scope list). A subagent cannot request the model it already runs on, which is why the main agent does it.
- **Legatus stays harness-neutral.** The router keys only on `x-session-affinity` and `x-legatus-lease`. The adapter builds one session value (`session#agent` when the harness has agent ids) and sends it in `x-session-affinity`; the router never reads a harness's own id headers.

## Holder rules

- **A caller can free only its own lease.** Acquire records the holder value (the session value). Release must present that value; a mismatch is refused with `lease_not_yours` (HTTP 409). There is no operator override, in the API, the dashboard or the MCP tools.
- **Stuck leases are reclaimed only by the idle TTL (`LEASE_IDLE_TTL_S`, 1800 PROPOSED) or the maximum lifetime (`LEASE_MAX_LIFETIME_S`, 14400 PROPOSED).** Nobody can free another caller's lease early. That is the cost of the rule: a crashed holder keeps its slots until a limit fires.
- **Holder and session values are never returned by admin reads.** A read shows the holder's label and `holder_ref`, a non-reversible reference computed with a key kept in its own owner-only file. The key is stable across restarts, so a `holder_ref` means the same holder before and after one. The values themselves are stored only where the router needs them to check a release or route a request.

## API (PROPOSED)

Served on the router's admin listener, HTTP and JSON, under the versioned prefix `/legatus/v1/` (the admin contract; see [architecture](architecture.md#admin-surface)). Earlier drafts spelled these `/legatus/lease`, `/legatus/capacity` and so on.

| Call | Purpose |
| --- | --- |
| `POST /legatus/v1/leases` | Acquire. Body: `needs` and `holder`. Authoritative. |
| `POST /legatus/v1/leases/{id}/release` | Release. Body carries the holder value. Idempotent for an ended lease. |
| `GET /legatus/v1/leases` | List leases with node, label, `holder_ref`, age and last request. Read-only. |
| `POST /legatus/v1/capacity` | Preflight: would an acquire for these needs be granted now? Advisory; changes nothing. |
| `GET /legatus/v1/pool` | Per node: slots total and in use, and whether every resource is busy. Read-only. |

`needs` is a role, or required capabilities, plus `endpoint_kind`, `protocol` and `context_tokens`. `holder` is a session id, an optional agent id and an optional label.

A grant returns `{lease_id, node, base_url, model_id, protocol, expires_at, max_ends_at}`: `expires_at` is when the lease ends if it stays idle, `max_ends_at` is the fixed upper bound. A refusal is returned to the caller; Legatus never queues. The codes are:

| Code | Meaning |
| --- | --- |
| `no_candidate` | No node meets the needs; carries the same per-node drop reasons as the router filter (see [dispatcher](dispatcher.md#selection-per-call)) |
| `no_capacity` | Nodes meet the needs but none has a free slot (the `no_free_slot` drop reason on every candidate) |
| `unknown_role` | The needs name a role that is not in the registry |
| `invalid_request` | The body is malformed or the needs are inconsistent |
| `session_has_lease` | The session already holds a live lease; the response names it so the adapter can adopt it |
| `unknown_lease` | On release or on a request: the id was never issued, or has aged out of the ended-lease history |
| `lease_not_yours` | On release: the holder value does not match the lease (HTTP 409) |
| `lease_ended` | On a request: the lease ended (released, idle TTL or maximum lifetime); the end reason is returned |

A request that carries `x-legatus-lease` goes to the leased node alone. The router does not pick another node and does not fall back to the session pin; an ended or unknown lease is an error. A request whose `model` names a role other than the lease's role is refused with `role_mismatch`. Requests with no lease header keep working through the implicit session pin, which expires after an idle TTL.

The per-session header is the weak point. `models.json` is static, so a header that changes per lease needs a per-request hook in the adapter. That a pi 1.0.3 extension can set a header after acquire, **on pi's chat path**, is an ASSUMPTION and NOT TESTED; the first task of the pi adapter story is a spike that proves it before the design relies on it. The admin address the adapter calls comes from the registry's `router` section (see [registry](registry.md#router)).

**Control surface.** In v1 the only state-changing admin calls are lease acquire and release. Everything else the admin API serves is read-only. The scope and audit rules are in [architecture](architecture.md#admin-surface).

## Preflight and pool state

The preflight (`POST /legatus/v1/capacity`) and the pool view (`GET /legatus/v1/pool`) let a caller see before it acts whether capacity exists, and let the dashboard show when all resources are busy. They are **advisory and non-binding**: another caller can take the last slot between a preflight and an acquire. The acquire is authoritative and may still refuse. Both are reads and change no state. The `can_spawn` tool in the adapter calls the preflight.

## Capacity

Each node has a `slots` field in the registry (see [registry](registry.md#fields)). The rule, stated once: slots is the registry value, or the engine default when the registry gives none, capped by the measured concurrency from the calibration probe. Engine defaults (PROPOSED):

| Engine | Default slots | Basis |
| --- | --- | --- |
| Ollama | 1 | Serial in the spike |
| llama-server | 4 | The count the spike ran with; it is the run configuration, not an engine constant |
| mlx_lm | 1 | The spike saw continuous batching, so set it in the registry if the measurement supports more |
| Hosted | 4 | PROPOSED; no measurement |
| Apple shim | 1 | One on-device session at a time |

A lease takes one slot, or all of them when the role's `exclusive` flag is true (a boolean on the role; there are no named resources). Capacity is a filter stage that runs after the context check, and a node with no free slot is dropped with `no_free_slot`.

Slots count lease holders only. The router does not block requests that arrive without a lease (sessionless calls, implicit pins) and does not measure real engine concurrency. Whether leased and unleased traffic should share the count is OPEN.

With no free slot on any candidate the call is refused at once. There is no waiting queue in v1; the caller decides whether to retry, pick another need, or report.

## Lifetime

A lease ends on release, after `LEASE_IDLE_TTL_S` (1800, PROPOSED) with no requests, or at `LEASE_MAX_LIFETIME_S` (14400, PROPOSED). Story 9 owns the catalogue entries. A request still in flight keeps the lease alive.

There is no heartbeat, no renewal timer, no guard cache and no dual-clock machinery in the harness. The router observes the requests it already sees, so a harness that forgets to release is reclaimed by the idle TTL.

- **Silent tool runs.** A tool run with no model request for longer than the idle TTL ends the lease, and the next request gets `lease_ended`. There is no "long call" signal in v1. An ended lease never silently falls back to the session pin.
- **Sleep and wake are not counted as idle.** Time the machine spends asleep does not count towards the idle TTL. How to read a clock that survives macOS sleep is OPEN: NOT TESTED.
- **Ended history.** The router keeps the last `ENDED_LEASE_HISTORY` (200, PROPOSED constant) ended leases so it can answer `lease_ended` and not `unknown_lease`.

## Persistence and restart

This section is about Legatus's own state. It is not the paused v2 router resilience (timeouts, retries, breakers, failover), which concerns requests to nodes.

The lease table is an append-only JSONL journal inside the router, replayed at start. The journal path is `lease_journal_path` in the router's `router_settings` block, next to `admin_addr`. The table types (`LeaseStore`, `LeaseRecord`, `NodeSlots`) are defined in the lease store story. PROPOSED rules:

| Situation | Behaviour |
| --- | --- |
| Journal durability | A durability policy states when a journal line is flushed to disk, and so what a crash can lose. The policy and its cost are chosen in the journal story; none is measured |
| Start order | The router listens early and serves only after replay finishes. Restart-to-ready time is NOT MEASURED |
| After replay | A surviving lease gets one fresh idle TTL; leases past their maximum lifetime end; a recovery report says how many journal lines were replayed, skipped and ended |
| Bad or unreadable files | Degraded start: the router starts, says what it could not read, and serves with what it has. A damaged journal line is skipped; an unreadable journal gives an empty table. A request for a lease it no longer knows gets `unknown_lease` |
| Second router started | A lock on the state files refuses a second router instead of letting two write one journal |
| Machine sleeps and wakes | Not counted as idle (see [Lifetime](#lifetime)) |
| A leased node restarts | The lease stays; requests fail until the node is back, and the error goes to the agent. The lease does not move |
| Router crash (SIGKILL) | Same as restart; what the journal policy flushed is what survives |

The router logs metadata only: no prompt text and no completion text. The lease journal stores a keyed digest of the holder value and its reference, never the raw value (decided in the lease store story, 52). The session pin store keeps raw session values for routing; it is owner-only and never an admin read.

## What it does not do

- It does not lock files, branches, worktrees, GPUs or any resource other than LLM capacity.
- It does not spawn, supervise or collect results from agents, and it does not verify what an agent did. Agents lie, so `done` is never a Legatus claim.
- It does not move a lease on failure, retry, or queue.
- It does not let an operator or another caller free a lease it does not hold; there is no override.
- It does not intercept the harness's spawn call in v1; the main agent acquires explicitly.
- It does not act on what it observes of a request. Truncation detection logs and records only; a bound request returns the error. The pre-send prompt-token guard may still return 400.
- It does not need MCP, a blocking hook, a delegate tool or a background timer in the harness.
- It does not offer drain, table reload, calibration or canary runs through the admin surface in v1.

## Acceptance tests (real process)

Each runs the real router, with real engine processes where the behaviour depends on the engine, and records engine and harness versions beside the result. Scripted fakes and the simulated cluster (see [roadmap](roadmap.md#simulated-cluster-tests)) are used only where a real failure cannot be produced on demand.

- **Grant and release.** A real client acquires a lease, sends requests with `x-legatus-lease` through the real router to a real engine, releases with its holder value, and the lease is gone from the list.
- **Holder rule.** A release with another holder value is refused with `lease_not_yours`; no admin route frees it; it ends only at the idle TTL or maximum lifetime.
- **Hidden holder.** No admin read, dashboard page or MCP result contains a holder or session value; the `holder_ref` of one holder is the same before and after a router restart.
- **Capacity and preflight.** On a serial engine (Ollama) a second acquire is refused with `no_capacity` while the first is live and granted after release. The preflight and pool view report the same state without changing it, and an acquire after a preflight that said "free" can still be refused when another caller took the slot. An `exclusive` role takes every slot.
- **Refusal reasons.** An acquire whose needs no node meets returns `no_candidate` with a drop reason per node, matching the router filter.
- **Idle and maximum lifetime.** A lease with no requests ends at `LEASE_IDLE_TTL_S` and the next request gets `lease_ended`; a lease kept busy ends at `LEASE_MAX_LIFETIME_S`.
- **Codes.** `session_has_lease`, `unknown_lease`, `unknown_role`, `invalid_request` and `lease_not_yours` each come back for the input that should cause them.
- **Restart and crash.** After SIGKILL and restart of the router, live leases are back with a fresh idle TTL and released leases stay released; a damaged journal gives a degraded start, not a failed one; a second router is refused; a node restart while leased leaves the lease in place.
- **Implicit pin.** A real pi session with no explicit lease stays on one node across turns and across a router restart.
- **Real harness.** A real pi 1.0.3 process, through the adapter's `can_spawn`, `request_llm` and `release_llm` tools, runs a task on the leased node.

## Failure modes

| Failure | Effect | Behaviour |
| --- | --- | --- |
| No free slot | Acquire cannot be granted | Refused with `no_capacity`; the caller decides. A preflight reports it beforehand, advisory only. |
| No node meets the needs | Acquire cannot be granted | Refused with `no_candidate` and per-node drop reasons. |
| Harness never releases, or holder crashed | Capacity held | Reclaimed at the idle TTL or the maximum lifetime; nobody can free it earlier. |
| Release by a non-holder | Refused | `lease_not_yours`. |
| Router restarts or crashes | In-memory table lost | Journal replay; survivors get one fresh idle TTL. |
| Journal unreadable | Table cannot be rebuilt | Degraded start with an empty table, and the router says so. PROPOSED. |
| Leased node down or restarted | Requests fail | The error goes to the agent; the lease does not move. |
| Ended lease id on a request | Cannot route | `lease_ended` (HTTP 409) with the end reason; no fallback to another node or to the pin. PROPOSED. |
| Unknown lease id on a request | Cannot route | `unknown_lease`; the router does not fall back to another node. PROPOSED. |
| Silent tool run longer than the idle TTL | Lease ended while the agent works | The next request gets `lease_ended`; the adapter may acquire again. PROPOSED. |

## Parameters (PROPOSED)

| Parameter | Value | Used for |
| --- | --- | --- |
| `LEASE_IDLE_TTL_S` | 1800 | Lease ends after this long with no requests |
| `LEASE_MAX_LIFETIME_S` | 14400 | Upper bound on any lease |
| `SESSION_PIN_IDLE_TTL_S` | 3600 | Idle expiry of the implicit pin |
| `ENDED_LEASE_HISTORY` | 200 | Constant, not a catalogue parameter: ended leases remembered so `lease_ended` can be told from `unknown_lease` |

All values are PROPOSED defaults, not measurements. Nothing in the spikes ran a lease. The journal path and the holder-reference key file are settings, not catalogue parameters. Tests may use small TTL values; how the catalogue allows that is decided in the parameter catalogue story.

## Seams for later

- A waiting queue instead of immediate refusal, if callers need it.
- Automatic acquire by intercepting the harness's spawn call.
- A lease-only token scope, so an adapter does not hold a full control token (a paused story in the security epic).
- Moving a lease on node failure, with the resilience work in [dispatcher](dispatcher.md#v2-resilience).
- Leases on non-LLM resources, if that is ever reopened; the old design is in the [archive](../archive/coordination-full-design.md).
- Per-machine power and thermal readings as an input to the capacity decision (see the [roadmap](roadmap.md)).
