# Roadmap and risks

Each phase ends with an acceptance test that can fail. A phase is done when its test passes, not when its code is written. Parameter names are defined in [architecture](architecture.md#parameters).

## Spikes completed

October 2026 spikes proved the basic feasibility of the pi baseline against scripted fakes and small real models (Qwen3 1.7B on an Apple M2); they did not test tool-calling quality, CUDA, Vulkan or larger models. Results, approved decisions D-1 to D-5, proposed defaults P-1 to P-5 and the list of what was not tested are in the [decision record](../decisions/2026-10-spike-decisions.md). The phases below build on them. Each acceptance test names a real-process test: it runs the real router, coord, harness and engine processes, with scripted fakes only where a real failure cannot be produced on demand, and the engine and harness versions are recorded beside the result.

## Phases

1. **Fleet serving.** An engine on each node (llama.cpp, MLX, vLLM as suits the machine), optionally behind llama-swap, on the local network.
   - **Accept:** every node answers an OpenAI-compatible chat request, and every model and engine pair that will join a role passes the [smoke test](registry.md#smoke-test) at `SMOKE_MIN_PASS`. Real-process test: the smoke suite run against each real engine at its pinned version.
   - **Fail:** a pair below the threshold is excluded from all roles.
2. **Roles in pi.** Registry file, generated `models.json` (with session-affinity headers on), pi-subagents agent definitions that list the guard in `extensions:`, frontier provider with a spend cap. Target pi 1.0.3 and pi-subagents 0.76.0.
   - **Accept:** from the registry alone, a pi session spawns one subagent per role, each on its first-choice node, and each subagent can use only the tools in its profile (a denied tool call is refused). Real-process test: real pi processes against the generated config.
   - **Accept:** adding a node entry and its name to a role's `candidates`, with no other change, makes that role usable on the new node. Real-process test: edit the registry, regenerate, and spawn a real pi session on the new node.
3. **Router.** `legatus-router` in Rust: proxy for both protocols (one per role) with capability, health, context and load filters, mandatory timeouts, session stickiness that persists, budget and failover.
   - **Accept (failover):** with a node killed mid-session, no new request reaches it within `FAILOVER_MAX_S`, and the session continues on the next candidate. A session retry after a mid-stream failure avoids the node that failed. Real-process test: real router and pi with a real engine process killed with SIGKILL.
   - **Accept (breaker):** after `BREAKER_FAILURE_THRESHOLD` consecutive failures the node's breaker opens; after `BREAKER_COOLDOWN_S` and `BREAKER_PROBE_SUCCESSES` probes it closes. Real-process test: real router with a real engine process that is stopped and restarted.
   - **Accept (stickiness):** across a scripted multi-turn session with all nodes healthy, 100% of requests land on one node, including after a router restart; the prompt-cache hit rate on the second and later turns is recorded as the baseline. Real-process test: real pi session through the real router, with the router restarted between turns.
   - **Accept (timeouts):** a node that hangs before its first byte is abandoned at the first-byte timeout, and the session continues, well inside the harness's own request timeout. Real-process test: real pi through the real router to a node process that accepts and then stalls.
   - **Accept (truncation):** a prompt larger than a silent-truncating node's effective context is rejected or rerouted, never answered from a cut prompt. Real-process test: real router with a real Ollama at its pinned version.
   - **Accept (capability filter):** a role whose `requires` no node meets returns the per-node reasons; a role that mixes protocols or endpoint kinds is rejected at load. Real-process test: real router started with each bad config, and a real request for each unmet role.
   - **Accept (budget):** at the cap, a role with another candidate moves to it, and `architect` returns `blocked: budget_exhausted`. Overshoot is at most one request. The same holds for a hosted non-frontier node. Real-process test: real router with a scripted hosted node that reports usage.
   - **Accept (empty candidates):** a request no candidate can serve returns the per-node reasons. Real-process test: real router with every node stopped, one at a time for each reason.
4. **Coordination.** Split so each step is verifiable alone.
   - **4a. Leases and guard.** `legatus-coord` leases on named resources plus `release`, and the `pi-legatus` guard.
     - **First check:** the spike confirmed a pi extension can run a background timer; re-confirm it on the pinned pi version (see [coordination](coordination.md#lease-expiry-and-renewal)).
     - **Accept:** two real pi processes racing for one named resource, run repeatedly, never both hold it; the event log replay shows zero overlapping holds.
     - **Accept:** with coord stopped mid-edit, the guard blocks the next write within `HOOK_CACHE_TTL_S` and permits reads; a hung coord call is cut off at `COORD_CALL_DEADLINE_S`. Real-process test: real coord process stopped with SIGSTOP and SIGKILL.
     - **Accept:** an agent that stops renewing loses its lease at `LEASE_TTL_S`; a second agent can then claim it. Real-process test: real coord and two real harness processes.
     - **Accept:** a two-resource crossed claim (A wants X then Y, B wants Y then X) completes for one agent without deadlock. Real-process test: two real harness processes against real coord.
     - **Accept (unguarded child):** a child launched without the guard is killed by the watcher, and the sandbox denies its write. Real-process test: a real pi launched with the guard disabled.
     - **Accept (self-check):** a guard with a wrong filename or a throwing `session_start` is reported, not run silently. Real-process test: real pi with each broken guard.
   - **4b. Task board and inboxes.** Tasks, dependencies, messages.
     - **Accept:** a release-manager task cannot claim `release` until its tester dependency is `done`, and `done` requires the acceptance check. Real-process test: real coord, real board, two real harness processes.
     - **Accept:** two agents exchanging messages over one resource are escalated as `blocked` after `NEGOTIATION_MAX_ROUNDS`. Real-process test: two real harness processes messaging through real coord.
   - **4c. Worktree per writer.** Each writing agent gets its own worktree and branch; the guard confines writes to it.
     - **Accept:** two writers edit the same file in parallel in their own worktrees without either overwriting the other, an absolute path, `../`, a symlink or `git -C` into another worktree is blocked, and the release manager merges both branches. A merge conflict returns `blocked: merge_conflict` with paths and branches intact.
   - **4d. Path-glob leases (optional, later).** Only if two writers must share files in one worktree; see [coordination](coordination.md#lease-scope).
   - **Accept (restart):** after killing and restarting coord, state matches the event log and no lapsed lease is extended. Real-process test: real coord killed with SIGKILL and restarted.
   - **Accept (clocks):** a simulated backward wall-clock jump does not extend a lease. A real OS clock change and laptop sleep are untested and stay open. Real-process test: real coord with a clock offset hook; the real clock is not changed.
5. **Remote execution.** For roles that must run where the hardware is, such as CUDA tests on the 4090. Plan B first: it needs no fork, and `pi-acp` (0.0.34) is an editor adapter with no ssh.
   - **Plan B (first):** keep the tester on the client, give it an ssh-backed exec tool that runs only the test command on the 4090 (still least privilege: one allow-listed command), and leave model inference on its normal node.
   - **Accept:** a tester subagent runs its tests on the 4090 through the ssh exec tool and returns a pointer to its result. Real-process test: real pi, real ssh to the 4090, real test command.
   - **Plan A (only if Plan B proves insufficient):** a pi-subagents fork that spawns `ssh node pi-acp`. Unshipped.
6. **Harness adapters.** After the pi baseline, one adapter per harness, each providing a guard, `delegate` and a generator. Router and coord stay unchanged.
   - **Claude Code.** Speaks only Anthropic Messages and sends its own session id headers. The spike completed a tool loop through the router on llama-server; interactive use and `count_tokens` are untested.
   - **opencode.** Not yet run; its block hook may not fire inside subagent sessions, which must be checked first.
   - **DeepSeek Harness.** A developer preview in the spike (`dsh` 0.2.0-rc.2). A guard plugin that fails to load fails open, and children share one process.
   - **Accept (per adapter):** a real harness process is blocked from writing outside its worktree in a subagent, an unguarded launch is detected, and a session stays pinned to one node. Real-process test: each real harness binary at its pinned version.
7. **Capabilities.** The capability registry and non-chat roles.
   - **Calibration probe.** Promote the spike probe to write the measured layer on join and on version, model or flag change.
   - **Non-chat endpoint kinds.** Embeddings, transcription, speech, images, rerank and classify, each with a canary.
   - **`decide`.** An endpoint kind for typed questions in and typed answers out, with a hosted node under the frontier's custody and budget rules, and an open-weight equivalent when one has an HTTP server.
   - **Apple on-device node (optional).** Behind a Swift OpenAI-compatible shim, for short summaries, tagging and classification.
   - **Accept:** a node that fails its capability canary is excluded from that role; a node whose measured effective context is below declared uses the measured value; a role mixing endpoint kinds is rejected at load. Real-process test: real probe and real canaries against each real engine.

## Risks

- **Under-escalation.** If a local model ever orchestrates, it will call the architect too rarely. Keep the frontier as orchestrator or use hard triggers.
- **Tool-calling variance.** Quality differs by model and engine. No pair joins a role without passing the smoke test.
- **Laptop nodes.** The M5 Max may sleep; health checks and the circuit breaker must remove it within `FAILOVER_MAX_S`.
- **Harness churn.** pi and pi-subagents move fast; pin versions and keep the spike harnesses as a compatibility check. Ollama auto-updates, so freeze it in tests.
- **Phase 5 dependency.** Plan A rests on an unshipped fork; Plan B avoids it.
- **Untested hardware.** The spikes ran on one Apple M2 with a 1.7B model. CUDA, Vulkan, Strix Halo and larger models are open.
- **Unconfirmed defaults.** The values in the parameter table are reasoned defaults, not measurements; the phase 3 and 4 acceptance tests are how they get confirmed or changed.

## Open questions

- Can a small classifier decide frontier-versus-local escalation reliably enough to replace hard triggers?

## Answered

- **Progress while pi-subagents blocks.** Use async mode: separate child processes, completions arriving as new user messages, results as pointers. Foreground mode blocks the parent. See [coordination](coordination.md#limitation).
