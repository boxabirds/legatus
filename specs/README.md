# Legatus

Loosely-coupled language model pool allocation for coding agents.

Legatus is a heterogeneous proxy that semantically links a subagent task with a local inference resource across a home lab (Apple Silicon, AMD Strix Halo, Nvidia GPUs, DGX Spark). It owns the LLM pool it exposes and nothing else: a subagent asks for an LLM by its semantic needs, is given a lease on a node, and frees it at the end of its task.

Status: design stage; spikes completed; scope reset on 2026-10-06 (see the [decision record](decisions/2026-10-scope-reset.md)); no product code. The lease design is PROPOSED and the owner will iterate.

## How it works

- **pi** (1.0.3) is the first harness; Claude Code, opencode and DeepSeek Harness follow as adapters. The adapter provides generated config and acquire and release of an LLM lease.
- **`legatus-router`** (Rust) is a proxy for OpenAI chat-completions and Anthropic Messages that routes each role to a node by capability, health, context limit and load, keeps sessions sticky for prompt caching, holds the lease table, and keeps a hosted node's key away from agents.
- **`legatus-node`** (Rust) reports health, load, and power and thermal readings per machine.
- **Engines are swappable.** Any server with a known endpoint contract can serve a node (OpenAI-compatible for chat). Routing never lives inside an inference engine.
- **Out of scope:** coordination of anything but the LLM pool, delegation, worktrees, guard and sandbox, remote execution, price and budget, the escalation gate, and (until v2) router resilience.

## Docs

- [Product vision](baseline/vision.md): problem, goals, non-goals, principles
- [Technical architecture](baseline/architecture.md): layers, protocols, credential custody, failure modes, parameters
- [Role registry](baseline/registry.md): nodes, roles, priorities, adding hardware
- [Dispatcher and router](baseline/dispatcher.md): node selection, live signals, affinity, v2+ resilience seams
- [LLM pool allocation](baseline/coordination.md): leases on LLM capacity (PROPOSED)
- [Implementation languages](baseline/implementation.md): Rust versus TypeScript, and why
- [Prior art](baseline/prior-art.md): closest projects and what Legatus borrows
- [Roadmap and risks](baseline/roadmap.md): phases, acceptance tests, spikes completed
- [Spike decisions](decisions/2026-10-spike-decisions.md): decisions D-1 to D-5, proposed defaults P-1 to P-5 and verified facts from the October 2026 spikes (partially superseded by the scope reset)
- [Scope reset](decisions/2026-10-scope-reset.md): the owner's decisions of 2026-10-06, consequences, seams and open items

## Archive

Designs removed from v1 scope, kept for the future. Each starts with a banner saying why and what would trigger revisiting it.

- [Coordination: full design](archive/coordination-full-design.md): file, branch and resource leases, task board, messaging, negotiation, guard, event store
- [Router resilience](archive/router-resilience-design.md): timeouts, breaker, failover, health demotion (v2+)
- [Delegation, worktrees, guard, sandbox and remote execution](archive/delegation-worktrees-guard-remote-design.md)
- [Price and budget](archive/budget-design.md)

## Licence

Apache-2.0. See [LICENSE](../LICENSE).
