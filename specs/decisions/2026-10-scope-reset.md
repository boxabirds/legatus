# Scope reset, 2026-10-06

Status: owner decisions recorded, last updated 2026-10-06 after further decisions (see [Decisions after the first cross-check](#decisions-after-the-first-cross-check)); the lease design is PROPOSED and open. Partially supersedes [2026-10-spike-decisions](2026-10-spike-decisions.md).

## Context

After the October 2026 spikes the baseline had grown into a coordination platform: leases on files, branches and named resources, a task board, messaging, a guard and sandbox, worktree isolation, a delegate tool with an acceptance check, remote execution, an escalation gate, price and budget, and a full resilience layer. The owner reviewed it and reduced the product to what it is for: a heterogeneous proxy that semantically links a subagent task with a local inference resource, owning the LLM pool it exposes and nothing else.

## Decisions (owner, 2026-10-06)

1. **Coordination is LLM pool allocation only.** A subagent gets a lease on an LLM endpoint and frees it at the end. No other resources (file systems, branches, worktrees, GPUs as locks, merges). The rest of the coordination is not relevant.
2. **Delegation, worktrees, guard and sandbox, and remote execution are out.** Spawning, results and worktrees are far outside scope; guard and sandbox are harness concerns; remote execution is not relevant. A subagent requests an LLM by its semantic needs, gets it, and frees it.
3. **Router resilience is v2 or later.** For v1 an endpoint failure goes to the agent, which deals with it or not. Design for the future, code for now (see the seams below).
4. **Agents lie, so Legatus does not verify.** `done` could only mean that the agent reported success; the acceptance check is cut.
5. **The escalation gate is cut.** Legatus does not decide when a role may be called.
6. **Price and budget are out** (already deferred before this decision).
7. **Power and thermal are in, per machine,** with regimes, supplied by the node agent.
8. **Behavioural telemetry is in scope** for the capability and behavioural profile; the part that stays in v1 was decided afterwards (see [Decisions after the first cross-check](#decisions-after-the-first-cross-check)).
9. **Selection is at the subagent task,** not the request (already a vision principle).
10. **Conversation capture is a separate product.** Decided after the first cross-check (below).
11. **Collection consent.** A collection consent switch, default off, with onboarding consent. Operational metadata is always collected.

What stays in v1: the registry (three-layer node descriptor, roles with requires, endpoint kind and protocol), the router, the node agent, capability and behavioural profiles, power and thermal, config generation, reduced harness adapters, and credential custody of a hosted node's key inside the router (reduced).

## Consequences: what is lost by each omission

- **No resilience (v2+).** A hung node holds pi for its 300 s request timeout and pi's retry returns to the same node. With a 3 s first-byte timeout pi recovered in 3.4 s in the spike. In v1 that failure is visible to the agent and the person, and nothing recovers it.
- **No guard, sandbox or worktrees.** In the spike a committed symlink escaped a worktree, and a child launched without the guard was caught only by a watcher and a sandbox profile. Legatus now offers none of that. Confinement of writes is the harness's responsibility, or the user's.
- **No acceptance check.** A real Qwen3 1.7B model looped on a wait call and reported done with no changes. Legatus will not catch this; it never claims work is done.
- **No escalation gate, no price or budget.** Nothing in Legatus stops a role from being called often or spend from growing. The router records requests, so the data to add this later exists.
- **No coordination of non-LLM resources.** Two agents can still collide over files or a shared GPU lock; that is outside the product.
- **No remote execution.** Roles that need to run where the hardware is (for example CUDA tests) are the harness's concern.

## What stays proven by the spikes

Router: byte-identical streaming, added latency of 0.06 ms to first byte and 3 to 6 MB resident memory, both protocols per role type with one protocol per role, session pinning on `x-session-affinity`, pins persisting across restarts, Messages-path retry behaviour. Engines: the Ollama, llama-server and mlx_lm differences in the ADR (silent truncation, 400 on overflow, load signals, concurrency). pi 1.0.3 provider config and session headers, Claude Code through the router, the Apple on-device node, and the calibration probe. All of these were seen with scripted fakes or a 1.7B model on one Apple M2. See the [spike record](2026-10-spike-decisions.md) for versions and limits. The spike evidence for the removed designs stays in that record and is not deleted.

## The two seams (design for the future, code for now)

- **One upstream call site** in the router, so timeouts, retries and breakers can be added in one place in v2.
- **A recorded outcome for every request,** observed and not acted on, so the future resilience work and any telemetry have data.

## Proposed lease design (PROPOSED)

From the owner's one-line description, the coordinator proposed: a lease table inside the router persisted as an append-only JSONL journal; an admin API (acquire, release and list, later versioned under `/legatus/v1/` and extended with a preflight and a pool view; see the later decisions below); slots per node, one per lease or all for an `exclusive` role; immediate refusal with `no_capacity` or `no_candidate` and no queue; lifetime by release, idle TTL and maximum lifetime, observed from requests with no heartbeat; one fresh idle TTL after a restart; the pin as an implicit lease; a lease never moves between nodes in v1. Full text in [LLM pool allocation](../baseline/coordination.md). The owner will iterate on it.

## Open items at the first reset

- Conversation capture and advanced telemetry were settled by the first cross-check (below).
- The lease design above, including whether a waiting queue is wanted and the journal-loss behaviour. The parameter values now have PROPOSED defaults (below).
- What the machine regimes are. The `machines` section and the `machine` node field are accepted.
- Whether any role-level tool permission profile stays in the registry; it was dropped from the docs along with the guard.

## Decisions after the first cross-check

Two read-only audits of the surviving stories (2026-10-06) found contradictions between stories and between stories and these docs. The owner decided the following. Items marked PROPOSED are defaults the coordinator chose and the owner can change.

1. **Conversation capture is a separate epic and product, not v1.** The epic is retitled 'SEPARATE PRODUCT (not v1)' and its stories are paused, except the router event log and the router event stream into the behavioural record, which stay active because telemetry needs them (moved into the behavioural-telemetry epic where the tools allow).
2. **Advanced telemetry is a separate low-priority epic.** Drift detection, backtesting and routing on observed behaviour move to a new epic `behavioural-telemetry-advanced` ('LOW PRIORITY (post-v1)'), paused. The v1 telemetry stories (identity, live record, energy, thermal) only record and report: nothing in v1 makes the router act on continuous observed behaviour. Join-time measurement (calibration probe, canaries, the stricter of declared and measured) still feeds routing.
3. **Lease defaults (PROPOSED).** `LEASE_IDLE_TTL_S` 1800 and `LEASE_MAX_LIFETIME_S` 14400, with the parameter catalogue story owning the entries. `exclusive` is a boolean role flag meaning the role takes every slot of its node; the named-resource design is dropped. Truncation detection only logs and records: a bound request returns the lease error, and only the pre-send prompt-token guard may return 400. `slots` is a node field in the registry, defaulting per engine (Ollama 1, llama-server 4 as run, mlx_lm 1, hosted 4 PROPOSED, Apple shim 1) and capped by measured concurrency.
4. **Lease design answers (PROPOSED).** The host's adapter acquires when it spawns a subagent and releases when the subagent ends, because a subagent cannot request the model it already runs on. A refused acquire is returned to the caller; Legatus never queues in v1. A silent tool run longer than the idle TTL ends the lease and the next request gets `lease_ended`; there is no 'long call' signal in v1. An ended lease never silently falls back to the session pin.
5. **Agent identity is the adapter's job.** The adapter produces one session value (`session#agent` when the harness has agent ids) and sends it in `x-session-affinity`. The router keys only on `x-session-affinity` and `x-legatus-lease` and stays harness-neutral.
6. **Machine readings belong to the node agent.** Power, temperature and throttling are owned per machine by `legatus-node`; the telemetry stories consume them; the capture epic's node modules are paused with it. A `machines` section and a `machine` field on nodes are accepted in the registry.
7. **No resilience in v1.** Timeouts, retries, breakers and failover stay paused (v2+). Two seams only: one upstream call site, and one recorded outcome per request.
8. **Shared names, one owner each.** The router settings block `router_settings {admin_addr, lease_journal_path}` is owned by the router config story, and the registry `router` section carries both the router and admin addresses. The `LeaseStore`, `LeaseRecord`, `NodeSlots` and `ENDED_LEASE_HISTORY` names are owned by the lease store story. `DropReason::NoFreeSlot` and the capacity stage are owned by the router filter story. The request id header is `x-legatus-request-id`, and the router strips every `x-legatus-*` header before a request goes to a hosted node. The upstream story owns the transport outcome, the failure-table story maps it to one of five classes, and one tap and one event type at the upstream call site are owned by the upstream story. `ThermalBand` (`Normal`, `Warm`, `Hot`, `Throttling`, `Unknown`) lives in `legatus-common`, owned by the thermal story. Protocol spellings are `openai-chat` and `anthropic-messages`, the endpoint field is `endpoint_kind`, and `limits.on_overflow` is `error_400`, `silent_truncate` or `unbounded`.

### Later decisions (all 2026-10-06)

Each has its consequence. Items marked PROPOSED are values or shapes the coordinator chose for the owner to confirm.

**Leases and the pool**

9. **Leases limit concurrency (slots), and the pool exposes when all resources are busy.** Consequence: capacity is a count of slots per node; `GET /legatus/v1/pool` shows total and used per node. Slots count lease holders only; whether unleased traffic should share the count is OPEN. Spike evidence: a queued Ollama request waited 44 s for its first byte, which is what an unlimited number of callers on a serial engine looks like.
10. **A non-binding capacity preflight is advisory; acquire is authoritative.** `POST /legatus/v1/capacity` changes nothing and can be wrong a moment later. Consequence: `can_spawn` can answer cheaply, and acquire must still handle `no_capacity`.
11. **Acquire is explicit in v1.** The main agent calls `can_spawn` and `request_llm` before it spawns, and `release_llm` after. Automatic interception of the spawn call is post-v1 and is recorded as a future option in the pi adapter story. This replaces the earlier proposal that the adapter acquires on spawn. Consequence: a harness that forgets to call holds nothing and is unlimited; the idle TTL only reclaims what was acquired.
12. **A caller can free only its own lease.** Release takes the holder value; a mismatch is refused `lease_not_yours` (HTTP 409); there is no operator override. Stuck leases are reclaimed only by the idle TTL (1800 s PROPOSED) or maximum lifetime (14400 s PROPOSED). Consequence: nobody, including the dashboard, can free a crashed holder's slots early. This reverses an earlier "no holder check" draft.
13. **Admin reads never return holder or session values.** They show a label and `holder_ref`, a non-reversible reference whose key lives in its own owner-only file and is stable across restarts. Consequence: the dashboard cannot release another caller's lease, because it does not hold the value.

**Admin surface**

14. **New epic `admin-surface`:** story 95 (contract, `/legatus/v1/`), 96 (auth and safety: `read` and `control` scopes, audit), 97 (dashboard shell), 98 (MCP shell), 99 (parity and leak checks). One admin story and one MCP story per live epic after that. No AppleScript API (this overrides the general parity rule for this product).
15. **Dashboard and MCP run in a separate small stateless process, `legatus-admin`,** a thin client forwarding the caller's own token. Consequence: the router stays small; the dashboard can show "router unreachable"; one more always-on process, footprint NOT MEASURED.
16. **The v1 control surface is lease acquire and release only.** `table.reload`, drain, and calibration or canary runs were dropped; no placeholder, 501 or "planned" routes. A route appears when its feature is built.
17. **A lease-only token scope** is a paused backburner story in the security epic. Consequence: until then an adapter that acquires holds a control token, which is broader than it needs.

**Testing and resilience**

18. **A simulated cluster test epic is required:** stub engines, virtual time, seeded concurrency checks and a scenario suite (crates `legatus-simcluster`, `legatus-simtests`). Consequence: lease lifetimes and restarts can be tested in useful time; the real-process tests remain the proof.
19. **Restart and crash resilience of Legatus's own state is in v1:** journal durability policy, listen early and serve after replay, degraded start on bad files, a lock against a double start, sleep and wake not counted as idle, node restarts while leased. This is explicitly NOT the paused v2 router resilience, which is about node failures. Consequence: the earlier text that a lease may outlive its time after a sleep is replaced by "sleep is not idle time", whose clock is unproven.

**Logging and records**

20. **The router logs metadata only:** no prompt or completion content. Conversation capture is a separate product, not v1.
21. **Event and telemetry logs keep everything in v1, with a fixed free-disk guard; there is no stale limit for records.** Consequence: a retention-days style parameter is not a v1 parameter, and disk use grows until the guard trips (what the guard does then is set in the event log story).
22. **Telemetry stories 70 to 72** (drift, backtesting, routing on observed behaviour) sit in a separate low-priority epic; stories 68, 69, 73 and 74 stay v1 and only record and report.

**Order and cuts**

23. **Story order and epic build order** are stored as story priorities and a "Build order" line in each epic description. The recommended first vertical slice: workspace, stubs and the virtual-time spike, version pins, parameters, registry, smoke verdicts, config generation, routing table, cluster stubs, router passthrough with a real pi 1.0.3 session; then leases with the preflight; then the pi adapter, with the header-hook spike first.
24. **Cuts:** story 14 drops queue thresholds (one sessionless rule: fewest in flight, then fewest leased slots, then registry order); story 51 drops ping injection and the normaliser; story 56 only logs truncation detection; the canary library has one home, story 60 (the duplicate in story 2 was removed); machine readings are owned by the node agent; the adapter contract lives in `packages/adapter-contract`.
25. **Explicit non-goals:** a waiting queue; automatic acquire in v1; an operator override of leases; admin control beyond lease acquire and release; an AppleScript API; placeholder routes; an embedded database (records are JSONL); router resilience for node failures in v1; conversation content in router logs; editing the registry through the admin surface.

**Spike evidence behind the consequences**

- A hung node held pi for its 300 s request timeout, then pi's retry went to the same node; a 3 s first-byte timeout let pi recover in 3.4 s. Reason for the v2 resilience seam and for the stuck-lease rule: nothing in v1 rescues a hung call.
- A committed symlink escaped a worktree. Confinement is the harness's problem; Legatus offers none.
- A real Qwen3 1.7B model reported done with no changes. Legatus never claims work is done.
- A queued Ollama request took 44 s to its first byte (item 9).

All of it was seen on one Apple M2 with a 1.7B model and scripted fakes. See the [spike record](2026-10-spike-decisions.md).

### Open items after the first cross-check

Honest list of what is unproven or unconfirmed. Nothing here has been run against Legatus.

- **Header hook after acquire.** That a pi 1.0.3 extension can set the lease header after acquire on pi's chat path is NOT TESTED; the whole explicit-lease path leans on it. The first task of the pi adapter story is the spike.
- **Paused-time router.** That the real router can run on virtual or paused time for the simulator is NOT TESTED.
- **Restart-to-ready time** (replay plus listen) is NOT MEASURED, and so is what the journal durability policy costs.
- **macOS sleep clock.** Which clock survives sleep, so that sleep is not counted as idle, is NOT TESTED.
- **MCP library choice.** A hand-written minimal server against a pinned library is undecided; pi 1.0.3's MCP client against `legatus-admin` is NOT TESTED.
- **Sidecar footprint.** Memory and CPU of `legatus-admin`, and of the router with the admin API, are NOT MEASURED.
- **27 placeholder defaults** in the stories were chosen by agents, not by the owner and not from measurement; they need confirming or measuring.
- **Unleased traffic and slots.** Whether sessionless calls and implicit pins should count against slots.
- **Hosted slot default** (4) has no measurement.
- **Machine regimes** and any role-level tool permission profile are still undecided (carried from the first reset).

Consequences for the docs: the baseline docs now carry the lease codes (`lease_ended`, `session_has_lease`, `unknown_lease`, `unknown_role`, `invalid_request`, `no_capacity`, `no_candidate`), the bound-node rule, the sessionless selection rule, the failure classes, the registry fields above and the PROPOSED defaults. Claims the audits found unproven are labelled ASSUMPTION or NOT TESTED in place, most importantly that a pi 1.0.3 extension can set a per-session header after acquire.

## Where the removed designs went

All in the [archive](../archive/): [coordination-full-design](../archive/coordination-full-design.md), [router-resilience-design](../archive/router-resilience-design.md), [delegation-worktrees-guard-remote-design](../archive/delegation-worktrees-guard-remote-design.md), [budget-design](../archive/budget-design.md).
