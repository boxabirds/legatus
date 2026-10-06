# Delegation, worktrees, guard, sandbox and remote execution: design (archived)

> **ARCHIVED DESIGN. NOT v1 SCOPE.** Owner decision 2026-10-06, recorded in the [scope reset](../decisions/2026-10-scope-reset.md).
>
> **Why it is not v1:** These are agent-harness and orchestration concerns, and Legatus no longer coordinates anything but the LLM pool. Spawning, results, worktrees, the guard, the sandbox and remote execution were all placed outside the product. Agents also lie about results, so Legatus never verifies work and the acceptance check and escalation gate are cut.
>
> **What would trigger revisiting:** A decision to make Legatus an orchestration product rather than a model proxy, or a harness that offers no confinement and where users ask for it from Legatus. The spike findings are the starting point: a committed symlink escaped a worktree, and a real 1.7B model reported done with no changes.
>
> This text is the design as it stood before the reset, kept so it can be picked up later. It is not maintained, and parameters and defaults named here are not in the live parameter table. Facts about pi, engines and the spikes are in the [decision record](../decisions/2026-10-spike-decisions.md).

## Delegate flow and selection steps that touched coordination

The orchestrator chose the role; `delegate(role, task, hints)`, a thin pi extension, asked the router for a node. The old selection steps 1, 5 and 6 are kept here (the live filter, stick and score steps are in [dispatcher](../baseline/dispatcher.md)).

1. **Resolve role.** Look up the role's candidates, permissions and leases. If the role is `escalation_only`, require a stated trigger in `hints`.
5. **Acquire leases.** Claim any `exclusive` resource and the worktree through `legatus-coord`; on failure, queue and report back.
6. **Spawn.** Launch the subagent via pi-subagents with the chosen model and permission profile, and record the run on the task board.

## Escalation gate and classifier note

It adds latency to every call, its mistakes are hard to trace, and the role already captures the nature of the task. Context length, tool needs and load are filters, not judgments. The one judgment worth automating is frontier-versus-local escalation; a small classifier (the technique behind vLLM Semantic Router) is a candidate for that, borrowed as a technique rather than a dependency.

### Frontier versus local orchestration

- **Frontier orchestrator (default):** better judgment about when to escalate; the main session carries growing context but each turn is short because reading and editing happen locally.
- **Local orchestrator:** cheaper, but a weaker model under-escalates. If used, rely on hard triggers: architecture keywords, repeated failure, explicit reviewer flags.

`escalation_only` in the registry was a gate: the orchestrator could call the role only on stated triggers (architecture, repeated failure, reviewer flag). Risk noted then: a local orchestrator under-escalates.

## Acceptance check and result pointers

- The dispatcher posts each delegation; agents update their own status.
- Dependencies gate work: the release manager's task depends on the tester's, and it cannot claim `release` until that task is done.
- Outputs are pointers (branch, file path, artifact link), not pasted content, to keep the frontier orchestrator's context lean.
- **Done is not "process exited 0".** In the spike a real 1.7B model was delegated correctly, the child never created the file, and the board reported `done` with no changes. Completion needs an acceptance check that the work changed something (or passed its gate), and completion is idempotent.

## Limitation of foreground mode and async delegate

pi-subagents' foreground mode blocks the parent while subagents run, so the orchestrator cannot react mid-flight. Async mode (separate child processes, completions arriving as new user messages, results as pointers) lets the orchestrator keep working; `delegate` uses it. A naive orchestrator can loop on those completion messages, which `delegate` has to guard against. Agents can also coordinate through this service while the orchestrator waits.

## Process model

"One process per role" does not hold for every mode of pi-subagents (0.76.0):

- **Foreground subagents** run in the parent's process and do not load the parent's extensions unless the agent lists them in `extensions:`.
- **Async and workflow children** are separate processes and load the user-level guard.
- There are no per-run environment variables, only a working directory and a small bindings channel, so per-child credentials need that channel or separate launches.

`delegate` uses async children so the orchestrator is not blocked.

## Isolation (worktree per writer)

Every writing agent gets its own git worktree and branch, enforced as confinement by the guard (a write is allowed only inside the agent's worktree); the release manager merges. Leases cover only what cannot be isolated or merged: shared named resources (a GPU, a test environment) and the release itself.

## Defence in depth (guard, registration, watcher, sandbox)

The guard cannot stop a child that was launched without it. In the spikes a textual `-ne` check was evaded and a wrong extension filename left pi running unguarded with exit code 0. Four layers cover each other:

1. **Guard.** A pre-tool-call hook that blocks writes outside the worktree and checks leases; it fails closed and every coord call has its own deadline.
2. **Registration check.** The guard registers with coord at startup, with a self-check that fails loudly; `delegate` verifies registration.
3. **Watcher.** A dispatcher-side process kills a child that never registers (1.2 s in the spike).
4. **Sandbox.** A per-worktree `sandbox-exec` profile on macOS denies writes outside the worktree at the OS level (20 to 40 ms per launch in the spike). It also lets the guard allow commands it cannot classify (`npm test`, `make`). Linux sandboxing is untested.

pi-subagents has no option to wrap a child's command; the spike used a PATH trick that relies on an undocumented fallback.

## Least privilege beyond credential custody

The network is assumed to be a trusted local one; Legatus does not own network-level security. Within that, least privilege is enforced at the agent and resource level:

- Each role's `tools` profile is an allow-list; unlisted tools are denied.
- Leases cover the narrowest resource that works (a named resource, not the repo).
- Hosted nodes follow the same rule: the router holds their credentials (this bullet survives in v1; the budget cap does not).
- The frontier API key is held by the router only. Agents never see it.
- A subagent receives only the leases and credentials its task needs, for the task's lifetime.

Only the credential custody bullets survive in v1, in the router.

## Guard failure rows

| Failure | Effect | Behaviour |
| --- | --- | --- |
| Coord call hangs | The harness gives a hook no timeout; one hung call stalled pi for over 70 s | Every guard call has its own deadline and fails closed. |
| Guard fails to load or throws at startup | Harness runs unguarded and exits 0 | Startup self-check, registration check and watcher; sandbox as the last layer. |
| Child launched without the guard | No hook runs | Watcher kills the unregistered child; sandbox denies writes outside the worktree. |
| Child reports done without changes | Board says `done`, nothing happened (seen with a 1.7B model) | Completion requires an acceptance check, not process exit 0. |

## Roadmap phases: worktree per writer, remote execution, adapter acceptance

   - **4c. Worktree per writer.** Each writing agent gets its own worktree and branch; the guard confines writes to it.
     - **Accept:** two writers edit the same file in parallel in their own worktrees without either overwriting the other, an absolute path, `../`, a symlink or `git -C` into another worktree is blocked, and the release manager merges both branches. A merge conflict returns `blocked: merge_conflict` with paths and branches intact.


5. **Remote execution.** For roles that must run where the hardware is, such as CUDA tests on the 4090. Plan B first: it needs no fork, and `pi-acp` (0.0.34) is an editor adapter with no ssh.
   - **Plan B (first):** keep the tester on the client, give it an ssh-backed exec tool that runs only the test command on the 4090 (still least privilege: one allow-listed command), and leave model inference on its normal node.
   - **Accept:** a tester subagent runs its tests on the 4090 through the ssh exec tool and returns a pointer to its result. Real-process test: real pi, real ssh to the 4090, real test command.
   - **Plan A (only if Plan B proves insufficient):** a pi-subagents fork that spawns `ssh node pi-acp`. Unshipped.

Adapter acceptance (old): a real harness process is blocked from writing outside its worktree in a subagent, an unguarded launch is detected, and a session stays pinned to one node.

## Boundary rows

| Boundary | Standard | Status |
| --- | --- | --- |
| In-process extension to coordination | Unix socket, JSON lines | Proposed default (P-1) |
| Agents to coordination | MCP over loopback streamable HTTP | Proposed default (P-1) |
| Orchestrator to remote subagent | ACP over ssh (`pi-acp`), or an ssh exec tool | `pi-acp` is an editor adapter with no ssh; plan B is the ssh exec tool |
