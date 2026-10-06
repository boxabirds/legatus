# Legatus

Resilient, loosely-coupled language model cluster management.

Legatus lets one coding session direct a team of specialist agents across a heterogeneous home lab. A frontier model orchestrates and takes the deepest reasoning; role-based subagents run on local machines (Apple Silicon, AMD Strix Halo, Nvidia GPUs, DGX Spark) chosen by role, health and live load. Agents coordinate over shared files, branches and releases without stepping on each other.

Status: design stage; spikes completed; no product code.

## How it works

- **pi** (1.0.3) is the first harness; Claude Code, opencode and DeepSeek Harness follow as adapters. A frontier model runs the main session; pi-subagents runs each role as a subagent, using async child processes so the orchestrator is not blocked.
- **`delegate(role, task)`** is a thin pi extension. The orchestrator picks the role; Legatus picks the node.
- **`legatus-router`** (Rust) is a proxy for OpenAI chat-completions and Anthropic Messages that routes each role to a node by capability, health, context limit and load, keeps sessions sticky for prompt caching, and caps frontier spend.
- **`legatus-coord`** (Rust; Unix socket and MCP) gives every agent leases on shared resources, a task board with dependencies, and inboxes. Agents may negotiate in plain language; leases enforce the outcome.
- **Engines are swappable.** Any server with a known endpoint contract can serve a node (OpenAI-compatible for chat). Routing never lives inside an inference engine.

## Docs

- [Product vision](baseline/vision.md): problem, goals, non-goals, principles
- [Technical architecture](baseline/architecture.md): layers, protocols, isolation, frontier budget
- [Role registry](baseline/registry.md): nodes, roles, priorities, adding hardware
- [Dispatcher and router](baseline/dispatcher.md): node selection, live signals, affinity
- [Coordination service](baseline/coordination.md): leases, task board, messaging, negotiation
- [Implementation languages](baseline/implementation.md): Rust versus TypeScript, and why
- [Prior art](baseline/prior-art.md): closest projects and what Legatus borrows
- [Roadmap and risks](baseline/roadmap.md): phases, acceptance tests, spikes completed
- [Spike decisions](decisions/2026-10-spike-decisions.md): decisions D-1 to D-5, proposed defaults P-1 to P-5 and verified facts from the October 2026 spikes

## Licence

Apache-2.0. See [LICENSE](../LICENSE).
