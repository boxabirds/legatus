# Roadmap and risks

Each phase ends with an acceptance test that can fail. A phase is done when its test passes, not when its code is written. Parameter names are defined in [architecture](architecture.md#parameters). Every acceptance test is a real-process test: it runs the real router, node agent, harness and engine processes, with scripted fakes only where a real failure cannot be produced on demand, and the engine and harness versions are recorded beside the result.

Scope was reduced on 2026-10-06; see the [scope reset](../decisions/2026-10-scope-reset.md). The phases for coordination, worktrees, delegation, guard and remote execution were removed and their designs are in the [archive](../archive/).

## Spikes completed

October 2026 spikes proved the basic feasibility of the pi baseline against scripted fakes and small real models (Qwen3 1.7B on an Apple M2); they did not test tool-calling quality, CUDA, Vulkan or larger models. Results, approved decisions D-1 to D-5, proposed defaults P-1 to P-5 and the list of what was not tested are in the [decision record](../decisions/2026-10-spike-decisions.md). Parts of that record about guard, coordination and delegation are now archived scope.

## Epics and build order

The order is stored in Ceetrix as story priorities and a "Build order" line in each epic description; this page is the summary. v1 epics: registry and config, router, LLM pool allocation (leases), node agent and profiles, harness adapters, simulated cluster tests, security (credential custody), admin surface, verification and workspace. Outside v1: router resilience (paused, v2+), advanced telemetry (separate low-priority epic, paused), conversation capture (separate product, paused except the router event log and its stream into the behavioural record).

**Recommended first vertical slice**, in order: workspace; stubs and the virtual-time spike; version pins; parameters; registry; smoke verdicts; config generation; routing table; cluster stubs; router passthrough with a real pi 1.0.3 session. Then leases with the preflight. Then the pi adapter, whose header-hook spike comes first because the lease design leans on it.

## v1 phases

v1 is: registry, router, lease allocation with preflight, node agents, capability and behavioural profile, power and thermal readings, config generation, the pi adapter, the admin surface and the simulated cluster tests.

1. **Fleet serving.** An engine on each node (llama.cpp, MLX, vLLM as suits the machine), optionally behind llama-swap, on the local network.
   - **Accept:** every node answers an OpenAI-compatible chat request, and every model and engine pair that will join a role passes the [smoke test](registry.md#smoke-test) at `SMOKE_MIN_PASS`. Real-process test: the smoke suite run against each real engine at its pinned version.
   - **Fail:** a pair below the threshold is excluded from all roles.
2. **Registry, config generation and router (the first slice).** Registry file, generated pi `models.json` (role names as models, session-affinity headers on, lean profile), the routing table, and `legatus-router` in Rust: proxy for both protocols (one per role) with capability, endpoint-kind, health, context and load filters, session pin that persists, silent-truncation guard (the guard may return 400; detection only logs), request patches, decision log, metadata-only logging, and a recorded outcome for every request, through one upstream call site. Target pi 1.0.3.
   - **Accept (registry to pi):** from the registry alone, a real pi session runs against the generated config through the router, and its requests land on the role's first-choice node. Real-process test: real pi processes against the generated config.
   - **Accept (new node):** adding a node entry and its name to a role's `candidates`, with no other change, makes that role usable on the new node. Real-process test: edit the registry, regenerate, and run a real pi session on the new node.
   - **Accept (stickiness):** across a scripted multi-turn session with all nodes healthy, 100% of requests land on one node, including after a router restart; the prompt-cache hit rate on the second and later turns is recorded as the baseline. Real-process test: real pi session through the real router, with the router restarted between turns.
   - **Accept (truncation):** a prompt larger than a silent-truncating node's effective context is rejected, never answered from a cut prompt. Real-process test: real router with a real Ollama at its pinned version.
   - **Accept (capability filter):** a role whose `requires` no node meets returns the per-node reasons; a role that mixes protocols or endpoint kinds is rejected at load. Real-process test: real router started with each bad config, and a real request for each unmet role.
   - **Accept (empty candidates):** a request no candidate can serve returns the per-node reasons. Real-process test: real router with every node stopped, one at a time for each reason.
   - **Accept (outcomes):** after real successes, a killed node and an invalid request, the recorded outcome classes match what happened and no behaviour changed because of them. Real-process test: real router and real engine processes.
   - **Accept (credential custody):** a hosted node's key reaches the node and appears in no agent-visible output and no router log, and no `x-legatus-*` header reaches it. Real-process test: real router with a scripted hosted node that records the headers it received.
   - **Accept (metadata only):** after real traffic carrying a planted marker string in the prompt and the completion, no router log contains the marker. Real-process test: real router and engine.
3. **LLM pool allocation with preflight.** The lease table, API, capacity, lifetime, holder rules, preflight and pool view, journal and restart behaviour in [LLM pool allocation](coordination.md). Acceptance tests are listed there: grant and release, holder rule, hidden holder values, capacity and preflight on a serial engine, refusal reasons and codes, idle and maximum lifetime, restart and crash, implicit pin. Acquire is explicit: the main agent calls it.
4. **pi adapter.** `pi-legatus`: the generated config plus the `can_spawn`, `request_llm` and `release_llm` tools, built against the contract in `packages/adapter-contract`. **The first task is the header-hook spike:** can a pi 1.0.3 extension set the lease header on pi's chat path after acquire. UNPROVEN until then.
   - **Accept:** a real pi 1.0.3 process runs the preflight, acquires a lease by semantic needs, runs on the leased node, and releases; a refused acquire is shown to the agent with the reason. Real-process test: real pi, real router, real engine.
5. **Node agents, profiles, power and thermal.** `legatus-node` on each machine reports health, load and machine readings. The calibration probe writes the measured layer on join and on version, model or flag change. Behavioural profile per node from observed requests, recorded and reported only (consent switch off by default; operational metadata always collected).
   - **Accept:** a node that fails its capability canary is excluded from that role; a node whose measured effective context is below declared uses the measured value; a role mixing endpoint kinds is rejected at load. Real-process test: real probe and real canaries against each real engine.
   - **Accept:** `legatus-node` on a real machine reports power and thermal readings that change when the machine is loaded, and the router records them. Real-process test: real node agent on real hardware under a real engine load.
   - **Telemetry records and reports only.** Node identity, the live behavioural record, energy and thermal readings are recorded and reported. Nothing in v1 makes the router act on continuous observed behaviour. Join-time measurement (calibration probe, canaries, the stricter of declared and measured) feeds routing as before. The router's event log and its stream into the behavioural record stay in v1 because this needs them. The event and telemetry logs keep everything in v1, with a fixed free-disk guard and no stale limit. Telemetry stories 70 to 72 (drift, backtesting, routing on observed behaviour) are a separate low-priority epic.
   - **Non-chat endpoint kinds.** Embeddings, transcription, speech, images, rerank and classify, each with a canary (the canary library lives in story 60 only). `decide` with a hosted node under the router's credential custody. Apple on-device node (optional) behind a Swift OpenAI-compatible shim.
6. **Admin surface (epic `admin-surface`, stories 95 to 99).** The versioned API under `/legatus/v1/`, auth with `read` and `control` scopes and an audit log, the dashboard and the MCP server in the separate stateless process `legatus-admin`, and the parity and leak checks. v1 control is lease acquire and release only. Each live epic adds one admin story and one MCP story (rows in the registries, no new conventions). No AppleScript.
   - **Accept:** a read token is refused on a control route; every control attempt appears in the audit log; the dashboard shows the pool and every figure the API offers; each MCP tool corresponds to a route and the dashboard and the tools show the same facts (parity); a planted stand-in key and a planted holder value appear in no route, page, tool result or audit entry. Real-process test: real router, real `legatus-admin` process, headless browser.
   - **Unproven:** the MCP library choice, pi's MCP client against `legatus-admin`, and the footprint of the extra process.

## Simulated cluster tests

A required epic, built from the first slice onward: `legatus-simcluster` provides stub engines that behave like the spike engines (Ollama serial, llama-server with slots, mlx_lm, hosted), a virtual clock, seeded concurrency checks and a scenario suite; `legatus-simtests` runs them against the real router and node binaries over real sockets. It exists because lease expiry (minutes to hours), restarts and contention cannot be tested on real hardware in useful time.

- **Accept:** a seeded run is reproducible; the scenarios cover lease capacity under contention, idle and maximum lifetime on virtual time, restart and crash recovery, a node restarting while leased, and a stuck lease reclaimed by TTL.
- **Unproven (OPEN):** that the real router can run on paused virtual time at all (the spike for it is in the first slice), and what the stubs cannot show: real engine timing and contention. The real-process tests above stay the proof; the simulator adds scenarios, it does not replace them.

## Restart and crash resilience of Legatus's own state

Part of the lease and event-log stories, listed here because it is easy to confuse with router resilience. It covers: the journal durability policy, listening early and serving after replay, degraded start on bad files, refusing a second instance, sleep and wake not counted as idle, and a node restarting while leased. It is not the paused v2 router resilience (timeouts, retries, breakers, failover), which is about requests to nodes. Restart-to-ready time and the macOS sleep clock are NOT MEASURED.

## v1.x and v2

- **Router resilience (PAUSED, v2+).** Timeouts, retries, circuit breakers, failover and health demotion, built at the two seams. Design in [router-resilience-design](../archive/router-resilience-design.md). Its acceptance tests (failover within a time bound, breaker open and close, timeout recovery) come back with it.
- **Additional harness adapters.** Each provides the generated config and the lease tools, against the same contract. Claude Code speaks only Anthropic Messages and sends its own session id headers (the spike completed a tool loop through the router on llama-server; interactive use and `count_tokens` are untested). opencode is not yet run. DeepSeek Harness is a developer preview (`dsh` 0.2.0-rc.2) that sends no session header on its chat and Anthropic routes. Per-adapter acceptance: a real harness binary at its pinned version acquires, uses and releases a lease, and a session stays pinned to one node.
- **Automatic acquire.** Intercepting the harness's spawn call so the main agent need not call the tools. Post-v1.
- **Lease-only token scope.** A paused backburner story in the security epic.
- **Behavioural telemetry, advanced (epic `behavioural-telemetry-advanced`).** LOW PRIORITY, post-v1, paused: drift detection, backtesting and routing on observed behaviour. Reason: v1 telemetry only records and reports, and acting on observed behaviour needs data that does not exist yet.
- **Conversation capture (epic `conversation-capture`).** A separate product, not v1; its stories are paused except the router event log and the router event stream into the behavioural record, which stay active because telemetry needs them. Conversation content is not logged by the router in v1. See the [scope reset](../decisions/2026-10-scope-reset.md#decisions-after-the-first-cross-check).

## Risks

- **Tool-calling variance.** Quality differs by model and engine. No pair joins a role without passing the smoke test.
- **Laptop nodes.** The M5 Max may sleep; with no failover in v1 a request to a sleeping leased node returns an error to the agent.
- **Hung nodes.** With resilience deferred, a hung node holds the harness for its own request timeout (300 s for pi in the spike).
- **Harness churn.** pi moves fast; pin versions and keep the spike harnesses as a compatibility check. Ollama auto-updates, so freeze it in tests.
- **Untested hardware.** The spikes ran on one Apple M2 with a 1.7B model. CUDA, Vulkan, Strix Halo and larger models are open.
- **Unconfirmed defaults.** The values in the parameter table are reasoned defaults, not measurements; the lease parameters (1800 and 14400 seconds) are PROPOSED defaults with no measurement behind them. The acceptance tests of phases 2, 3 and 5 are how they get confirmed.
- **Proposed lease design.** The allocation design is the coordinator's proposal refined by the owner; expect it to change. Its weakest assumption is that a pi 1.0.3 extension can set a per-session header after acquire on pi's chat path (ASSUMPTION, NOT TESTED); the first task of the pi adapter story proves it.
- **Unproven pieces.** The paused-time router for the simulator, restart-to-ready time, the macOS sleep clock, the MCP library and `legatus-admin` footprint are listed as open items in the [decision record](../decisions/2026-10-scope-reset.md#open-items-after-the-first-cross-check).
- **Stuck leases wait for the TTL.** With no operator override, a crashed holder's slots are free only after the idle TTL or maximum lifetime.
- **Slot counts.** The engine defaults (Ollama 1, llama-server 4 as run, mlx_lm 1, hosted 4 PROPOSED, Apple shim 1) come from one machine and a 1.7B model; no real-engine contention test has been run.

## Answered

- **Progress while pi-subagents blocks.** No longer a Legatus concern: spawning and result handling are the harness's, and Legatus sees only requests and lease calls.
