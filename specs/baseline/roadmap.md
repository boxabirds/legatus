# Roadmap and risks

Each phase ends with an acceptance test that can fail. A phase is done when its test passes, not when its code is written. Parameter names are defined in [architecture](architecture.md#parameters).

## Phases

1. **Fleet serving.** An engine on each node (llama.cpp, MLX, vLLM as suits the machine), optionally behind llama-swap, on the local network.
   - **Accept:** every node answers an OpenAI-compatible chat request, and every model and engine pair that will join a role passes the [smoke test](registry.md#smoke-test) at `SMOKE_MIN_PASS`.
   - **Fail:** a pair below the threshold is excluded from all roles.
2. **Roles in pi.** Registry file, generated `models.json`, pi-subagents agent definitions, frontier provider with a spend cap.
   - **Accept:** from the registry alone, a pi session spawns one subagent per role, each on its first-choice node, and each subagent can use only the tools in its profile (a denied tool call is refused).
   - **Accept:** adding a node entry and its name to a role's `candidates`, with no other change, makes that role usable on the new node.
3. **Router.** `legatus-router` in Rust: OpenAI-compatible proxy with health, context and load filters, session stickiness, frontier budget and failover.
   - **Accept (failover):** with a node killed mid-session, no new request reaches it within `FAILOVER_MAX_S`, and the session continues on the next candidate.
   - **Accept (breaker):** after `BREAKER_FAILURE_THRESHOLD` consecutive failures the node's breaker opens; after `BREAKER_COOLDOWN_S` and `BREAKER_PROBE_SUCCESSES` probes it closes.
   - **Accept (stickiness):** across a scripted multi-turn session with all nodes healthy, 100% of requests land on one node; the prompt-cache hit rate on the second and later turns is recorded as the baseline.
   - **Accept (budget):** at the cap, a role with another candidate moves to it, and `architect` returns `blocked: budget_exhausted`. Overshoot is at most one request.
   - **Accept (empty candidates):** a request no candidate can serve returns the per-node reasons.
4. **Coordination.** Split so each step is verifiable alone.
   - **4a. Leases and hook.** `legatus-coord` leases plus the `pi-legatus` lease hook.
     - **First check:** confirm a pi extension can run a background renewal timer (see [coordination](coordination.md#lease-expiry-and-renewal)).
     - **Accept:** two agents racing for overlapping globs, run repeatedly, never both hold a write lease; the audit log shows zero overlapping writes.
     - **Accept:** with coord stopped mid-edit, the hook blocks the next write within `HOOK_CACHE_TTL_S` and permits reads.
     - **Accept:** an agent that stops renewing loses its lease at `LEASE_TTL_S`; a second agent can then claim it.
     - **Accept:** a two-resource crossed claim (A wants X then Y, B wants Y then X) completes for one agent without deadlock.
   - **4b. Task board and inboxes.** Tasks, dependencies, messages.
     - **Accept:** a release-manager task cannot claim `release` until its tester dependency is `done`.
     - **Accept:** two agents exchanging messages over one resource are escalated as `blocked` after `NEGOTIATION_MAX_ROUNDS`.
   - **4c. Worktree per writer.** Each writing agent gets its own worktree and branch.
     - **Accept:** two writers edit the same file in parallel without either overwriting the other; the release manager merges both branches.
   - **Accept (restart):** after killing and restarting coord, state matches the event log and no lapsed lease is extended.
5. **Remote execution.** pi-subagents fork that spawns `ssh node pi-acp`, for roles that must run where the hardware is, such as CUDA tests on the 4090.
   - **Accept:** a tester subagent runs its tests on the 4090 and returns a pointer to its result.
   - **Plan B if the fork stalls:** keep the tester on the client, give it an ssh-backed exec tool that runs only the test command on the 4090 (still least privilege: one allow-listed command), and leave model inference on its normal node. This needs no pi-subagents fork.

## Risks

- **Under-escalation.** If a local model ever orchestrates, it will call the architect too rarely. Keep the frontier as orchestrator or use hard triggers.
- **Tool-calling variance.** Quality differs by model and engine. No pair joins a role without passing the smoke test.
- **Laptop nodes.** The M5 Max may sleep; health checks and the circuit breaker must remove it within `FAILOVER_MAX_S`.
- **Harness churn.** pi and pi-subagents move fast; pin versions.
- **Phase 5 dependency.** It rests on an unshipped fork; see Plan B above.
- **Unconfirmed defaults.** The values in the parameter table are reasoned defaults, not measurements; the phase 3 and 4 acceptance tests are how they get confirmed or changed.

## Open questions

- How does the orchestrator see subagent progress while pi-subagents blocks? May need an async delegation variant.
- Can a small classifier decide frontier-versus-local escalation reliably enough to replace hard triggers?
