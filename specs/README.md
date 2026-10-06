# Legatus

Loosely-coupled language model pool allocation for coding agents.

Legatus is a heterogeneous proxy that semantically links a subagent task with a local inference resource across a home lab (Apple Silicon, AMD Strix Halo, Nvidia GPUs, DGX Spark). It owns the LLM pool it exposes and nothing else: a subagent asks for an LLM by its semantic needs, is given a lease on a node, and frees it at the end of its task. A lease limits concurrency; a caller can free only its own lease.

Status: design stage; spikes completed; scope reset on 2026-10-06 (see the [decision record](decisions/2026-10-scope-reset.md)); no product code. The lease design is PROPOSED and the owner will iterate; the decisions made since, with their consequences, and the honest list of what is still unproven are in the [same record](decisions/2026-10-scope-reset.md#decisions-after-the-first-cross-check).

## How it works

- **pi** (1.0.3) is the first harness; Claude Code, opencode and DeepSeek Harness follow as adapters. The adapter provides generated config and the tools the main agent calls explicitly in v1: `can_spawn` (advisory preflight), `request_llm` (acquire) and `release_llm`.
- **`legatus-router`** (Rust) is a proxy for OpenAI chat-completions and Anthropic Messages that routes each role to a node by capability, health, context limit and load, keeps sessions sticky for prompt caching, holds the lease table, serves the versioned admin API, logs metadata only (never prompt or completion text), and keeps a hosted node's key away from agents.
- **`legatus-node`** (Rust) reports health, load, and power and thermal readings per machine; it owns those readings, and telemetry only records and reports them.
- **`legatus-admin`** (Rust) is a separate small stateless process serving the dashboard and the MCP server as thin clients of the router's admin API. v1 control is lease acquire and release only; there is no AppleScript API.
- **Engines are swappable.** Any server with a known endpoint contract can serve a node (OpenAI-compatible for chat). Routing never lives inside an inference engine.
- **Not v1:** conversation capture (a separate product; only the router event log and its stream into the behavioural record stay) and advanced telemetry (drift, backtesting, routing on observed behaviour; a separate low-priority epic).
- **Simulated cluster tests** (stub engines, virtual time, seeded concurrency checks) are a required epic; they add scenarios and do not replace real-process tests.
- **Out of scope:** coordination of anything but the LLM pool, delegation, worktrees, guard and sandbox, remote execution, price and budget, the escalation gate, a waiting queue, an operator override of leases, and (until v2) router resilience for node failures. Restart and crash resilience of Legatus's own state is in scope.

## Docs

- [Product vision](baseline/vision.md): problem, goals, non-goals, principles
- [Technical architecture](baseline/architecture.md): layers, admin surface, protocols, credential custody, failure modes, parameters
- [Role registry](baseline/registry.md): nodes, roles, priorities, adding hardware
- [Dispatcher and router](baseline/dispatcher.md): node selection, live signals, affinity, v2+ resilience seams
- [LLM pool allocation](baseline/coordination.md): leases on LLM capacity, holder rules, preflight, restart behaviour (PROPOSED)
- [Implementation languages](baseline/implementation.md): Rust versus TypeScript, workspace layout, and why
- [Prior art](baseline/prior-art.md): closest projects and what Legatus borrows
- [Roadmap and risks](baseline/roadmap.md): epics and build order, the first slice, phases, acceptance tests, simulated cluster tests, spikes completed
- [Horizon](../docs/horizon/README.md): future possibilities that are not in the v1 plan (for example Open WebUI as a client); nothing there is a commitment
- [Spike decisions](decisions/2026-10-spike-decisions.md): decisions D-1 to D-5, proposed defaults P-1 to P-5 and verified facts from the October 2026 spikes (partially superseded by the scope reset)
- [Scope reset](decisions/2026-10-scope-reset.md): the owner's decisions of 2026-10-06, consequences, seams, the decisions after the first cross-check, and open items

## Archive

Designs removed from v1 scope, kept for the future. Each starts with a banner saying why and what would trigger revisiting it.

- [Coordination: full design](archive/coordination-full-design.md): file, branch and resource leases, task board, messaging, negotiation, guard, event store
- [Router resilience](archive/router-resilience-design.md): timeouts, breaker, failover, health demotion (v2+)
- [Delegation, worktrees, guard, sandbox and remote execution](archive/delegation-worktrees-guard-remote-design.md)
- [Price and budget](archive/budget-design.md)

## Licence

Apache-2.0. See [LICENSE](../LICENSE).
