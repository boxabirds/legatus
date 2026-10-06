# Product vision

Legatus is a heterogeneous proxy that semantically links a subagent task with a local inference resource. It owns the LLM pool it exposes, and nothing else. See the [scope reset](../decisions/2026-10-scope-reset.md).

## Problem

Home labs now hold several capable but very different machines: Apple Silicon, AMD Strix Halo, Nvidia GPUs, and soon DGX Spark. Today a harness session uses one model on one machine. The rest of the fleet sits idle, and there is no open way to match a subagent's task to the machine that suits it, or to hand that machine to the subagent for the length of its task.

## Goals

1. A subagent asks for an LLM by its semantic needs (a role, or capabilities, an endpoint kind, a protocol and a context size), is given a node, and frees it at the end of its task. A lease limits concurrency, and the pool shows when every resource is busy. A caller can free only its own lease. See [LLM pool allocation](coordination.md).
2. Node choice follows capability, health and load, while keeping each session on one node so prompt caches stay warm.
3. Adding a machine (a 5060 Ti, a DGX Spark) is one registry entry, not a redesign.
4. Roles are defined by the capabilities and endpoint kind the work needs, so a new kind of node (an embedder, a speech model, an on-device model) joins without changing the roles.
5. Each node is described by what its model can do, what its engine exposes and how it behaves when measured, including power and thermal readings per machine.
6. Behavioural telemetry on how nodes perform in use: identity, a live record, energy and thermal readings, recorded and reported only. Nothing in v1 makes the router act on continuous observed behaviour; join-time measurement (the calibration probe and canaries) still feeds routing. Drift detection, backtesting and routing on observed behaviour are a separate low-priority epic after v1. The collection consent switch is off by default.

## Non-goals

- Coordinating anything other than LLM capacity: files, branches, worktrees, a task board, messaging, merges.
- Delegation: spawning subagents, collecting their results, or verifying them. Agents lie, so Legatus never says work is done.
- Guard and sandbox: confining what an agent may write is a harness concern.
- Remote execution.
- Router resilience (timeouts, retries, circuit breakers, failover) in v1: an endpoint failure goes to the agent. It is v2 or later; see [dispatcher](dispatcher.md#v2-resilience).
- Price and budget.
- Escalation gating: deciding when a role may be called.
- Splitting one model across machines (exo-style). Each request is served whole by one node.
- Replacing the harness. The harness stays the agent runtime. pi 1.0.3 is the first harness; Claude Code, opencode and DeepSeek Harness are later adapters, not non-goals.
- Multi-user or cloud scale. This is a single-owner home lab. No Kubernetes.
- Conversation capture is a separate product, not v1. Only the router's event log and its stream into the behavioural record stay, because telemetry needs them. The router logs metadata only: no prompt or completion text.
- A waiting queue: a refused acquire goes back to the caller.
- Automatic acquire by intercepting the harness's spawn call (post-v1); in v1 the main agent calls the lease tools.
- An operator override that frees someone else's lease, and admin control actions beyond lease acquire and release (table reload, drain, calibration or canary runs). The admin surface never edits the registry.
- An AppleScript API. The dashboard and the MCP server are the two admin clients.
- Placeholder routes: no admin route exists before the feature behind it is built.
- An embedded database. Records are JSONL or JSON files.
- Router resilience in v1, which includes the v2 router work; resilience of Legatus's own state across restart and crash is in scope and is a different thing.

All of the removed designs are kept in the [archive](../archive/); the isolate-first principle for resources lives there too.

## Principles

- **Own the routing; keep engines swappable.** Routing logic never lives inside an inference server. Putting it there ties you to that engine, and at the state of the art you cannot afford to be wedded to one. Engines are leaves behind an OpenAI-compatible contract.
- **Roles require capabilities.** A role declares what it needs (capabilities, an endpoint kind and one protocol), and a node qualifies by what its model, its engine and a measurement say it can do. Declared claims never outrank measured ones.
- **Harnesses are adapters.** The router is harness-neutral. Only the thin adapter (the config generator and the acquire and release surface) is per-harness, so a second harness is an adapter, not a rewrite.
- **Open standards at every boundary.** OpenAI-compatible and Anthropic Messages for inference, AGENTS.md for repo instructions.
- **Pin, don't spray.** Sessions stick to a node; load balancing is placement at acquire time, not round-robin.
- **Select at the subagent task, not the request.** A subagent is spawned with a task and a model endpoint and returns a result; Legatus is the part that matches that task with a model. The task is the deliberate unit of selection, because the host agent chooses the granularity: "summarise this document", "build this spec" and "research this topic" are all legitimate subagent calls. Within a task the model does not change for quality or preference reasons. Choosing clever models by role is a large body of prior work that Legatus does not extend. It declares coarse requirements (capabilities, context, endpoint kind) and, inside the resulting set of interchangeable nodes, keeps affinity.
- **Simplest thing first, no speculative machinery.** Build what a live story needs and no more: no placeholder route, no parameter for a feature that is not built, no heartbeat where observing requests will do, no queue where a refusal will do. A feature returns when something real needs it, and the design is kept in the archive until then.
- **Restart resilience of our own state.** Legatus's own state (the lease journal, the event and telemetry logs, its settings files) survives a restart or a crash: replay before serving, start degraded rather than not at all on a bad file, refuse a second instance, do not count sleep as idle. This is not router resilience for node failures, which is v2+.
- **Minimal footprint.** Anything shared, long-lived or on an inference node is Rust, and the dashboard process is small and stateless. See [implementation](implementation.md).
- **Lightweight harness.** Built on pi for its small system prompt, which matters when every subagent call pays prefill on slower hardware.
- **Deterministic routing.** Rules and a registry choose nodes, not another LLM.
- **Credential custody in the router.** The key of a hosted or frontier node lives only in the router, never in an agent. This is the whole of least privilege in v1.
