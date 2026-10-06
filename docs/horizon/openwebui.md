# Open WebUI as a client of Legatus

Status: researched from documentation on 2026-10-06. Not run, not tested. Everything below is a reading of public documentation and search results unless it says otherwise.

## What it is

[Open WebUI](https://github.com/open-webui/open-webui) is a self-hosted chat platform. It talks to Ollama and to any OpenAI-compatible endpoint (the README names LM Studio, GroqCloud, Mistral, OpenRouter and vLLM), and it can hold several connections at once. It adds tools, filters, actions and "pipes" (Python plugins that run inside it), MCP over HTTP and through a proxy for stdio servers, per-user and per-group access control, single sign-on and a built-in file and terminal tool set. The README describes agents as "wrap any base model with custom instructions, tools, and knowledge". It ships as pip, Docker or Kubernetes packages under a custom "Open WebUI License" that requires preserving its branding.

## Sub-agents, as documented

- The model starts a sub-agent by calling the built-in `delegate_task` tool with a `task`, an optional `context`, optional file ids and an optional `background` flag.
- A sub-agent uses the same model as its parent chat, with the same tools, skills and filters. The documentation says nothing about choosing a different model for a sub-agent.
- Several delegations in one reply run at the same time. The admin limits for concurrent foreground and background sub-agents default to 20 each.
- Only the sub-agent's final result goes back to the parent. Its intermediate calls stay in its own chat.
- It needs native function calling, which has been the default since v0.10.0, and it works only for chats started in the interface.
- A recorded issue says built-in tools behave differently for plain HTTP callers of `/api/chat/completions` than for the interface.

## Why it might matter

Open WebUI is a different kind of client from the coding harnesses in the plan. It is multi-user, it is a web interface that non-developers use, and it already speaks the OpenAI protocol that Legatus serves for local roles. If Legatus can be its model backend, the same pool of local nodes serves chat users and coding agents. It also uses the non-chat endpoints (embeddings for document search, speech to text, text to speech, image generation), which is what the endpoint-kind routing in the specs is for.

## How it could work with Legatus

1. **One connection pointing at the router.** Open WebUI lists models from the connection it is given. Roles would appear as model names. Whether the router serves a model list is not checked: the pi adapter uses a generated static `models.json`, so no spike needed it. This is the first thing to confirm in the stories.
2. **Session identity.** With `ENABLE_FORWARD_USER_INFO_HEADERS` set, Open WebUI forwards the chat id (`X-OpenWebUI-Chat-Id`) and user fields to the backend. A sub-agent has its own chat, so it would get its own chat id and so its own pin. The router keys only on `x-session-affinity` (decision of 2026-10-06), so the connection or a plugin has to map the chat id onto that header. Whether Open WebUI can set a custom header per connection is not checked.
3. **Non-chat features.** Embeddings, transcription, speech and image requests go to OpenAI-style paths. They fit the endpoint-kind stories. Which paths Open WebUI sends, and with what bodies, is not checked.
4. **Task models.** Open WebUI uses a separately chosen model for small side calls such as titles, tags and autocomplete. It would need a role of its own, as Claude Code's small-model calls do.
5. **An adapter inside Open WebUI.** Its Functions, Pipes and Filters are Python code running in its process, so an adapter could set headers and call the lease API. The Legatus MCP server could also be added as an MCP tool server, since Open WebUI supports HTTP MCP. Neither is tested. The adapter contract in the specs has three capabilities (model endpoint with a role string, a stable session identity, lease acquire and release), and each of the three would need a way to be met here.

## What it exposes or breaks in the current design

- **Same model for every sub-agent.** Legatus matches a subagent task with a model. In Open WebUI every sub-agent shares its parent's model, so Legatus can choose a node for the role but never a different kind of model for the sub-agent.
- **Parallel fan-out meets capacity.** Twenty parallel sub-agents on one role is the case the lease design is meant to prevent, but Open WebUI would not call a capacity check, and in v1 requests without a lease take no slot. Against a single-slot node the extra requests queue. In the engine spike a queued request on a serial Ollama node saw 44 s to first byte on a large prompt. The practical mitigation is to set Open WebUI's concurrency limits to the pool's slot count, by hand. A router that refuses beyond a role's total slots even for lease-less requests is a possible future change, and it is not planned.
- **Multi-user data.** Forwarded headers carry user name, id, email and role. The router would see them. They should be stripped before any hosted node, and the event log must not record them. This applies the metadata-only rule to a new source.
- **Conversation state lives in Open WebUI.** It keeps its own chat history, so Legatus would not need to store conversations for it, which is consistent with the stateless-about-conversations position.
- **Licence.** Running Open WebUI as a client is a use, not a redistribution. Bundling it with Legatus would be a different matter and has not been looked at.

## Checked and not checked

Checked (read, not run): the repository overview, the sub-agents documentation page, a documentation search result on the tools and header-forwarding features, and the pull request that adds the chat-id header. The version and the exact setting names are as the sources state them on 2026-10-06 and may have moved.

Not checked: any behaviour of a running Open WebUI; whether a custom header can be set per connection; the paths and bodies of its embedding, audio and image requests; whether its sub-agent requests carry a chat id different from the parent's in practice; how it renders a refusal such as `no_capacity` or `lease_ended`; how many requests a 20-way delegation sends at once; whether its MCP client works with the Legatus MCP server; whether its plugin API can hold an adapter.

## Trigger to pick it up

Someone wants a shared chat interface over the local fleet, or wants Legatus tested by a client that fans out in parallel. If it is picked up, it becomes an adapter story in the harness-adapters epic, after the pi adapter, with a first task that re-checks the current Open WebUI version and runs these four checks against the simulated cluster: it lists roles from the model list, the chat id reaches the router as a session key, a 20-way delegation behaves under the slot limits, and a refusal is shown to the user in a usable form.

## Sources

- [Open WebUI repository](https://github.com/open-webui/open-webui)
- [Sub-agents documentation](https://docs.openwebui.com/features/chat-conversations/chat-features/subagents/)
- [Tools documentation](https://docs.openwebui.com/features/extensibility/plugin/tools/)
- [Pull request exposing the chat id header](https://github.com/open-webui/open-webui/pull/15813)
- [Discussion of native tool calling through the HTTP API](https://github.com/open-webui/open-webui/discussions/23342)
