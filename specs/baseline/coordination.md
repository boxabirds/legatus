# Coordination service

Agents coordinate through `legatus-coord`, one service every agent, local or frontier, connects to. No existing open protocol covers this: ACP connects client to agent, A2A has a task lifecycle but no shared resources, and MCP is just tools. Exposing it as MCP tools keeps it portable across harnesses. Coord is harness-neutral; the enforcement in front of it (the guard) is a per-harness adapter. See [architecture](architecture.md#harness-neutral-boundary).

## Capabilities

| Capability | Tools | Purpose |
| --- | --- | --- |
| Leases | `claim(resource, ttl, mode)`, `claim_many`, `renew`, `release`, `who_holds`, `transfer` | Exclusive or shared hold on a named resource: `release`, or a shared resource such as `gpu:rtx4090` or a test environment |
| Task board | `post_task`, `update_status`, `depends_on`, `list_tasks` | Status (queued, running, blocked, done, failed), owner, dependencies, outputs |
| Messages | `post(to, body)`, `inbox`, `ask(to, question)` | Findings, questions, handoffs between agents and to the orchestrator |

## Lease scope

Isolate first; lock only what cannot be isolated or merged.

- **Worktree confinement is not a lease.** Each writer gets its own git worktree and branch, and the guard allows a write only inside the agent's own worktree. That needs no coord call per edit.
- **`release` is a lease.** Only one agent may release at a time; the release manager claims it, and its task depends on the tester's. The releaser merges through a structurally constrained guard.
- **Generic named resources are leases.** A shared GPU (`gpu:rtx4090`), a shared test environment, or any name a role lists in `exclusive`. `delegate` claims these before it spawns the child. A failed claim returns `blocked` with the holder.
- **Branch leases are dropped.** Branches are per writer, so there is nothing to contend for.
- **Path-glob leases are deferred** to an optional later phase, for the case where two writers genuinely need the same files in one worktree. Until then the glob-overlap rules below are not needed.
- **`tester` is a writer** (proposed default P-2): it gets its own worktree like any other writer.

## Lease rules

- Every lease has a TTL (default `LEASE_TTL_S`) and must be renewed, so a crashed agent cannot hold a resource forever.
- Modes are `exclusive` and `shared`; shared leases coexist, an exclusive one waits.
- A claim by the same holder on a resource and mode it already holds renews it (idempotent).
- A failed claim returns the holder, its stated reason and expected expiry, so the agent can wait, message the holder, or report blocked.
- An expired lease cannot be renewed; the holder must claim again.

## Two clocks

A lease is live only if the wall clock and a monotonic clock both say it is. A backward wall-clock jump extended leases when expiry used the wall clock alone. The spike showed this by simulating the jump in code, not by changing the OS clock, so laptop sleep and real clock changes are still untested. Expiry never extends a lease: any disagreement resolves toward expired.

## Guard failure behaviour

The guard is a per-harness extension that checks every write, commit and release command before it runs. Its only coord traffic is lease checks for named resources, plus its own registration and heartbeat. It caches its own agent's positive decisions to stay in-process.

- **Fails closed for writes.** If coord is unreachable, or the cache is older than `HOOK_CACHE_TTL_S`, the guard blocks the write, commit or release and tells the agent why. Reads are not blocked. A throwing hook also fails closed.
- **Own deadline on every coord call.** The harness gives a hook no timeout: in the spike a hung call stalled pi for more than 70 s. The guard therefore races every coord call against `COORD_CALL_DEADLINE_S` and fails closed on expiry.
- **Cache is shorter than the lease.** `HOOK_CACHE_TTL_S` is below `LEASE_TTL_S`, and a cached decision is capped at the lease's expiry minus a margin, so a transferred or expired lease is noticed before the old holder's TTL would have run out.
- **A block is an event.** Each blocked action is logged with agent, resource and reason.

## Guard registration and startup self-check

A guard that fails to load does not stop the harness: a wrong extension filename or a throw in `session_start` left pi running unguarded with exit code 0.

- **Startup self-check.** On start the guard pings coord and registers itself. If either fails it prints a loud banner and refuses writes.
- **Registration check.** `delegate` and the dispatcher-side watcher confirm that each child has registered (`guard_registered`) before and while it runs. A child that never registers is killed (the spike's watcher did this in 1.2 s).
- **Foreground subagents** run in the parent's process and load an extension only if the agent lists it in `extensions:`; every agent definition must list the guard (proposed default P-4).
- **A textual check for `-ne` is not enough.** The guard cannot stop a child launched without it; see defence in depth in [architecture](architecture.md#defence-in-depth).

## Transport

- **Unix socket** for in-process extensions (the guard and `delegate`), one persistent connection per agent. In the spike a handler held one connection across calls.
- **Loopback streamable-HTTP MCP** for MCP clients, because pi's MCP supports only stdio and streamable HTTP, not a Unix socket or legacy SSE. Per-call overhead was about 0.4 ms over the socket and 1 ms over HTTP.
- Both are a **proposed default (P-1)**. If coord is down when the harness starts, its MCP tools are silently absent and never appear later, so the harness must be started after coord and the self-check must confirm the tools are present. MCP tools are declared to the model only with `exposure: "direct"` in pi 1.0.3.

Unix socket paths on macOS are limited to 104 bytes, so the socket lives in a short directory.

## Lease expiry and renewal

- **Automatic renewal.** The guard renews each held lease when `LEASE_RENEW_FRACTION` of its TTL remains, so an agent busy in a long model turn does not lose its lease by forgetting to call `renew`.
- **Renewal is a background timer.** pi's `setInterval` fired through a 10 s model wait and a 5 s tool run, so the timer works, but a synchronous busy loop starves it. Timers and sockets must be `unref()`d or `pi -p` never exits. Confirm in phase 4a on each harness.
- **On expiry.** The guard blocks further writes to that resource until the agent claims it again. Work already done stays on the agent's own worktree and branch, so nothing is lost and nothing overwrites another agent's files. If another agent now holds the resource, the original agent gets the holder and reason, as with any failed claim.

## Deadlock

- **Multi-resource claims are all-or-nothing.** An agent that needs several resources claims them in one call (`claim_many`); if any is unavailable, none is taken. This removes the hold-and-wait pattern behind deadlocks. A claim that cannot be satisfied returns every blocking holder.
- **Negotiation has a limit.** After `NEGOTIATION_MAX_ROUNDS` messages between two agents about one resource without a lease transfer or release, the task is posted as `blocked` and the orchestrator is notified. Two agents cannot negotiate forever.
- **Deferred with path globs.** If path-glob leases are added later, overlapping globs (`tests/**` and `tests/unit/**`) conflict, decided by comparing the globs rather than listing files.

## Negotiate in language, enforce in leases

Capable agents given a free-text channel negotiate resource contention on their own ("I'm using the 4090, hold off until I've finished"). This has been seen emerging in Claude Code agent teams, whose mailboxes carry plain messages. Legatus uses that behaviour but does not depend on it.

- **Language layer.** Agents negotiate who needs what, why, for how long, and whether work can be reordered. Rigid locks cannot express this.
- **Enforcement layer.** Leases with TTLs and an audit log, enforced by the guard on every write, commit and release command, not left to the agent's discretion.
- **The join.** A failed claim returns the holder and reason, so the blocked agent can message the holder. Only the agreed outcome is recorded, as a lease transfer or release, so the log shows what was settled.

Why both: weaker local models negotiate unreliably (they may never read the inbox), spoken agreements leave no audit trail, and an agent that compacts or restarts forgets what it promised.

## Task board rules

- The dispatcher posts each delegation; agents update their own status.
- Dependencies gate work: the release manager's task depends on the tester's, and it cannot claim `release` until that task is done.
- Outputs are pointers (branch, file path, artifact link), not pasted content, to keep the frontier orchestrator's context lean.
- **Done is not "process exited 0".** In the spike a real 1.7B model was delegated correctly, the child never created the file, and the board reported `done` with no changes. Completion needs an acceptance check that the work changed something (or passed its gate), and completion is idempotent.

## Storage and event log

SQLite on the client machine is enough for one owner and a handful of agents. Every lease and status change is an append-only event, giving an audit trail of who touched what.

- **Append-only.** The event log is the source of truth; the derived lease table is rebuilt from it.
- **Replay on start.** After a crash, state is rebuilt by replaying the log. Leases whose term ended while coord was down are expired, never extended, and survivors are clamped to at most one TTL of remaining time.
- **Idempotent claims.** Repeating a claim by the same holder renews rather than duplicates.
- **Verifiable.** An independent replay of the log can check that no two holders of overlapping exclusive leases were ever live together. The spike ran 8 race rounds with scripted models and found none.

## Limitation

pi-subagents' foreground mode blocks the parent while subagents run, so the orchestrator cannot react mid-flight. Async mode (separate child processes, completions arriving as new user messages, results as pointers) lets the orchestrator keep working; `delegate` uses it. A naive orchestrator can loop on those completion messages, which `delegate` has to guard against. Agents can also coordinate through this service while the orchestrator waits.
