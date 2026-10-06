# Product vision

One coding session on a laptop directs a team of specialist agents. Each agent runs on whichever home-lab machine or frontier model suits its role and is free right now. Local models do the volume work; a frontier model does the scarce, high-value thinking.

## Problem

Home labs now hold several capable but very different machines: Apple Silicon, AMD Strix Halo, Nvidia GPUs, and soon DGX Spark. Today a harness session uses one model on one machine. The rest of the fleet sits idle, and there is no open way to hand work to it, balance load across it, or stop agents colliding over shared files and releases.

## Goals

1. A single pi session on the client machine can spawn role-specific subagents whose models run on other machines.
2. Role assignment follows the nature of the task: architect, researcher, tester, spec writer, tech writer, marketing comms, release manager.
3. Node choice follows live health and load, while keeping each session on one node so prompt caches stay warm.
4. Agents coordinate over shared resources (files, branches, GPUs, releases) without stepping on each other.
5. Adding a machine (a 5060 Ti, a DGX Spark) is one registry entry, not a redesign.
6. Roles are defined by the capabilities and endpoint kind the work needs, so a new kind of node (an embedder, a speech model, an on-device model) joins without changing the roles.

## Non-goals

- Splitting one model across machines (exo-style). Each request is served whole by one node.
- Replacing the harness. The harness stays the agent runtime and Legatus adds routing and coordination around it. pi 1.0.3 is the first harness; Claude Code, opencode and DeepSeek Harness are later adapters, not non-goals.
- Multi-user or cloud scale. This is a single-owner home lab. No Kubernetes.

## Principles

- **Frontier drives, local executes.** The frontier model orchestrates and takes the deepest reasoning roles. Traffic to it is small but valuable, and it benefits from very large context windows. Local models take everything else by preference.
- **Own the routing; keep engines swappable.** Routing logic never lives inside an inference server. Putting it there ties you to that engine, and at the state of the art you cannot afford to be wedded to one. Engines are leaves behind an OpenAI-compatible contract.
- **Roles require capabilities.** A role declares what it needs (capabilities, an endpoint kind and one protocol), and a node qualifies by what its model, its engine and a measurement say it can do. Declared claims never outrank measured ones.
- **Harnesses are adapters.** The router and coordination service are harness-neutral. Only the guard, `delegate` and the config generator are per-harness, so a second harness is an adapter, not a rewrite.
- **Isolate first, lock only what cannot be isolated or merged.** One worktree per writer; leases are for the few shared things that remain.
- **Open standards at every boundary.** OpenAI-compatible and Anthropic Messages for inference, MCP for tools and coordination, ACP for spawning agents remotely, AGENTS.md for repo instructions.
- **Pin, don't spray.** Sessions stick to a node; load balancing is failover and overflow, not round-robin.
- **Negotiate in language, enforce in leases.** Capable agents may resolve contention in plain English; leases guarantee the outcome.
- **Minimal footprint.** Anything shared, long-lived or on an inference node is Rust; anything that runs per tool call stays in-process in the harness. See [implementation](implementation.md).
- **Lightweight harness.** Built on pi for its small system prompt, which matters when every subagent call pays prefill on slower hardware.
- **Deterministic routing.** Rules and a registry choose nodes, not another LLM.
- **Least privilege.** Every agent, tool, credential and lease is scoped to the minimum its role needs, and anything not granted is denied. Roles get explicit tool profiles; leases cover the narrowest resource that works; the frontier API key, and the key of any hosted node, lives only in the router, never in an agent. The network is assumed to be a trusted local one, so privilege is enforced at the agent and resource level, not by network controls.
