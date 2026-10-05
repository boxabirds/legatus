# Legatus

Resilient, loosely-coupled language model cluster management.

Legatus lets one coding session direct a team of specialist agents across a heterogeneous home lab. A frontier model orchestrates and takes the deepest reasoning; role-based subagents run on local machines (Apple Silicon, AMD Strix Halo, Nvidia GPUs, DGX Spark) chosen by role, health and live load. Agents coordinate over shared files, branches and releases without stepping on each other.

Status: design stage. No code yet.

## How it works

- **pi** is the harness. A frontier model runs the main session; pi-subagents spawns each role as its own process.
- **`delegate(role, task)`** is a thin pi extension. The orchestrator picks the role; Legatus picks the node.
- **`legatus-router`** (Rust) is an OpenAI-compatible proxy that routes each role to a node by health, context limit and load, keeps sessions sticky for prompt caching, and caps frontier spend.
- **`legatus-coord`** (Rust, MCP) gives every agent leases, a task board with dependencies, and inboxes. Agents may negotiate in plain language; leases enforce the outcome.
- **Engines are swappable.** Any OpenAI-compatible server can serve a node. Routing never lives inside an inference engine.

## Docs

- [Product vision](baseline/vision.md): problem, goals, non-goals, principles
- [Technical architecture](baseline/architecture.md): layers, protocols, isolation, frontier budget
- [Role registry](baseline/registry.md): nodes, roles, priorities, adding hardware
- [Dispatcher and router](baseline/dispatcher.md): node selection, live signals, affinity
- [Coordination service](baseline/coordination.md): leases, task board, messaging, negotiation
- [Implementation languages](baseline/implementation.md): Rust versus TypeScript, and why
- [Prior art](baseline/prior-art.md): closest projects and what Legatus borrows
- [Roadmap and risks](baseline/roadmap.md)

## Licence

Apache-2.0. See [LICENSE](../LICENSE).
