# Scope reset, 2026-10-06

Status: owner decisions recorded; the lease design is PROPOSED and open. Partially supersedes [2026-10-spike-decisions](2026-10-spike-decisions.md).

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
8. **Behavioural telemetry is in scope** for the capability and behavioural profile; whether specific stories stay is open.
9. **Selection is at the subagent task,** not the request (already a vision principle).
10. **Conversation capture is likely a separate product.** Open.
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

From the owner's one-line description, the coordinator proposed: a lease table inside the router persisted as an append-only JSONL journal; an admin API (`POST /legatus/lease`, `POST /legatus/lease/{id}/release`, `GET /legatus/leases`); slots per node, one per lease or all for an `exclusive` role; immediate refusal with `no_capacity` or `no_candidate` and no queue; lifetime by release, idle TTL and maximum lifetime, observed from requests with no heartbeat; one fresh idle TTL after a restart; the pin as an implicit lease; a lease never moves between nodes in v1. Full text in [LLM pool allocation](../baseline/coordination.md). The owner will iterate on it.

## Open items

- Conversation capture: separate product or not (owner leans to separate).
- Behavioural telemetry stories: possibly paused later.
- The lease design above, including parameter values, whether a waiting queue is wanted, and the journal-loss behaviour.
- The machines entity in the registry (PROPOSED, pending with the registry owner) and what the regimes are.
- Whether any role-level tool permission profile stays in the registry; it was dropped from the docs along with the guard.

## Where the removed designs went

All in the [archive](../archive/): [coordination-full-design](../archive/coordination-full-design.md), [router-resilience-design](../archive/router-resilience-design.md), [delegation-worktrees-guard-remote-design](../archive/delegation-worktrees-guard-remote-design.md), [budget-design](../archive/budget-design.md).
