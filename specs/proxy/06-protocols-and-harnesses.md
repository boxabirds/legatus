# 06 Protocols and harnesses

Status: draft for review. Date: 2026-10-07. Owner of IDs: PRX-PROTO.

This file defines what the proxy speaks, what it changes, how it forwards streams, which errors it returns, and how each harness behaves. Terms are in the [glossary](15-glossary.md). The architecture is in [02](02-architecture.md). Key rules are in [03](03-affinity-and-keys.md).

Evidence for the harness rows comes from [spike C](evidence/spikes/sC-key-stability/README.md) (key stability) and [spike D](evidence/spikes/sD-hold-tolerance/README.md) (hold tolerance and retries). Both ran real harness binaries against fake servers on 2026-10-07. No real model ran. The versions are Claude Code 2.1.291, pi 1.0.3, DeepSeek Harness (dsh) 0.2.0-rc.2, opencode 1.18.35, Codex 0.160.1, openai-node 7.30.0, anthropic-node 0.131.0, openai-python 3.26.0 and anthropic-python 1.11.0.

Labels: PROVEN means measured or read from source, with a reference. PROPOSED means a design choice that nobody measured. ASSUMPTION means believed and not checked. NOT TESTED means no test exists.

## 1. Protocols

| Protocol | Paths | Role of the proxy | Label |
| --- | --- | --- | --- |
| OpenAI chat completions | `POST /v1/chat/completions` | Route, rewrite `model`, forward. | PROVEN for the prototype (round 1, pi) |
| Anthropic Messages | `POST /v1/messages` | Route, rewrite `model`, forward. | PROVEN for the prototype (round 1, pi and Claude Code) |
| Model list | `GET /v1/models` | Answer from the registry aliases. | PROPOSED. Open WebUI lists models from its connection. |
| Token count | `POST /v1/messages/count_tokens` | Route like Messages. | NOT TESTED |
| Other OpenAI-style paths | Embeddings, audio, images | Pass by path to a node that serves the path. | Approved by the owner on 2026-10-07 (DEC-004). The path list is NOT TESTED |
| OpenAI Responses API | `POST /v1/responses` | Route, rewrite `model`, forward. The same rules as the chat path | Decided by the owner on 2026-10-07 (DEC-062, which supersedes DEC-057). Codex 0.160.1 ran against a fake server (PROVEN, spikes C and D). Pass-through only, no translation (DEC-065, owner decision pending). A node needs the `responses` flag. Engine support: [05](05-engine-behaviour.md) section 14 |

The proxy does not translate between protocols (see [01](01-decisions.md)). A request on the Messages path goes to a node that speaks Messages. A request on the chat path goes to a node that speaks chat. A request on the Responses path goes to a node that speaks Responses. The registry records the protocol of each node. See [07](07-registry-and-configuration.md).

| ID | Requirement |
| --- | --- |
| PRX-PROTO-001 | The proxy must serve the paths that the table in section 1 marks PROVEN, without a change in the harness. |
| PRX-PROTO-002 | The proxy must send a request to a node that speaks the protocol of the request. |
| PRX-PROTO-003 | The proxy must refuse a request whose alias has no node for the protocol of the request. |
| PRX-PROTO-004 | The proxy must pass an unknown path to a node that serves the path, or answer 404 when no node serves it. |
| PRX-PROTO-005 | The proxy must answer `GET /v1/models` from the aliases of the registry (PROPOSED). |
| PRX-PROTO-042 | REMOVED 2026-10-07. The 404 on `/v1/responses` ended with DEC-062. |
| PRX-PROTO-043 | The proxy must serve `POST /v1/responses` by the rules of the chat path. It must route by `model`, rewrite `model` and forward the response byte for byte. |
| PRX-PROTO-044 | The proxy must send a Responses request only to a node whose registry flag `responses` is true. PRX-PROTO-056 governs an alias with no such node. |
| PRX-PROTO-045 | The proxy must write each refusal on the Responses path in the OpenAI error shape. |

### 1.1 Responses API: pass-through only

Source: [R7](evidence/research/r7-responses-api-20261007T081115Z.md), Codex 0.160.1, read from source and observed against a fake server (spikes C and D). No live engine ran with Codex. The decision is DEC-065 (proposed by coordinator, owner decision pending).

Facts about Codex 0.160.1 on a custom provider (PROVEN unless marked):

- It sends `POST /v1/responses` over HTTP with `stream: true`, `store: false` and `include: ["reasoning.encrypted_content"]`. It sends `prompt_cache_key` equal to the session id. It sends the full history in `input` every turn. The body grows by about 340 bytes for each tool loop.
- It sends `previous_response_id` only on a websocket path. Websocket is on only for the built-in OpenAI provider. A custom provider uses HTTP and sends no `previous_response_id` (source, observed in 22 requests).
- The headers are `session-id`, `thread-id`, `x-client-request-id`, `x-codex-window-id`, `x-codex-turn-metadata`, `x-codex-installation-id`, `x-codex-beta-features`, `x-codex-parent-thread-id` (subagents, source only), `x-codex-routing-hint` (source only) and `x-codex-turn-state` (see below).
- The server sets `x-codex-turn-state` on a response. Codex replays it on later requests of the same turn. Only the OpenAI backend sets it today (source, inferred).
- The SSE parser reads the JSON field `type` and not the `event:` line. It takes messages, tool calls and reasoning only from `response.output_item.done`. It ignores unknown event types and `response.function_call_arguments.delta`. It treats `usage` as optional.
- Codex retries a stream that closes before `response.completed`. The idle timeout is 300 s and a real SSE event resets it. A comment line does not reset it (spike D).
- The defaults are 5 stream retries and 4 request retries. Codex never retries a 429. It retries a 503 with `Retry-After` and honours the value.
- It calls `/v1/responses/compact` when remote compaction is on. Ollama and mlx-vlm serve it. llama-server does not (unverified).

| ID | Requirement |
| --- | --- |
| PRX-PROTO-054 | The proxy must pass a `/v1/responses` request and its response without translation to or from another protocol (DEC-065). |
| PRX-PROTO-055 | The proxy must read the `responses` flag of each node from the registry. The default is false. |
| PRX-PROTO-056 | The proxy must answer a Responses request with status 404 in the OpenAI error shape when no node of the pool has `responses` true. |
| PRX-PROTO-057 | The proxy must pass every Codex request header unchanged to a local node, with the exceptions of PRX-PROTO-016 and PRX-PROTO-017 for a hosted node. |
| PRX-PROTO-058 | The proxy must pass the header `x-codex-turn-state` unchanged on the request and on the response. |
| PRX-PROTO-059 | The proxy must change no byte of a Responses request body except the `model` value (PRX-REG-012). This covers item ids, `encrypted_content`, `include`, `store`, `reasoning` and the tool list. |
| PRX-PROTO-060 | The proxy must forward each Responses SSE event unchanged and in order, and must not parse an event except `response.created` and `response.completed` on a node with `stateful_responses` true. |
| PRX-PROTO-061 | The proxy must record the `response.id` of a node with `stateful_responses` true in a table that maps the id to the node. The proxy must route a request with that `previous_response_id` to that node. |
| PRX-PROTO-062 | The proxy must answer 400 in the OpenAI error shape to a request with `previous_response_id` when the target node has `ignores_previous_response_id` true. PROPOSED. The engine would ignore the id and lose the context with no error. A warning with pass-through is the alternative (OPEN-041). |
| PRX-PROTO-063 | The proxy must route `/v1/responses/compact` and other paths below `/v1/responses` by the rules of `/v1/responses` and must pass the answer of the node, also an error. |
| PRX-PROTO-064 | The proxy must use the error code `context_length_exceeded` in a context refusal on the Responses path. PROPOSED. How Codex reads an HTTP error body before the stream starts is NOT TESTED. |
| PRX-PROTO-065 | The proxy must answer a request whose body is larger than the setting `body_limit_bytes` with status 413 in the error shape of the protocol. The default of the setting is NOT SET. |

The table of response ids is soft state. A restart loses it. A follow-up request then reaches a node that does not know the id and gets an error from the node. This has the same cost class as a cold turn. Without the table, a stateful engine loses the context of a conversation that the proxy sends to another node (a data-loss hazard, R7 inferred).

R7 advises `response.failed` with an `error.code` for an error after the head. The proxy does not send it, because PRX-PROTO-047 and PRX-PROTO-048 say that the proxy ends a Responses stream with an abrupt close. Codex retries a dropped stream up to 5 times (PROVEN, spike D). Section 5 keeps this rule. The owner can change it (OPEN-041).

Early headers hurt Codex unless the proxy sends a real SSE event at least every 300 s. The proxy sends none during a hold (PRX-ADM-045).

## 2. What the proxy reads and what it changes

The proxy reads a small part of the request body. It copies everything else byte for byte. The prompt cache of an engine depends on the exact tokens, so a changed byte in the prompt can break the cache.

| Item | Read | Change |
| --- | --- | --- |
| Top-level `model` | Yes. It names the alias. | Yes. Replace the value with the node model name. |
| Top-level `stream` | Yes. | No. |
| Key fields (see [03](03-affinity-and-keys.md)), including `instructions`, `input` and `prompt_cache_key` of a Responses request | Yes. | No. |
| `messages`, `system`, `tools` and their order | Only to compute the hash key. | Never. |
| Node patches (for example `chat_template_kwargs`, `reasoning_effort`, `max_tokens` limit) | No. | Yes. Fixed per node. |
| Unknown fields | No. | No. |
| Response body | Cache fields only, on the side. | Never. |

| ID | Requirement |
| --- | --- |
| PRX-PROTO-006 | The proxy must pass every byte of `messages`, `system`, `tools` and their order unchanged to the node. |
| PRX-PROTO-007 | The proxy must pass unknown request fields unchanged. |
| PRX-PROTO-008 | The proxy must not change the order of fields that it does not patch. |
| PRX-PROTO-009 | The proxy must not change a response body or a response header value that the node sends, except hop-by-hop headers. |
| PRX-PROTO-046 | The proxy must pass the fields `instructions`, `input`, `store`, `previous_response_id` and `prompt_cache_key` of a Responses request unchanged. |

## 3. Model name rewriting

The `model` field names an alias. The registry maps the alias to a pool of nodes. After the proxy selects a node, it writes the model name of that node in the request.

| Case | Rule |
| --- | --- |
| Alias found | Replace the value of the top-level `model` key. Copy all other bytes. |
| Alias not found | Answer 404 with a protocol-shaped error. The body must not hold the words in section 7. |
| Hosted node | Replace `model` with the hosted model name. Keep the protocol of the node. |
| Node patch present | Apply the patch after the rewrite. Use the same patch on every turn of a conversation. |
| Alias equal to node name | Rewrite still runs. The result can be the same bytes. |
| Response `model` field | Leave the node value. The proxy does not rewrite response bodies. |

Claude Code accepts any model string (PROVEN, round 1). NOT TESTED: whether every harness accepts a response `model` value that differs from the request. A harness that checks the value needs a rewrite of the response. That rewrite breaks the byte-faithful rule, so the proxy does not do it in v1 (OPEN).

A patch must be the same on every turn. A patch that changes between two turns changes the prompt and breaks the cache of the node. See [05](05-engine-behaviour.md) for the patch list per engine.

| ID | Requirement |
| --- | --- |
| PRX-PROTO-010 | The proxy must replace only the value of the top-level `model` key when it rewrites the model name. |
| PRX-PROTO-011 | The proxy must answer 404 when the `model` value names no alias. |
| PRX-PROTO-012 | The proxy must apply the patches of a node in the same way on every request that goes to the node. |
| PRX-PROTO-013 | The proxy must not rewrite a response body in v1. |
| PRX-PROTO-014 | The proxy must reject a request with no `model` value using status 400 and a protocol-shaped error. |
| PRX-PROTO-015 | The proxy must keep the rewrite independent of the turn number. |

## 4. Headers

| Direction | Rule |
| --- | --- |
| Request, to a local node | Pass all headers except hop-by-hop headers. Keep conversation and beta headers. |
| Request, to a hosted node | Remove inbound credentials (`authorization`, `x-api-key`). Remove every `x-legatus-*` header. Add the key of the node. |
| Request, identity headers from a harness | Read for the key. For a hosted node, remove user and chat identity headers. |
| Response | Pass all headers except hop-by-hop headers. |

Open WebUI can forward the user name, id, email and role. The proxy must not log these values. The proxy must remove them before a hosted node (PRX-SEC rules in [07](07-registry-and-configuration.md)).

| ID | Requirement |
| --- | --- |
| PRX-PROTO-016 | The proxy must remove inbound credentials before it sends a request to a hosted node. |
| PRX-PROTO-017 | The proxy must remove every `x-legatus-*` header before it sends a request to a hosted node. |
| PRX-PROTO-018 | The proxy must pass the `anthropic-beta` and `anthropic-version` headers unchanged to a Messages node. |
| PRX-PROTO-019 | The proxy must never write a session header value in the clear to the log or the admin API. |

## 5. SSE rules

The harness reads the stream as it arrives. The proxy forwards the stream. It does not parse the stream.

| Rule | Detail |
| --- | --- |
| Byte-faithful | The proxy copies the stream bytes with no re-encoding. The round 1 prototype gave byte-identical streams (PROVEN for the prototype). |
| No buffering | The proxy sends each chunk when it arrives. It does not wait for a full event. |
| Chunk boundaries | The proxy can split or join TCP chunks. It must not split inside an SSE event on purpose. It does not need to keep the TCP boundaries. |
| Ping and comments | Pass `: ping` comment lines and `event: ping` events from the node. The proxy sends no ping of its own in v1. |
| Terminator | Pass `data: [DONE]` on the chat path and `message_stop` on the Messages path. The proxy does not add one. |
| Headers | Pass `content-type: text/event-stream`. Do not add `content-length`. |
| Held request | The proxy sends no byte while it holds a request. It sends the status line after a cap place is free. |
| Backpressure | A slow harness slows the read from the node. The proxy does not grow a buffer without limit. |

A hold of 250 s with no byte completed at the first attempt in every harness tested (PROVEN, [spike D](evidence/spikes/sD-hold-tolerance/README.md)). The lowest time at which a harness times out with no byte is 299 s (pi, DeepSeek Harness). Section 8.2 gives the table. The proxy sends no head and no keep-alive during a hold, because it has not yet chosen the status code. DEC-064 rejects an early head with keep-alive for v1. See [04](04-admission-control-and-queueing.md), section 8.5.

### Errors before the first byte

The proxy sent no byte to the harness. It can return a normal HTTP error with a protocol-shaped body.

| Protocol | Status | Body shape |
| --- | --- | --- |
| Chat completions | As in section 7 | `{"error":{"message":"...","type":"...","code":"..."}}` |
| Messages | As in section 7 | `{"type":"error","error":{"type":"...","message":"..."}}` |
| Messages, overload | 529 | `{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}` |

The proxy passes an error status and body from a node unchanged. The proxy writes its own error only for its own refusals.

### Errors after the first byte

The proxy sent the status line and some bytes. The status cannot change. The proxy has no retry and no failover in v1. The proxy ends the stream in one of two ways.

| Path | Action when the node fails mid-stream | Evidence |
| --- | --- | --- |
| Chat completions | Close the connection with no terminator. pi reports `terminated` or `Stream ended without finish_reason`, retries, and discards the partial text. | PROVEN, s5 case 3b |
| Messages | Send one `event: error` with an `overloaded_error` body, then close. pi and Claude Code retry. | PROVEN for the prototype, round 1 |
| Messages, abrupt close | Close with no event. pi reports `Anthropic stream ended before message_stop` and retries. Claude Code retries a dropped connection (11 attempts). | PROVEN for pi (s5) and for Claude Code (spike D) |
| Responses | Close the connection with no terminator. Codex retries a dropped connection (30 requests). | PROVEN for Codex (spike D). PROPOSED as the default |

An `overloaded_error` does not describe a node crash. The proxy uses it because it triggers a retry in both harnesses. The cost is a wrong label in the harness log. The proxy must use it only when the node failed or hung up.

Spike D measured what each harness does after a 200 head when the stream then fails (PROVEN).

| Failure after the 200 | Harness | Result |
|---|---|---|
| Anthropic `overloaded_error` event | pi, opencode, Claude Code | Retried. pi made 4 attempts. Claude Code retried 3 times, then fell back to a non-stream request |
| OpenAI-style error event | openai and anthropic SDK streams, pi on the OpenAI path, DeepSeek Harness | Fail cleanly with the message. No retry |
| OpenAI-style error event | opencode | Retried 7 to 9 times, then failed with the message |
| Error event or stream end without `response.completed` | Codex | 5 reconnects, then failed |
| Spaces then an error body in a non-stream request | SDKs | A JSON parse error |
| Dropped connection at 5 s | SDKs, pi, DeepSeek Harness, opencode, Codex, Claude Code | Retried by all. curl and `fetch` fail at once |

The proxy therefore ends a chat or Responses stream with a dropped connection and a Messages stream with the `overloaded_error` event. An OpenAI-style error event stops the retry in most harnesses.

Messages event shape (PROPOSED, from the Anthropic stream format):

```
event: error
data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}

```

### Why `api_error` mid-stream is wrong

Claude Code treats an `api_error` event in a stream as a stream fault. It then falls back to a non-streaming request (PROVEN, round 1, [spike decisions](../decisions/2026-10-spike-decisions.md) section 3.2). The non-streaming request sends the full prompt again. The node must prefill again, with a possible cold cache on another node. The answer then arrives as one body after a long wait with no byte. For these reasons the proxy must never send `api_error` mid-stream.

| ID | Requirement |
| --- | --- |
| PRX-PROTO-020 | The proxy must copy response stream bytes to the harness without a change and without buffering a full event. |
| PRX-PROTO-021 | The proxy must pass ping events and comment lines from the node. |
| PRX-PROTO-022 | The proxy must add no SSE event of its own to a stream, except the error event in PRX-PROTO-025. |
| PRX-PROTO-023 | The proxy must send no byte to the harness while it holds the request. |
| PRX-PROTO-024 | The proxy must end a chat completions stream with an abrupt close when the node fails mid-stream. |
| PRX-PROTO-025 | The proxy must end a Messages stream with one `overloaded_error` event and then a close when the node fails mid-stream. |
| PRX-PROTO-026 | The proxy must never send an `api_error` event in a stream. |
| PRX-PROTO-027 | The proxy must slow the read from the node when the harness reads slowly. |
| PRX-PROTO-028 | The proxy must close the node request when the harness closes the connection. |
| PRX-PROTO-047 | The proxy must end a Responses stream with an abrupt close when the node fails mid-stream. |
| PRX-PROTO-048 | The proxy must not originate an OpenAI-style error event after the response head. The SDKs, pi on the OpenAI path and DeepSeek Harness then fail with no retry. |

Decision note for PRX-PROTO-025: the owner approved the `overloaded_error` event as the default on 2026-10-07 (DEC-058). An abrupt close on the Messages path is simpler and needs no message text. Spike D showed that Claude Code retries after an abrupt close (PROVEN). The default stays as decided. The owner can change it with no loss of retry.

## 6. Anthropic 529 and `overloaded_error`

Anthropic uses HTTP status 529 for overload. The body names the error type `overloaded_error`. Before the first byte the response is a normal JSON error. After the first byte the same error arrives as an SSE `error` event. Both shapes are in section 5.

| Fact | Detail | Label |
| --- | --- | --- |
| Status 529 alone does not trigger a pi retry | The retry test of pi is a pattern on the error text. The pattern has 520 and 524 and not 529. | PROVEN, s5 status table |
| The word "overloaded" does trigger a pi retry | The body of a 529 with `overloaded_error` matches. | PROVEN for the prototype, round 1 |
| Claude Code retries a 529 `overloaded_error` and an `overloaded_error` event | Seen in round 1. Spike D: Claude Code retried 529 after a hold of 250 s | PROVEN (round 1, spike D) |
| Every harness tested retries a 529 | Spike D, with the Messages body type `overloaded_error` and the OpenAI body type `server_error`. pi retried it on both paths | PROVEN (spike D) |
| Shape of the fields | From the Anthropic API errors page. Not re-checked on 2026-10-07. | ASSUMPTION |

| ID | Requirement |
| --- | --- |
| PRX-PROTO-029 | The proxy must use status 529 with an `overloaded_error` body on the Messages path when it needs the harness to retry because of load. |
| PRX-PROTO-030 | The proxy must use the text "overloaded" in the body of every Messages error that must cause a retry in pi. |

## 7. Status codes for the proxy's own refusals

Spike s5 measured pi 1.0.3 on both paths. Pi decides on a retry from a pattern on the text `<status> <body>`. The status code alone does not decide. Spike D repeated the test on every harness with a refusal after a hold of 3 s and after a hold of 250 s.

| Status | Retried by (spike D) | Not retried by | Advice |
|---|---|---|---|
| 503, 502, 504, 500, 529 | Every harness tested | None | Use 503 when a retry helps. |
| 429 | Every harness except Codex | Codex fails at once with `exceeded retry limit` | Avoid. |
| 408 | Every harness except one | DeepSeek Harness on the OpenAI path | Avoid. |
| 409 | SDKs, Claude Code, Codex, opencode, pi on the OpenAI path | pi on the Messages path, DeepSeek Harness | Do not use for a refusal that must stop. |
| 400 | pi and opencode on the OpenAI path, when the body text matches their pattern | Every other harness | Use for a refusal that must stop, with a body that has no pattern word. |
| 401, 404, 422 | NOT TESTED in spike D | pi (s5) | Use 401 and 404 for a refusal that must stop. |
| Connection error | Every harness | curl and `fetch` | Not a status. The node or proxy is down. |

The pi retry has three retries at 2, 4 and 8 s. The window is about 14 s. pi ignores the header `Retry-After`, also with the value 120 (PROVEN, s5 cases 2b and 2c, and spike D).

The OpenAI bodies of spike D had the type `server_error`. pi retried them on every status, also 400 and 409. The Messages bodies followed the status. pi did not retry the Messages 400 and 409 (type `invalid_request_error`). It retried the Messages 408 (`timeout_error`) and 429 (`rate_limit_error`). The words match its pattern. opencode decides in the same way.

Trap: a body with the word "503", "server_error", "overloaded", "timeout", "rate limit" or `terminated` can cause a pi or opencode retry. This holds for a 400 or a 409 (PROVEN, s5 and spike D). The proxy must keep these words out of refusals that the harness must not retry.

The Python SDKs, anthropic-node, Codex and openai-node (below about 60 s) read `Retry-After`. Claude Code reads it up to 60 s and fails at once at 90 s or more. pi, DeepSeek Harness and opencode ignore it. The table is in [04](04-admission-control-and-queueing.md), section 8.3. The proxy sends no `Retry-After` or a value of 30 s or less.

| Refusal | Status | Reason |
|---|---|---|
| Unknown alias | 404 | Stops at once in pi. The harness has a wrong model name. |
| Invalid body or no `model` | 400 | Stops at once. |
| Prompt larger than the loaded context | 400, body with the word "context" | Stops at once. This is a request problem. |
| Node unavailable and no other node | 503 | pi retries for 14 s. A node that comes back in time helps. |
| Hold limit reached | 503, no `Retry-After` or 30 s or less | PROVEN as retried by every harness (spike D). See [04](04-admission-control-and-queueing.md) and DEC-064. |
| Client token missing or wrong, when the registry defines a hosted node | 401 | Stops at once. See PRX-SEC-013 in [07](07-registry-and-configuration.md). |

| ID | Requirement |
| --- | --- |
| PRX-PROTO-031 | The proxy must use only status codes 400, 401 and 404 for refusals that must stop at once. |
| PRX-PROTO-032 | The proxy must keep these words out of the body of a refusal that must stop at once. The words are "503", `server_error`, "timeout", "rate limit", "overloaded" and `terminated`. |
| PRX-PROTO-033 | The proxy must not rely on the `Retry-After` header to control a harness retry. |
| PRX-PROTO-049 | The proxy must send no `Retry-After` header, or a value of 30 s or less, in a refusal that wants a retry. |
| PRX-PROTO-050 | The proxy must not use the status 429 or 408 for a refusal at the hold limit. |
| PRX-PROTO-051 | The proxy must keep the words of PRX-PROTO-032 out of a refusal body on status 400 and 409 as well. |
| PRX-PROTO-034 | The proxy must write each refusal body in the error shape of the protocol of the request. |
| PRX-PROTO-035 | The proxy must put a stable machine-readable error code in each refusal body. |

## 8. Per-harness compatibility matrix

Each row is one harness. Cells are short. Details follow the matrices. Spike C and spike D ran the real binaries of the versions in the introduction. They used fake servers and no real model.

### 8.1 Identity and keys

| Harness | Protocol | Session identity (PROVEN, spike C) | Affinity key source | Children and side requests | Evidence |
|---|---|---|---|---|---|
| pi 1.0.3 | Chat or Messages | `x-session-affinity`, only with `compat.sendSessionAffinityHeaders: true` | The header | Each pi-subagents 0.76.0 child sends its own header. The compaction call has no header | PROVEN (s1b, spike C) |
| Claude Code 2.1.291 | Messages only | `x-claude-code-session-id` and `x-claude-code-agent-id`. A nested child also sends `x-claude-code-parent-agent-id` | The session id and the agent id | A child keeps the session id and has its own agent id. The TUI title call shares the session header | PROVEN (round 1, spike C) |
| Open WebUI | Chat | `X-OpenWebUI-Chat-Id` when the forward setting is on | The header, else the hash | Sub-agents use the parent model, up to 20 in parallel. Task model for titles | ASSUMPTION (docs read, not run) |
| opencode 1.18.35 | Chat and Messages | `x-session-affinity` and `x-session-id`, both equal to `ses_...` | The header | A child has its own `ses_` id. One title call at session start, with headers | PROVEN (spike C, spike D) |
| DeepSeek Harness (dsh) 0.2.0-rc.2 | Chat and Messages | None | The hash | A child differs by its first message. One title call at session start, with no header | PROVEN (spike C, spike D) |
| Codex 0.160.1 | Responses | `session-id`, `thread-id`, `x-codex-window-id` (`<id>:0`) and body `prompt_cache_key`, equal to the session id | The header `session-id`, then `thread-id`, then `prompt_cache_key`, then the derived key. Never `x-codex-window-id` | The first `input` item is an environment block. Subagents and compaction NOT TESTED | PROVEN for plain, two-session and resume runs against a fake server (spike C). R7 for the source |
| Node SDKs: openai-node 7.30.0, anthropic-node 0.131.0 | Chat or Messages | None | The hash, or a configured header | None | PROVEN (spike C, spike D) |
| Python SDKs: openai-python 3.26.0, anthropic-python 1.11.0 | Chat or Messages | None | The hash, or a configured header | None | PROVEN (spike C, spike D) |

### 8.2 Hold, retry and errors (PROVEN, spike D)

| Harness | Times out with no byte | Attempts and patience when all fail | `Retry-After` | Status and body quirks |
|---|---|---|---|---|
| pi 1.0.3 | 299 s | 4 attempts, about 27 to 31 s (gaps 2, 4, 8 s) | Ignored | Retry depends on the error text. A hold of 250 s completes |
| Claude Code 2.1.291 | 360 s | 11 attempts, about 212 s (gaps 0.6 to 37 s) | Honoured to 60 s. Fails at once at 90 s or more | `API_TIMEOUT_MS` can shorten the wait and cannot raise it. `api_error` mid-stream causes a non-stream fallback |
| Open WebUI | NOT TESTED | NOT TESTED | NOT TESTED | NOT TESTED |
| opencode 1.18.35 | 300 s | 9 attempts, about 90 to 108 s | Ignored | Retry depends on the error text, also on 400 and 409 with an OpenAI body |
| DeepSeek Harness 0.2.0-rc.2 | 299 s. A keep-alive does not extend it | 7 attempts, about 35 to 46 s | Ignored | 408 on the OpenAI path is not retried |
| Codex 0.160.1 | Never (tested to 600 s) | 30 requests, about 117 to 121 s | Honoured on 503 and 529 | 429 fails at once. It needs a real SSE event, not a comment line, to keep alive |
| Node SDKs | 301 s (undici overrides the 600 s of the SDK) | 3 attempts, about 11 to 20 s | anthropic-node: honoured. openai-node: only below about 60 s | Retry 408, 409, 429, 500, 502, 503, 504, 529. No retry on 400 |
| Python SDKs | 600 s | 3 attempts, about 11 to 20 s | Honoured | Same statuses as the Node SDKs |

### pi 1.0.3

- Pi sends `x-session-affinity` on both paths only when the provider has `compat.sendSessionAffinityHeaders: true` and `sessionAffinityFormat: "openai-nosession"`. Without the flag the Messages path sends no affinity header ([s1b](evidence/spikes/s1b-pi-lease-gaps-README.md), item 2a', PROVEN).
- Compaction and summary calls carry no session header (s1b, item 1d, PROVEN, same in spike C). Their key is the hash of their own first messages. They are a different conversation (PRX-KEY rules in [03](03-affinity-and-keys.md)).
- Pi retries at the agent level. The provider SDK retry is 0 (s5, PROVEN).
- The next request does not include a failed attempt. Pi discards the partial text (s5, case 3b, PROVEN). The retry body is byte-identical to the first body (spike C, PROVEN).
- A held request of 250 s completes at the first attempt. A request with no byte times out at 299 s (spike D, PROVEN). Earlier: holds of 5, 20, 60 and 120 s and a hang that ends at 300.6 s (s5, case 4).
- Pi sends `x-client-request-id` with the session id as value. It is not a per-request id (spike C, PROVEN).
- Pi sends `max_completion_tokens: 1` for some small windows (Apple node note, spike decisions section 3.7, PROVEN there).
- Versions: pi 1.0.3 is the target. npm latest was 1.0.4 on 2026-10-06.

### Claude Code

- Claude Code speaks Messages only. It completed a real tool loop through the prototype on llama-server, with 3 turns in 452 s on an isolated setup (PROVEN, round 1).
- It sends `x-claude-code-session-id` and `x-claude-code-agent-id`. A child has a distinct agent id and keeps the session id. A child of a child also sends `x-claude-code-parent-agent-id` (PROVEN, spike C). The key uses the session id and the agent id (PRX-KEY-009).
- The first block of its `system` field starts with `x-anthropic-billing-header:`. The proxy drops it for the key (PRX-KEY-037).
- It accepts any model string. It sends about 11 beta headers. It sends `system` messages in the middle of a conversation. It tolerates unknown fields (PROVEN, round 1).
- Its prompt is about 15 000 tokens. Prefill dominates the latency on the test machine (PROVEN, round 1). A moved conversation is expensive.
- It times out after 360 s with no byte, then retries (PROVEN, spike D). `API_TIMEOUT_MS=100000` shortened the wait to 100 s. `API_TIMEOUT_MS=1800000` did not raise it. The stream timeout settings had no visible effect. The cause is not understood. NOT TESTED further.
- It retried a dropped connection and a 503 with `Retry-After` of 7, 15, 30 and 60 s. It failed at once at 90 s and 120 s (spike D, PROVEN).
- Its TUI sends a title call for each user prompt with the session header. In real use the request goes to a small-model alias. It needs its own alias.
- NOT TESTED: `count_tokens` on a real Claude Code path and interactive subagents.

### Open WebUI

- Documentation says it forwards the chat id as `X-OpenWebUI-Chat-Id` and the user fields when the operator sets `ENABLE_FORWARD_USER_INFO_HEADERS`. A pull request adds the chat header. The notes come from the documentation and not from a run ([Open WebUI notes](../../docs/horizon/openwebui.md), ASSUMPTION).
- A sub-agent has its own conversation and so its own chat id (ASSUMPTION). A sub-agent uses the model of its parent. Up to 20 sub-agents run at the same time by default. The load on one pool can be high (see [04](04-admission-control-and-queueing.md)).
- A task model makes small calls for titles and tags. It needs its own alias.
- Not checked: custom headers per connection, paths of embeddings and audio requests, harness timeouts, and how the harness shows a refusal.

### opencode

- opencode 1.18.35 ran on the chat path and on the Messages path (PROVEN, spike D). It sends `x-session-affinity` and `x-session-id` with the same `ses_` value (PROVEN, spike C).
- A child has its own `ses_` id. It sends one title or small-model call at session start with the session headers. Its compaction is a side call with its own system prompt and the session headers (spike C).
- It times out at 300 s with no byte. It makes 9 attempts in about 90 to 108 s. It ignores `Retry-After` and decides on the error text, also on 400 and 409 with an OpenAI body (spike D).
- An early head with pings helps opencode. It waited to 400 s. The proxy does not use this (DEC-064).
- The block hook can fail to fire inside subagent conversations (spike decisions section 3.6). That fact is about guards and does not affect a proxy.

### DeepSeek Harness

- Version 0.2.0-rc.2, a developer preview. It sends no session header on its chat and Anthropic routes (PROVEN, round 1 and spike C). The key is the hash.
- It sends two requests at session start: a title call and the main request. The proxy holds both (spike D). A child differs from its parent by the first message (spike C).
- It times out at 299 s with no byte. A keep-alive does not extend this. It makes 7 attempts in about 35 to 46 s and ignores `Retry-After`. It does not retry 408 on the OpenAI path (spike D).
- After a 200 and an error event it fails with the message and no retry (spike D).
- Children share one process, so one process can carry many conversations. A process-level identity is not usable as a key.

### Codex

- Codex 0.160.1 speaks the Responses API. The owner decided that v1 supports Codex (DEC-062). Spike C ran plain, two-session and resume cases. It did not run compaction, subagents, retries and long loops. Spike D ran the hold and error cases.
- It sends `session-id`, `thread-id`, `x-codex-window-id` and the body field `prompt_cache_key` (PROVEN, spike C). The first `input` item is an environment block (`<environment_context>`). Its system text is 19.6 KB and the working directory sits at character 17198 (PROVEN, spike C).
- It never timed out during a hold with no byte, tested to 600 s. With a 200 head and no event it timed out at 300 s. A comment line did not reset the timer. An SSE event `ping` did (PROVEN, spike D).
- It retries 503, 502, 504, 500, 529 and 408. It fails at once on 429. It honours `Retry-After` on 503 and 529. When every attempt fails it makes 30 requests in about 117 to 121 s (PROVEN, spike D).
- Engines that serve the Responses API are in [05](05-engine-behaviour.md) section 14 (R7, read from source). No engine ran with a live Codex conversation. The proxy passes `previous_response_id` and `store` unchanged (PRX-PROTO-046). Codex 0.160.1 sends `store: false` and no `previous_response_id` on a custom provider (section 1.1).
- Codex retries a stream that closes before `response.completed`, up to 5 times. Codex never retries a 429. It retries a 503 with `Retry-After` and honours the value (R7, source and spike D).

### SDK clients

- The official SDKs retry by default. Node SDKs time out at 301 s because undici overrides the SDK timeout of 600 s. They then retry twice. Python SDKs time out at 600 s (PROVEN, spike D).
- The SDKs retry 408, 409, 429, 500, 502, 503, 504 and 529. They do not retry 400. They send `x-stainless-retry-count`, which changes on a retry and is not part of any key (PROVEN, spikes C and D).
- anthropic-python, openai-python and anthropic-node honour `Retry-After`. openai-node ignores values of about 60 s or more (PROVEN, spike D).
- These harnesses send no session identity. The key is the hash. The configured key map can name a header that the operator sets in the harness.
- The Anthropic SDK that pi bundles (version 0.129.0) has a similar default. Pi sets the provider retry to 0 and retries by itself (s5, PROVEN).

| ID | Requirement |
| --- | --- |
| PRX-PROTO-036 | The proxy must treat a compaction or summary request from pi as a conversation of its own when the request has no session header. |
| PRX-PROTO-037 | The proxy must keep a request that has no session header working, with the hash key. |
| PRX-PROTO-038 | The proxy must pass a session header from any harness to the key extractor without a change in value. |
| PRX-PROTO-039 | The proxy must keep a table of known harness headers in the configuration and not in code (PROPOSED). |
| PRX-PROTO-040 | The proxy must record a harness label in each event, derived from the user agent or the header set. |
| PRX-PROTO-041 | The test suite must include one recorded traffic capture per harness in the matrix before the matrix labels the harness PROVEN. |
| PRX-PROTO-052 | The proxy must keep a Responses harness working with the header `session-id` as the key. |
| PRX-PROTO-053 | The proxy must hold a request of any harness in the matrix for 250 s with no byte. The harness must then complete with no failure. |

## 9. How to settle the open rows

| Harness | Test | Settles |
|---|---|---|
| Open WebUI | Run with forward headers on. Capture 20 parallel sub-agents. | Chat id on the wire. Harness timeout. How the harness shows a refusal. |
| Claude Code | Interactive conversation with many subagents. | Agent ids. The timeout settings that had no effect. |
| Codex | Capture compaction, a subagent, a retry and a long loop. | Key stability. `thread-id` against `session-id`. |
| opencode | Capture a subagent conversation. | Child headers in real use. |
| Real engines | Measure the time until the response head under load. | `t_head` of [04](04-admission-control-and-queueing.md), section 8.2. |
| Responses engines | Run real Codex conversations with tool loops and compaction on llama-server, Ollama, vLLM and gufo (spike F, owner decision pending). R7 gave the source reading. | Which nodes serve Codex. |

The recorded captures feed [13](13-test-fixtures-and-scenarios.md). The risks are in [14](14-open-questions-and-risks.md).

## Sources

- [Spike s5, pi restart](evidence/spikes/s5-pi-restart-README.md): retry timeline, status table, hold tolerance, partial text discard.
- [Spike s1b, pi header gaps](evidence/spikes/s1b-pi-lease-gaps-README.md): affinity header, compaction calls, Messages path.
- [Spike C, key stability](evidence/spikes/sC-key-stability/README.md): headers, children, side requests and retries of each harness.
- [Spike D, hold tolerance](evidence/spikes/sD-hold-tolerance/README.md): timeout times, status matrix, `Retry-After`, errors after a 200.
- [Spike decisions, round 1](../decisions/2026-10-spike-decisions.md): router prototype, Messages path, Claude Code, `dsh`.
- [Open WebUI notes](../../docs/horizon/openwebui.md): read from documentation on 2026-10-06, not run.
- [Research r6, routers](evidence/research/r6-routers-20261007T055357Z.md): program identity headers for Claude Code and Codex.
- [Research r7, Responses API and Codex](evidence/research/r7-responses-api-20261007T081115Z.md): Codex wire format, SSE parser, retries, engine support.
- [Research r5, gateways](evidence/research/r5-gateways-20261007T055410Z.md): OpenRouter key rule and `x-session-id`.
- Anthropic API errors page, `https://docs.anthropic.com/en/api/errors`: 529 `overloaded_error`. Not re-read on 2026-10-07.
