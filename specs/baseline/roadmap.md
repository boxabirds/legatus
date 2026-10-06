# Roadmap and risks

Each phase ends with an acceptance test that can fail. A phase is done when its test passes, not when its code is written. Parameter names are defined in [architecture](architecture.md#parameters). Every acceptance test is a real-process test: it runs the real router, node agent, harness and engine processes, with scripted fakes only where a real failure cannot be produced on demand, and the engine and harness versions are recorded beside the result.

Scope was reduced on 2026-10-06; see the [scope reset](../decisions/2026-10-scope-reset.md). The phases for coordination, worktrees, delegation, guard and remote execution were removed and their designs are in the [archive](../archive/).

## Spikes completed

October 2026 spikes proved the basic feasibility of the pi baseline against scripted fakes and small real models (Qwen3 1.7B on an Apple M2); they did not test tool-calling quality, CUDA, Vulkan or larger models. Results, approved decisions D-1 to D-5, proposed defaults P-1 to P-5 and the list of what was not tested are in the [decision record](../decisions/2026-10-spike-decisions.md). Parts of that record about guard, coordination and delegation are now archived scope.

## v1 phases

v1 is: registry, router, lease allocation, node agents, capability and behavioural profile, power and thermal readings, config generation and the pi adapter.

1. **Fleet serving.** An engine on each node (llama.cpp, MLX, vLLM as suits the machine), optionally behind llama-swap, on the local network.
   - **Accept:** every node answers an OpenAI-compatible chat request, and every model and engine pair that will join a role passes the [smoke test](registry.md#smoke-test) at `SMOKE_MIN_PASS`. Real-process test: the smoke suite run against each real engine at its pinned version.
   - **Fail:** a pair below the threshold is excluded from all roles.
2. **Registry and config generation.** Registry file, generated pi `models.json` (role names as models, session-affinity headers on, lean profile), the router's routing table. Target pi 1.0.3.
   - **Accept:** from the registry alone, a real pi session runs against the generated config through the router, and its requests land on the role's first-choice node. Real-process test: real pi processes against the generated config.
   - **Accept:** adding a node entry and its name to a role's `candidates`, with no other change, makes that role usable on the new node. Real-process test: edit the registry, regenerate, and run a real pi session on the new node.
3. **Router.** `legatus-router` in Rust: proxy for both protocols (one per role) with capability, endpoint-kind, health, context and load filters, session pin that persists, silent-truncation guard, request patches, decision log, and a recorded outcome for every request, through one upstream call site.
   - **Accept (stickiness):** across a scripted multi-turn session with all nodes healthy, 100% of requests land on one node, including after a router restart; the prompt-cache hit rate on the second and later turns is recorded as the baseline. Real-process test: real pi session through the real router, with the router restarted between turns.
   - **Accept (truncation):** a prompt larger than a silent-truncating node's effective context is rejected, never answered from a cut prompt. Real-process test: real router with a real Ollama at its pinned version.
   - **Accept (capability filter):** a role whose `requires` no node meets returns the per-node reasons; a role that mixes protocols or endpoint kinds is rejected at load. Real-process test: real router started with each bad config, and a real request for each unmet role.
   - **Accept (empty candidates):** a request no candidate can serve returns the per-node reasons. Real-process test: real router with every node stopped, one at a time for each reason.
   - **Accept (outcomes):** after real successes, a killed node and an invalid request, the recorded outcome classes match what happened and no behaviour changed because of them. Real-process test: real router and real engine processes.
   - **Accept (credential custody):** a hosted node's key reaches the node and appears in no agent-visible output and no router log. Real-process test: real router with a scripted hosted node that records the headers it received.
4. **LLM pool allocation.** The lease table, API, capacity, lifetime and journal in [LLM pool allocation](coordination.md). Acceptance tests are listed there: grant and release, capacity on a serial engine, refusal reasons, idle and maximum lifetime, restart, implicit pin.
5. **Node agents, profiles, power and thermal.** `legatus-node` on each machine reports health, load and machine readings. The calibration probe writes the measured layer on join and on version, model or flag change. Behavioural profile per node from observed requests (consent switch off by default; operational metadata always collected).
   - **Accept:** a node that fails its capability canary is excluded from that role; a node whose measured effective context is below declared uses the measured value; a role mixing endpoint kinds is rejected at load. Real-process test: real probe and real canaries against each real engine.
   - **Accept:** `legatus-node` on a real machine reports power and thermal readings that change when the machine is loaded, and the router records them. Real-process test: real node agent on real hardware under a real engine load.
   - **Non-chat endpoint kinds.** Embeddings, transcription, speech, images, rerank and classify, each with a canary. `decide` with a hosted node under the router's credential custody. Apple on-device node (optional) behind a Swift OpenAI-compatible shim.
6. **pi adapter.** `pi-legatus`: the generated config plus the acquire and release call surface.
   - **Accept:** a real pi 1.0.3 process acquires a lease by semantic needs, runs on the leased node, and releases; a refused acquire is shown to the agent with the reason. Real-process test: real pi, real router, real engine.

## v1.x and v2

- **Router resilience.** Timeouts, retries, circuit breakers, failover and health demotion, built at the two seams. Design in [router-resilience-design](../archive/router-resilience-design.md). Its acceptance tests (failover within a time bound, breaker open and close, timeout recovery) come back with it.
- **Additional harness adapters.** Each provides the generated config and the acquire and release surface. Claude Code speaks only Anthropic Messages and sends its own session id headers (the spike completed a tool loop through the router on llama-server; interactive use and `count_tokens` are untested). opencode is not yet run. DeepSeek Harness is a developer preview (`dsh` 0.2.0-rc.2) that sends no session header on its chat and Anthropic routes. Per-adapter acceptance: a real harness binary at its pinned version acquires, uses and releases a lease, and a session stays pinned to one node.
- **Telemetry drift and backtest.** Compare observed behaviour with the measured profile over time.
- **Conversation capture.** Likely a separate product; open (see the [scope reset](../decisions/2026-10-scope-reset.md#open-items)).

## Risks

- **Tool-calling variance.** Quality differs by model and engine. No pair joins a role without passing the smoke test.
- **Laptop nodes.** The M5 Max may sleep; with no failover in v1 a request to a sleeping leased node returns an error to the agent.
- **Hung nodes.** With resilience deferred, a hung node holds the harness for its own request timeout (300 s for pi in the spike).
- **Harness churn.** pi moves fast; pin versions and keep the spike harnesses as a compatibility check. Ollama auto-updates, so freeze it in tests.
- **Untested hardware.** The spikes ran on one Apple M2 with a 1.7B model. CUDA, Vulkan, Strix Halo and larger models are open.
- **Unconfirmed defaults.** The values in the parameter table are reasoned defaults, not measurements; the lease parameters have no value yet. The phase 3 to 5 acceptance tests are how they get confirmed.
- **Proposed lease design.** The allocation design is the coordinator's proposal from one line of owner description; expect it to change.

## Answered

- **Progress while pi-subagents blocks.** No longer a Legatus concern: spawning and result handling are the harness's, and Legatus sees only requests and lease calls.
