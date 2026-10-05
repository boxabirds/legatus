# Coordination service

Agents coordinate through `legatus-coord`, one MCP server that every agent, local or frontier, connects to. No existing open protocol covers this: ACP connects client to agent, A2A has a task lifecycle but no shared resources, and MCP is just tools. Building it as MCP tools keeps it portable across harnesses.

## Capabilities

| Capability | Tools | Purpose |
| --- | --- | --- |
| Leases | `claim(resource, ttl, mode)`, `renew`, `release`, `who_holds` | Exclusive or shared hold on a path glob, branch, worktree, node GPU, or `release` |
| Task board | `post_task`, `update_status`, `depends_on`, `list_tasks` | Status (queued, running, blocked, done, failed), owner, dependencies, outputs |
| Messages | `post(to, body)`, `inbox`, `ask(to, question)` | Findings, questions, handoffs between agents and to the orchestrator |

## Lease rules

- Every lease has a TTL (default `LEASE_TTL_S`) and must be renewed, so a crashed agent cannot hold a resource forever.
- Modes are `exclusive` and `shared`; shared read leases coexist, a write lease waits.
- Path leases use globs, so a tester holding `tests/**` doesn't block a tech writer on `docs/**`.
- A failed claim returns the holder, its stated reason and expected expiry, so the agent can wait, message the holder, or report blocked.

## Lease hook failure behaviour

The `pi-legatus` hook checks a lease before every edit, commit and release command. It caches its own agent's leases to stay in-process.

- **Fails closed for writes.** If coord is unreachable, or the cache is older than `HOOK_CACHE_TTL_S`, the hook blocks the write, commit or release and tells the agent why. Reads are not blocked.
- **Cache is shorter than the lease.** `HOOK_CACHE_TTL_S` is below `LEASE_TTL_S`, so a transferred or expired lease is noticed before the old holder's TTL would have run out.
- **A block is an event.** Each blocked action is logged with agent, resource and reason.

## Lease expiry and renewal

- **Automatic renewal.** The hook renews each held lease when `LEASE_RENEW_FRACTION` of its TTL remains, so an agent busy in a long model turn does not lose its lease by forgetting to call `renew`.
- **Renewal is a background timer.** It runs inside the pi process independently of model turns, so a slow turn does not starve renewal. This is an assumption about pi extensions to confirm in phase 4a; if pi cannot run a timer, `LEASE_TTL_S` must exceed the slowest candidate node's longest turn.
- **On expiry.** The hook blocks further writes to that resource until the agent claims it again. Work already done stays on the agent's own worktree and branch, so nothing is lost and nothing overwrites another agent's files. If another agent now holds the resource, the original agent gets the holder and reason, as with any failed claim.

## Overlap and deadlock

- **Glob overlap is a conflict.** `tests/**` and `tests/unit/**` overlap, so an exclusive claim on one conflicts with any claim on the other. Overlap is decided by comparing the globs, not by listing the files that exist now.
- **Multi-resource claims are all-or-nothing.** An agent that needs several resources claims them in one call; if any is unavailable, none is taken. This removes the hold-and-wait pattern behind deadlocks. A claim that cannot be satisfied returns every blocking holder.
- **Negotiation has a limit.** After `NEGOTIATION_MAX_ROUNDS` messages between two agents about one resource without a lease transfer or release, the task is posted as `blocked` and the orchestrator is notified. Two agents cannot negotiate forever.

## Negotiate in language, enforce in leases

Capable agents given a free-text channel negotiate resource contention on their own ("I'm in `auth.ts`, hold off until I've pushed"). This has been seen emerging in Claude Code agent teams, whose mailboxes carry plain messages. Legatus uses that behaviour but does not depend on it.

- **Language layer.** Agents negotiate who needs what, why, for how long, and whether work can be reordered. Rigid locks cannot express this.
- **Enforcement layer.** Leases with TTLs and an audit log, enforced by a pi hook on every edit, commit and release command, not left to the agent's discretion.
- **The join.** A failed claim returns the holder and reason, so the blocked agent can message the holder. Only the agreed outcome is recorded, as a lease transfer or release, so the log shows what was settled.

Why both: weaker local models negotiate unreliably (they may never read the inbox), spoken agreements leave no audit trail, and an agent that compacts or restarts forgets what it promised.

## Task board rules

- The dispatcher posts each delegation; agents update their own status.
- Dependencies gate work: the release manager's task depends on the tester's, and it cannot claim `release` until that task is done.
- Outputs are pointers (branch, file path, artifact link), not pasted content, to keep the frontier orchestrator's context lean.

## Storage

SQLite on the client machine is enough for one owner and a handful of agents. Every lease and status change is an append-only event, giving an audit trail of who touched what.

## Limitation

pi-subagents blocks the parent while parallel subagents run, so the orchestrator cannot react mid-flight. Agents can still coordinate with each other through this service while the orchestrator waits. An async delegation variant is an open question.
