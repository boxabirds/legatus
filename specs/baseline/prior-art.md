# Prior art and build-versus-borrow

As of October 2026 nothing found does the whole job, in particular choosing a node by live load across a mixed home-lab fleet. Several projects cover parts of it and are worth borrowing from.

## Closest projects

| Project | Covers | Gap for Legatus |
| --- | --- | --- |
| [Bernstein](https://mcpservers.org/tr/servers/chernistry/bernstein) | Deterministic (non-LLM) scheduler, per-task model routing, local for boilerplate and cloud for architecture, worktree per task, bulletin board | No load-aware node choice; pi adapter unconfirmed |
| [MCP Agent Mail](https://github.com/Dicklesworthstone/mcp_agent_mail) | Agent identities, inboxes, searchable messages, advisory file leases with stale-lease release | Leases advisory only; no task dependencies |
| [agent-comm](https://github.com/keshrath/agent-comm) | Hooks that enforce file locks on every edit and guard commits, pushes and builds | Built around Claude Code hooks |

Lesson taken from agent-comm: MCP tools alone rely on the agent remembering to call them, so Legatus enforces leases with a pi hook.

## Claude Code agent teams

Experimental feature with a lead, teammates, a shared task list with dependencies, per-agent mailboxes and lifecycle hooks (`TeammateIdle`, `TaskCreated`, `TaskCompleted`). Roles carry their model via subagent definitions. Locking covers task claims only; the docs warn that two teammates editing the same file overwrite each other. State is local JSON files under `~/.claude/teams` and `~/.claude/tasks`, so it is single-machine and not an open protocol. Legatus copies the pattern over MCP and adds path leases and load-aware placement. Source: [Claude Code docs](https://code.claude.com/docs/en/agent-teams).

## Routing inside inference servers: rejected

- **llama-server router mode** loads, unloads and switches models on one machine, routed by model name. Useful for per-node model lifecycle; not cross-machine or role-based. [Hugging Face blog](https://huggingface.co/blog/ggml-org/model-management-in-llamacpp)
- **vLLM Semantic Router** classifies prompts by intent and complexity and routes to model lanes, with session continuity since v0.3. Envoy and Kubernetes oriented. [vLLM blog](https://vllm.ai/blog/2026-07-21-vllm-sr-new-chapter-mom)

Decision: routing stays in a layer Legatus owns. Routing inside an inference server ties you to that server, and Kubernetes is out of proportion for a handful of nodes. Techniques (such as a small escalation classifier) may be borrowed; engines remain swappable leaves.
