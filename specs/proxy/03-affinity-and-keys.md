# 03 Affinity and keys

Status: draft for review. Language: STE-style (not STE-compliant). Written 2026-10-07.

This file defines the key that names a conversation. It defines the table that holds the node of a conversation. It also defines the use of cache feedback. File [04](04-admission-control-and-queueing.md) defines caps, the protected window and the hold queue. Labels: PROVEN, PROPOSED, ASSUMPTION, NOT TESTED.

The key rules in this file follow [spike C](evidence/spikes/sC-key-stability/README.md) (2026-10-07). Spike C recorded real harness traffic against a fake server. It ran pi 1.0.3, Claude Code 2.1.291, opencode 1.18.35, DeepSeek Harness 0.2.0-rc.2, Codex 0.160.1 and four SDKs. Versions are in [the version list](evidence/spikes/sC-key-stability/VERSIONS-20261007T065702Z.txt). The reference code is [keyv1.py](evidence/spikes/sC-key-stability/keyv1.py). The proxy starts from the rule: header first, body hash as the fallback (DEC-063).

## 1. Purpose

Affinity keeps one conversation on one node. The node keeps the prompt cache of that conversation. The next turn then prefills only the new tokens.

A conversation that moves pays a full prefill. The cost at 60k tokens on Strix Halo is 47 s on gufo and about 230 s on llama.cpp. The source is the owner's notes, read by [R4](evidence/research/r4-local-engines-20261007T022141Z.md) (PROVEN as reported).

Affinity is an optimisation, not a guarantee. The proxy measures whether it pays and switches it off for a node where it does not (section 9).

## 2. Terms used here

| Term | Meaning in this file |
|---|---|
| Key | A 128-bit value that names one conversation inside one alias |
| Source | The place where the proxy reads the key: a header, the body, or the credential |
| Key class | STRONG (from a header), DERIVED (from two message parts), WEAK (from one part or from the credential) |
| Table | The map from key to node, with a last-seen time |
| Seat | One reserved place for a warm conversation on a node. See [04](04-admission-control-and-queueing.md) |
| Side request | A request in the same harness that is not a turn of the main conversation, for example a title, a summary or a compaction call |

## 3. Key sources and header precedence

### 3.1 Default order (PROPOSED)

The proxy reads the sources in this order. The first source that gives a value wins. The proxy never merges two sources.

| Rank | Source | Class | Sent by | Status |
|---|---|---|---|---|
| 1 | `x-session-affinity` | STRONG | pi 1.0.3 with `compat.sendSessionAffinityHeaders: true` and `sessionAffinityFormat: "openai-nosession"`. opencode 1.18.35 always | PROVEN ([s1b](evidence/spikes/s1b-pi-lease-gaps-README.md), row 1c, and [spike C](evidence/spikes/sC-key-stability/README.md)) |
| 2 | `x-session-id` | STRONG | opencode 1.18.35 (same value as `x-session-affinity`). OpenRouter convention. Other harnesses can send it | PROVEN for opencode (spike C). Convention PROVEN as documented ([R3](evidence/research/r3-sglang-routing-20261007T022127Z.md)) |
| 3 | `x-claude-code-session-id` plus `x-claude-code-agent-id` | STRONG | Claude Code 2.1.291 | PROVEN (spike C). A subagent keeps the session id and sends its own agent id |
| 4 | `session-id` | STRONG | Codex 0.160.1. It also sends `thread-id`, `x-codex-window-id` and the body field `prompt_cache_key` | PROVEN (spike C). Whether `thread-id` differs from `session-id` in a Codex subagent is NOT TESTED |
| 5 | `X-OpenWebUI-Chat-Id` | STRONG | Open WebUI with `ENABLE_FORWARD_USER_INFO_HEADERS` | Documented only, NOT TESTED ([Open WebUI notes](../../docs/horizon/openwebui.md)) |
| 6 | Body hash (section 4) | DERIVED or WEAK | Any chat, Messages or Responses request with input | PROVEN stable on every turn in six harnesses (spike C). Same derivation as OpenRouter, with the rules of section 4 |
| 7 | `Authorization` hash | WEAK | Any request with a credential | PROPOSED. Last resort |

Rank 1 comes first because a sender that sets it knows Legatus. A harness that sends two headers gets the lower rank number.

The Responses route has its own default order: `session-id`, then `thread-id`, then the body field `prompt_cache_key`, then the derived key of section 4 (PRX-KEY-055). Codex 0.160.1 sends all three with the same value in the plain, two-session and resume runs of spike C ([R7](evidence/research/r7-responses-api-20261007T081115Z.md), observed against a fake server). The proxy can read the headers without the body.

The proxy must not key on `x-codex-window-id`. Its value is `<session id>:<window number>`. The window number changes at compaction, but the conversation stays the same (R7, inferred, compaction NOT TESTED).

Codex also sends `x-codex-turn-metadata`, `x-codex-parent-thread-id` (a subagent) and `x-codex-turn-state`. The proxy does not use them as keys. A Codex subagent sends its own `thread-id`. Whether its `session-id` differs from the parent value is NOT TESTED. If the value is equal, the order above gives parent and children one key (see section 11).

Pi sends `x-client-request-id`. Its value equals the session id and is not a per-request id (PROVEN, spike C). The proxy must not use it as a request id.

### 3.2 Header value rules

The proxy reads a header name without regard to case. It trims white space at both ends of the value. A value that is empty after trimming counts as absent. If a header occurs more than once, the proxy reads the first line.

The proxy never stores the raw value. It stores a keyed hash (section 5.1).

### 3.3 Claude Code

Claude Code sends one session id for the main agent and for its subagents. It also sends an agent id. The key for a Claude Code request is the hash of the session id and the agent id together. A subagent then gets its own key, so its table entry and its seat differ from the parent.

If the agent id is absent, the key uses the session id alone.

PROVEN (spike C, Claude Code 2.1.291): a child keeps the session id of the parent and adds `x-claude-code-agent-id`, which is distinct for each child. A child of a child also sends `x-claude-code-parent-agent-id`. A background child runs at the same time as the parent. Spike C closed the earlier ASSUMPTION about the agent id.

The proxy does not add the parent agent id to the key. The agent id of each child is already distinct. NOT TESTED: Claude Code interactive subagents with many children.

Claude Code sends a title call for each user prompt in the TUI. The request has no tools, its own system prompt and the same session header. Its key equals the key of the conversation, because the header is the same. A real deployment sends the request to a small-model alias, so the alias scope of section 3.6 separates it.

### 3.4 Per-route key maps

Each route has its own key map. A route is a path pattern, for example `/v1/chat/completions` or `/v1/messages`, with an optional alias. The map is an ordered list of sources. The registry file holds the map (see [07](07-registry-and-configuration.md)).

A source in a map has one of these kinds.

| Kind | Reads | Notes |
|---|---|---|
| `header` | One header | Class STRONG |
| `header_pair` | Two headers joined | Class STRONG. Used for Claude Code |
| `body_field` | One field of the JSON body, for example `session_id` or `prompt_cache_key` | Class STRONG. Off by default |
| `body_hash` | The two message parts of section 4 | Class DERIVED or WEAK |
| `credential` | The `Authorization` header | Class WEAK |
| `none` | Nothing | The request gets no key |

A compact example of a map for a route (PROPOSED shape):

```yaml
routes:
  - path: /v1/messages
    key_map:
      - header_pair: [x-claude-code-session-id, x-claude-code-agent-id]
      - header: x-session-affinity
      - body_hash
  - path: /v1/responses
    key_map:
      - header: session-id
      - header: thread-id
      - body_field: prompt_cache_key
      - body_hash
```

A route without a map uses the default order of section 3.1. A route can add a header name that the default does not know.

### 3.5 Why the credential is weak

All conversations of one user carry the same credential. A key from it puts every conversation of that user on one node. That node reaches its cap while other nodes stay idle. The key also merges the main conversation, its side requests and its subagents.

Two users who share one credential also share the key. Nodes in a small cluster often run with no credential at all, so the source gives nothing.

The proxy therefore uses the credential only when no header and no body hash exists. Section 8 gives the cases. LiteLLM and Olla also place the credential after the session header ([R5](evidence/research/r5-gateways-20261007T055410Z.md), [R6](evidence/research/r6-routers-20261007T055357Z.md)).

### 3.6 Scope of a key

The table key is the pair of the alias name and the 128-bit key. The same session id used with two aliases gives two entries, because each alias has its own pool.

A route can add the credential hash to the scope (`scope: [alias, credential]`). Two users then never share an entry, even when their first messages match. The default scope is the alias alone (PROPOSED).

### 3.7 Headers, children and side requests by harness (PROVEN, spike C)

| Harness | Child agents | Side requests | Retry after 503 |
|---|---|---|---|
| pi 1.0.3 | pi-subagents 0.76.0 children each send their own `x-session-affinity`, their own system prompt and their own first message | The compaction call has no session header and its own system prompt. Pi sends no other side call | Body and header are byte-identical to the first attempt |
| Claude Code 2.1.291 | The session id stays. The agent id differs per child | The title call shares the session header. The compaction call shares the system prompt, the tools, the key and the header of the main thread | Same body and header |
| opencode 1.18.35 | A child has its own `ses_` session id | One title call at session start, with the session headers. Compaction is a side call with its own system prompt and the session headers | Same body and header |
| DeepSeek Harness 0.2.0-rc.2 | No header. A child differs by its first message | One title call at session start, with no header. The compaction call has the system prompt and first message of the main thread | Same body |
| Codex 0.160.1 | NOT TESTED. Spike C ran plain, two-session and resume runs only | NOT TESTED | NOT TESTED |
| SDK clients (OpenAI and Anthropic, Node and Python) | No header | None | Same body. The header `x-stainless-retry-count` changes and is not part of any key |

A retry after a 503 therefore matches the key of its first attempt in every harness tested. The proxy needs no retry rule in the key.

The title call at the start of a DeepSeek Harness or opencode conversation is a second request with its own system prompt. The proxy must not count it as a second conversation. Section 8.5 gives the rule.

## 4. Body hash: exact rules

The rules below are the key version `v1`. They follow `tools/keyv1.py` of [spike C](evidence/spikes/sC-key-stability/keyv1.py). The reference code is a pure function of the path and the body.

### 4.1 Which messages

The proxy reads two parts of the request. The default is the OpenRouter-documented derivation (PROVEN as documented: [R5](evidence/research/r5-gateways-20261007T055410Z.md), OpenRouter prompt caching guide). Spike C proved that this derivation stays byte-stable on every turn (section 4.8).

| Part | OpenAI chat completions | Anthropic Messages | OpenAI Responses |
|---|---|---|---|
| System part | The leading run of messages with role `system` or `developer` | The top-level `system` field, minus the billing block (section 4.7), plus the leading run of `system` or `developer` messages | The field `instructions`, plus the leading run of items in `input` with role `system` or `developer` |
| First part | The first message with any other role, and the wrapper-only messages before it (section 4.7) | The first message in `messages`, and the wrapper-only messages before it | The first item in `input` with another role that is not a tool item, and the wrapper-only items before it. A string `input` is one user message |

The proxy ignores every later message. It also ignores any `system` message that follows the first non-system message. Claude Code sends such messages in the middle of a conversation (PROVEN, spike C). The proxy skips the tool items `function_call` and `function_call_output` of the Responses path.

The system text of a request joins its pieces with one space.

### 4.2 Which fields

For each part the proxy reads the role and the text content. It reads no other field.

| Content shape | Text the proxy reads |
|---|---|
| A string | The string |
| An array of parts | The `text` of each part of type `text`, `input_text` or `output_text`, in order, joined with one space |
| A part of another type (image, audio, file, document) | The marker `[type]`, where `type` is the part type. The proxy ignores the payload |
| A part of type `thinking`, `redacted_thinking`, `reasoning`, `tool_use`, `tool_result`, `function_call` or `function_call_output` | Nothing. The proxy skips the part |
| `null` or absent content | The empty string |

The proxy ignores these fields: `tool_calls`, tool call ids, `name`, `reasoning_content`, `reasoning`, thinking blocks, `cache_control`, `metadata`, `refusal`, `annotations` and every unknown field. It also ignores item ids. Codex sends `msg_<uuid>` ids.

### 4.3 Canonical text

The proxy converts each text with these steps, in this order.

1. Convert the text to Unicode NFC.
2. Replace each run of Unicode white space (including line breaks, tabs and the no-break space) with one space.
3. Remove the spaces at both ends.
4. Cut the text to the byte limit of its part, at a character boundary.

The limits are two parameters (PROPOSED).

| Parameter | Default | Range | Reason |
|---|---|---|---|
| `key_text_limit_system` | 32768 bytes | 256 to 262144 | The working directory sits at character 8663 in the opencode system text and at character 17198 in the Codex system text (PROVEN, spike C). A limit of 8192 drops it. Two projects with one harness then collide |
| `key_text_limit_first` | 8192 bytes | 256 to 65536 | The first user text is short. A small limit costs little |

System text sizes in spike C: 2.7 KB (DeepSeek Harness), 3.1 KB (pi), 6 KB (Claude Code), 9.7 KB (opencode), 19.6 KB (Codex). A text above the limit is cut, so two prompts that differ only after the limit give one key. This is a documented limit (vectors V17 and V17b).

The proxy does not change letter case. It does not remove punctuation.

The steps make the key stable when a harness re-serialises early messages. A harness can move white space, change line endings, split a string into parts, add an id, or add a reasoning field. The key stays the same.

### 4.4 Hash

The proxy computes this value (PROPOSED). The sign US is the byte 0x1F.

```
canonical = "v1" US "S" system_text US "F" role US first_text
key       = first 16 bytes of HMAC-SHA-256(secret, canonical)
```

`role` is the role of the first message that the proxy scanned for the first part. `secret` is a random 32-byte value that the proxy makes at start. The table is soft state and does not survive a restart, so the key does not need to be stable across restarts. A random secret makes the key non-reversible, which protects short prompts (see [08](08-observability-and-admin.md)).

The marker `v1` allows a later change of the rules without a clash. The reference code uses an unkeyed SHA-256 for the test vectors only.

### 4.5 The first 512 bytes are not a key (REMOVED as an option)

Olla hashes the first 512 bytes of the serialised `messages` array (FNV-64a). The hash window is PROVEN as read from source ([R6](evidence/research/r6-routers-20261007T055357Z.md), sticky.go).

Spike C measured the same rule on real traffic. The first 512 bytes were constant across all conversations of pi, Claude Code, DeepSeek Harness and opencode. The system prompt fills them (PROVEN, [spike C, stability table](evidence/spikes/sC-key-stability/README.md)). In pi on the Messages path the value changed from turn 2, because `cache_control` moves. In an SDK conversation under 512 bytes the value changed on every turn.

The proxy must not offer this rule, not even as an option. Section 4.3 gives the system part and the first part separate byte budgets, so the first user text always reaches the hash.

### 4.6 Worked canonicalisation

All rows use the same alias. Rows A, B, C and E give one key. Row D gives another key. Row F gives a third key.

| Row | System text | First user message | Key result |
|---|---|---|---|
| A | `You are a coder.` | `"Fix the bug in a.py"` (string) | K1 |
| B | `You are a coder.` | `"Fix  the bug\r\nin a.py "` (extra spaces, CRLF) | K1 |
| C | `You are a coder.` | `[{"type":"text","text":"Fix the bug in a.py"}]` plus `reasoning_content` and a tool call id | K1 |
| D | `You are a coder.` | `"Fix the bug in b.py"` | K2 |
| E | Messages `system` with a first block `x-anthropic-billing-header: ...` and then `You are a coder.` | `"Fix the bug in a.py"` | K1 |
| F | Responses: `instructions` `You are a coder.` | First item `<environment_context>` of a project, then `"Fix the bug in a.py"` | K3 |

Row F joins the environment item to the first real message (section 4.7). Two Codex conversations in one directory with different prompts then get different keys.

### 4.7 Canonicalisation rules of key version v1

These rules come from `tools/keyv1.py` (PROVEN as running code, 19 vectors pass).

1. Drop each block of the Anthropic `system` array whose text starts with `x-anthropic-billing-header:`. Claude Code puts this block first. Its value depends on the first prompt, so it differs per conversation, per child and after compaction. It was stable inside one conversation. The field `cch=` did not appear in 2.1.291. Newer versions can change the block (RISK).
2. Skip every `system` or `developer` message that follows the first non-system message.
3. Skip the tool items of the Responses path.
4. A wrapper-only message is a message whose text parts all start with one of the tags `<environment_context>`, `<system-reminder>`, `<user_instructions>` and `<skills_instructions>`. The proxy keeps a wrapper-only message and joins its text in front of the next message. It stops at the first message that has text outside the wrapper tags. The tag list is configurable (PROPOSED).
5. Join the text parts of one message with one space. A part of another type becomes `[type]`.
6. Ignore ids, thinking parts, tool items and `cache_control`.
7. Apply NFC, the white space collapse, the trim and the limit of section 4.3 to the system text and to the first text.
8. Build the canonical string of section 4.4 and compute the HMAC.

The Claude Code first user message holds `<system-reminder>` blocks (CLAUDE.md, git status) and then the real prompt. It is not wrapper-only, so its blocks stay in the key. They were stable across turns, resume and git changes in spike C. Removal changed no stability result. Removal also merges conversations of different repositories.

### 4.8 What spike C proved about the body hash

| Fact | Detail | Label |
|---|---|---|
| Stable per conversation | The key from the first system text and the first non-system message was byte-stable on every turn. This holds for pi, Claude Code, opencode, DeepSeek Harness, Codex and the SDKs. Runs had 14 turns, tool loops, reasoning fields, 503 retries and resume | PROVEN (spike C) |
| Different prompts differ | Different first prompts gave different keys in pi, Claude Code, opencode and DeepSeek Harness | PROVEN |
| Breaks at compaction | The key changes when the harness compacts, by design. All four harnesses that compact showed it (section 8.1) | PROVEN |
| Same first prompt | Two conversations with the same first prompt give one key. The session header separates them. DeepSeek Harness and SDK clients send no header, so they collide. This is a known limit | PROVEN |
| Codex | The first input item is an environment block. The old rule gave one key to every Codex conversation in one directory. The wrapper rule fixes it | PROVEN (Codex 0.160.1, plain, two-session and resume runs only) |

The test suite must run the 19 vectors of spike C ([FIX-305](13-test-fixtures-and-scenarios.md)).

## 5. The table

### 5.1 Fields

| Field | Content |
|---|---|
| `alias` | The alias name |
| `key` | The 128-bit key |
| `node` | The node id that serves the conversation |
| `last_seen` | Monotonic time of the last successful request end. Initial value: the placement time |
| `in_flight` | The count of requests of this key that run now |
| `done_count` | The count of successful requests, saturating at 255 |
| `class` | STRONG, DERIVED or WEAK |
| `harness` | A label such as `pi` or `claude-code`, for the admin API only |

The table holds no prompt text, no completion text and no raw session value.

### 5.2 Expiry and cap (PROPOSED)

| Parameter | Default | Range | Source |
|---|---|---|---|
| `table_ttl` | 600 s | 300 to 86400 s | The owner approved 600 s idle on 2026-10-07 (DEC-049). The label PROPOSED stays until a measurement. Anchors: OpenRouter 10 min, Anthropic 5 min or 1 h, OpenAI 30 min, Olla 600 s ([R3](evidence/research/r3-sglang-routing-20261007T022127Z.md), [R6](evidence/research/r6-routers-20261007T055357Z.md)) |
| `table_cap` | 10000 entries | 100 to 1000000 | PROPOSED. Olla uses an LRU cap of 10000 (PROVEN as read from source) |

The expiry is an idle expiry. An entry expires when `last_seen` is older than `table_ttl` and `in_flight` is zero.

The table TTL is longer than the protected window of [04](04-admission-control-and-queueing.md). The window controls the seat. The TTL controls the table entry. A node can still hold the cache of a conversation after its seat ended, because engines evict only when they need memory. A longer entry lets the conversation return to that cache.

When the table is full, the proxy removes the entry with the oldest `last_seen` among the entries with `in_flight` zero.

### 5.3 Refresh rule

The proxy sets `last_seen` only when a request ends with a success status and a complete response. An error does not refresh `last_seen`. This follows OpenRouter, where errors do not refresh the 10 minute timer (PROVEN as documented, [R5](evidence/research/r5-gateways-20261007T055410Z.md)).

A conversation that fails forever therefore expires and no longer holds a table entry or a seat.

### 5.4 Soft state

The table is in memory only. A restart loses it. Traffic rebuilds it. The first turn after a restart goes to a node by the placement rules and pays a cold prefill if the node differs. See [09](09-restart-and-failure.md).

The earlier prototype kept the node of each conversation in a journal. v1 has no journal (see [01](01-decisions.md)).

## 6. Placement, strictness and moves

### 6.1 Placement of a request with a key

1. Find the key in the table.
2. If an entry exists and its node is available, send the request to that node. Admission control of [04](04-admission-control-and-queueing.md) then applies.
3. If no entry exists, place the key as a new conversation by the rules of [04](04-admission-control-and-queueing.md). Write the entry.
4. If an entry exists and its node is unavailable, place the key again and overwrite the entry. This is a move.

A node is unavailable in three cases. It fails its health check. The registry removes it. It cannot serve the request, for example because its context is too small.

### 6.2 Strict stickiness

A conversation moves only when its node is unavailable. A full node is not an unavailable node. The request waits in the hold queue.

The reason is cost. A move pays a full prefill, 47 s to 230 s at 60k tokens. A wait of a few seconds is cheaper. A wait longer than the prefill cost is not cheaper. The default is strict anyway, because the proxy has no good estimate of the wait.

An optional parameter allows a move after a wait (PROPOSED, default off).

| Parameter | Default | Range | Meaning |
|---|---|---|---|
| `spill_after` | off | 0 to `hold_limit` | A held request with a table entry can move to another node with a free seat after this wait |

NOT TESTED: no spike measured the break-even point of spill against wait.

### 6.3 What a move does

On a move the proxy rewrites `node` in the entry and keeps the key. It resets `done_count` to zero, because the new node has no warm state. It writes a `moved` event with the old node, the new node and the reason (see [08](08-observability-and-admin.md)).

### 6.4 Nodes with affinity off

If affinity is off for a node (section 9), the proxy creates no entry that points to that node. A request goes to the least-loaded node by the rules of [04](04-admission-control-and-queueing.md). When affinity turns off, the proxy removes the entries that point to that node.

## 7. Best practice and what other projects do

The table lists what each project documents. The right column gives the consequence for Legatus. Labels follow the research notes: documented (read in docs or source) and inferred.

| Project | What it does | Weakness or lesson | Consequence |
|---|---|---|---|
| OpenRouter | Key is the hash of the first system or developer message and the first non-system message. `session_id` (at most 256 characters) or `x-session-id` overrides. Sticky for 10 minutes idle. Each success resets the timer. Errors do not. Sticky only when cache reads cost less. Uses the next provider on error (documented, [R3](evidence/research/r3-sglang-routing-20261007T022127Z.md), [R5](evidence/research/r5-gateways-20261007T055410Z.md)) | A hosted service. Its nodes have large caches | Copy the derivation, the override order and the refresh rule |
| Olla | Key order: session header (`X-Olla-Session-ID`), prefix hash, auth header, IP. FNV-64a of the first 512 bytes of `messages`. Sliding idle TTL 600 s. LRU cap 10000. Exact table (documented, [R6](evidence/research/r6-routers-20261007T055357Z.md)) | The 512 bytes fill with the system prompt. They were constant across all conversations in four harnesses (PROVEN, spike C). No spill when the node of the entry is busy. No side request detection | Copy the table. Do not copy the 512 byte key. Use separate byte budgets. Add admission |
| LiteLLM | `session_affinity` check keeps a conversation on the deployment of its first request. It reads a session header and the API key hash. Off by default. LiteLLM fixed the TTL at 5 min (issue 28427). PR 40776 matches it to the `cache_control` TTL (documented). R3 names `x-claude-code-session-id` as an example header. R5 reads `x-litellm-session-id` in source. Both notes agree on the TTL bug | A fixed TTL shorter than the cache TTL breaks warm hits. No body derivation | Make the expiry a parameter. Keep it longer than the window |
| SGLang router `cache_aware` | Per-worker radix tree of request text. A match above 0.3 routes to the best worker. Else the worker with the smallest tree. Spill when load differs by more than 64 absolute or 1.5 relative. Eviction every 120 s. Tree cap 67108864. Defaults come from a search summary, so check them (documented in part) | The tree guesses what the worker caches. It gets no feedback about real eviction. A burst goes to one worker (smg issue 2804, 29 to 3). It does not know hybrid state validity | Use an exact table. Add cache feedback from responses. Count load before dispatch |
| SGLang `manual` policy | `X-SMG-Routing-Key` table, up to 2 candidates, remap only when unhealthy (read from source) | Needs SGLang workers | Same shape as the Legatus table with strict moves |
| vLLM production-stack (Python router) | Consistent-hash ring (uhashring) on a header or a body field. No exact table. No idle expiry | A ring changes placement when a node joins. No load bound | Use a table, not a ring |
| vLLM router RFC 219 | Exact map `session_id` to `(worker, last_access_ms)`. Key from `x-session-id` or `x-user-id`, else a prefix match of the conversation history. Hit rate rose from 20.5% to 59.5% in their test (RFC, implementation state unknown) | A single test from the authors | Supports the table design |
| vLLM router program scheduling | Program identity from Claude Code, Codex, OpenCode headers or `x-session-id`. A pool holds requests when a backend lacks capacity (documented, [R6](evidence/research/r6-routers-20261007T055357Z.md)) | Token capacity model for vLLM only | Design reference for [04](04-admission-control-and-queueing.md) |
| llm-d | Approximate scorer: in-memory LRU of prefix hashes, can diverge from the node. Precise scorer: vLLM KV events over ZMQ, token exact. Vendor benchmark on 8 pods with 16 H100 gives these mean times to first token. Precise: 0.30 s. Approximate: 13.3 s. Load only: 47 s. Random: 45 s ([R2](evidence/research/r2-vllm-20261007T022033Z.md)) | Needs events from the engine. Silent about hybrid state | The gap between approximate and precise is the case for cache feedback. Only llama.cpp, mlx-lm and SGLang give response fields (section 9) |
| HAProxy | `stick-table` with `expire` gives an exact table with idle expiry. `hash-balance-factor` bounds load. `hash-preserve-affinity maxconn` skips full servers on the ring. `maxconn` with `timeout queue` gives a hold queue, then 503 (documented, [R5](evidence/research/r5-gateways-20261007T055410Z.md)) | A body key needs the whole body in a 16384 byte buffer by default. JSON key order is not fixed. GPL-2 licence. Cannot log `cached_tokens` | Copy the maxconn plus queue timeout semantics |
| Envoy | `ring_hash` and `maglev` with `hash_balance_factor`: a host takes at most factor/100 of the average load. Typical 120 to 200 (documented, [R3](evidence/research/r3-sglang-routing-20261007T022127Z.md)) | Coarse at five nodes (see below) | Not used. A table with explicit spill is simpler |

### 7.1 Why bounded-load hashing is coarse at five nodes

The load of a host is a small integer. Take 5 nodes of 4 slots with 8 running requests. The average is 1.6. A factor of 1.5 gives a bound of 2.4 requests per host.

Now take 3 running requests. The average is 0.6 and the bound is 0.9. A host with one request is above the bound, so the ring spills the next request. This reading comes from the factor formula (inferred) ([R3](evidence/research/r3-sglang-routing-20261007T022127Z.md)). Nobody checked whether Envoy rounds the bound up.

A ring with 12 conversations also gives lumpy placement.

An exact table with explicit placement avoids both problems.

### 7.2 Hosted cache windows

Hosted caches give the design anchors for the table TTL (documented, [R3](evidence/research/r3-sglang-routing-20261007T022127Z.md)).

| Service | Idle window | Refresh |
|---|---|---|
| OpenRouter sticky routing | 10 min | Each success |
| Anthropic prompt cache | 5 min default, 1 h option | Each hit, at no cost |
| OpenAI prompt cache | 30 min on GPT-5.6 and later | Each reuse |

A local engine has no TTL. It evicts when it needs memory. The idle window of a local conversation is therefore a warm capacity decision, not a time limit. See [04](04-admission-control-and-queueing.md).

## 8. Compaction and collisions

### 8.1 Compaction by the harness

A harness compacts a conversation by replacing early messages with a summary. The first user message then changes. A DERIVED key changes with it, and the table sees a new conversation. This is the right result, because the prompt prefix changed and the old cache no longer matches. The derived key breaks at compaction by design (PROVEN, spike C, all four harnesses that compact).

A STRONG key survives compaction, because the session id does not change. The proxy keeps the entry. The node still holds the cache of the shared system prompt.

| Harness | Compaction request | First message after compaction | Key effect |
|---|---|---|---|
| pi 1.0.3 (automatic) | Side call with no session header and its own system prompt | "The conversation history before this point was compacted..." The session header does not change | The side call gets its own DERIVED key. The main thread keeps its STRONG key |
| Claude Code 2.1.291 (`/compact`) | Same system prompt, tools, key and header as the main thread (prefix fork) | "This session is being continued from a previous conversation..." | STRONG key unchanged. A DERIVED key changes |
| opencode 1.18.35 (automatic) | Side call with its own system prompt and the session headers | "What did we do so far?" | STRONG key unchanged |
| DeepSeek Harness 0.2.0-rc.2 (automatic) | Same system prompt and first message as the main thread | "This is an automatically generated checkpoint..." | No header, so the key changes after compaction. The old cache is useless anyway |

The cost of a new key after compaction is small, because the old cache does not match the new prompt. Spike C forced the compaction thresholds with settings and fake token counts. Real compaction timing is NOT TESTED.

### 8.2 Collision cases

| Case | Key result | Proxy behaviour |
|---|---|---|
| Two conversations with the same first message and no header | One DERIVED key | They share an entry and a node. A sibling spill applies (section 8.3) |
| Subagents given the same task and the same system prompt, no header | One DERIVED key | Same as above. Siblings run at once, so the spill matters |
| Two conversations of one harness with the same first prompt and no header (DeepSeek Harness, SDK clients) | One DERIVED key | Known limit (PROVEN, spike C). They share an entry and a node. A session header separates them in pi, Claude Code, opencode and Codex |
| Two pi-subagents children with the same task | One DERIVED key, two STRONG keys | The header route separates them (PROVEN, spike C) |
| Two Codex conversations in one directory, different prompts | Two DERIVED keys with the wrapper rule. One key without it | The environment item sits first in `input` (PROVEN, spike C). The wrapper rule of section 4.7 joins it |
| Claude Code requests that differ only in the billing block | One DERIVED key | The proxy drops the block (section 4.7) |
| Subagents with a Claude Code agent id | Different STRONG keys | Separate entries and seats |
| A side request with its own system prompt (title, summary) | A new DERIVED key | A separate conversation. It takes a seat or waits ([04](04-admission-control-and-queueing.md)) |
| A side request that shares the system prompt but has a different first message | A new DERIVED key | A separate conversation. The shared system prompt can still hit the node cache only if the node is the same |
| A side request that carries the same session header | The same STRONG key | The proxy treats it as part of the conversation |
| A request with a system part and no other message | WEAK key from the system part alone | Entry created. The window is the probation window (see [04](04-admission-control-and-queueing.md)) |
| A request with no messages and no header (embeddings, audio, images) | No key | No entry. Least-loaded routing |
| First message with only an image | WEAK or DERIVED key from the marker `[image]` and the system part | Images with the same system prompt collide. The proxy accepts this |
| First message that is an assistant message with tool calls and no text | Empty first text | The key is WEAK and uses the system part and the role |
| A harness that edits the first message each turn (for example it adds a time stamp) | A new key every turn | No affinity. Cold-turn detection shows it ([08](08-observability-and-admin.md)) |

### 8.3 Sibling spill

Two requests with the same DERIVED key that run at the same time are not turns of one conversation. A real conversation sends its next turn after the previous response. The siblings share one entry, so both go to one node.

Take a request with a DERIVED key. The node of its entry is at the cap. Another request of that key is in flight. The proxy can then send the new request to another node.

This is a sibling spill. The proxy does not change the entry. The request counts as a cold request.

A STRONG key gets no sibling spill. The harness says it is one conversation, so the request waits.

A harness retry after an abort can look like a sibling. The proxy removes the aborted request from `in_flight` at once, so the retry is not a sibling.

NOT TESTED: no recording shows how often pi, Claude Code or Open WebUI send concurrent requests with one DERIVED key.

### 8.4 Multimodal content

The key ignores image and audio payloads. A harness can resize an image or encode it again between turns. The key stays stable. The cost is a collision between conversations with the same system prompt and an image as first message. Both cases go to one node, which is a loss of balance but not of correctness.

### 8.5 Title and small-model calls at the start of a conversation

DeepSeek Harness and opencode send a title or small-model call at session start. Claude Code in the TUI sends a title call for each user prompt. These calls have their own system prompt and no tools (PROVEN, spike C).

An opencode or Claude Code title call carries the session header, so it has the key of the conversation. A DeepSeek Harness title call has no header, so it gets its own DERIVED key. It looks like a second conversation.

The proxy must not count a key as a conversation until the key has two successful requests (PROPOSED). A title call has one request, so it never counts. It takes a seat under the probation window of [04](04-admission-control-and-queueing.md), which is short. The count feeds the admin view and the cold-turn statistics of [08](08-observability-and-admin.md). The rule changes no routing.

## 9. Cache-hit feedback

### 9.1 What the proxy reads

The proxy reads cache reuse from each successful response. It does not read prompt text.

| Engine | Field | Reuse ratio | Status |
|---|---|---|---|
| llama.cpp `llama-server` and gufo | `timings.cache_n` and `timings.prompt_n` | `cache_n / (cache_n + prompt_n)` | PROVEN as read from source ([spike notes](evidence/spikes/notes-llamacpp-20261007T054753Z.md)). Do not use `tokens_cached`: it counts the prompt plus the generated tokens |
| mlx-lm | `usage.prompt_tokens_details.cached_tokens` | `cached_tokens / prompt_tokens` | PROVEN as read from source ([spike notes](evidence/spikes/notes-mlx-20261007T054857Z.md)) |
| SGLang | `usage.prompt_tokens_details.cached_tokens` | Same | Documented in the brief. Not read in these notes |
| vLLM | `usage.prompt_tokens_details.cached_tokens` | Same | Only with `--enable-prompt-tokens-details` |
| Ollama | None found | None | NOT TESTED. The research notes name no field |
| Responses API nodes | `usage.input_tokens_details.cached_tokens` in `response.completed` | `cached_tokens / input_tokens` | PROVEN as read from source for llama-server and vLLM. SGLang only with `enable_prompt_tokens_details`. Ollama maps it from `PromptEvalCachedCount` and real values are NOT TESTED. The field is optional in Codex (R7, [05](05-engine-behaviour.md) section 14) |
| Hosted nodes | Provider usage fields | Not used | Affinity does not apply |

If a field is absent, the node has no feedback. The proxy keeps the configured affinity mode and writes one warning event per node.

### 9.2 Which turns count

A turn counts if all of these are true.

- The request has a table entry with `done_count` of 1 or more before it.
- The request ran on the same node as the previous turn.
- The prompt has at least `feedback_min_tokens` tokens (PROPOSED 2048).
- The request ended with success.

A first turn can not hit the cache, so it does not count.

### 9.3 Switch off and on (PROPOSED)

The proxy keeps a window of the last `feedback_window` counted turns per node. It computes the token-weighted reuse: the sum of reused tokens divided by the sum of prompt tokens.

| Parameter | Default | Range | Meaning |
|---|---|---|---|
| `feedback_window` | 20 turns | 5 to 200 | Counted turns per node |
| `feedback_min_turns` | 10 turns | 3 to `feedback_window` | Turns needed before a decision |
| `reuse_floor` | 0.20 | 0.05 to 0.9 | Affinity turns off below this reuse |
| `reprobe_after` | 600 s | 60 to 86400 s | Wait before affinity turns on again to test |

All values have the label PROPOSED. Spike 7 measured the cost of edits on llama-server (section 9.6). It did not measure a hit rate of real harness traffic. A trace replay sets the values.

When affinity is off, the node gets plain least-loaded routing and no protected window. After `reprobe_after` the proxy turns affinity on again until it has `feedback_min_turns` new counted turns. If reuse is still below the floor, affinity goes off again.

An operator can force a node to `on` or `off` in the registry. A forced node skips the measurement.

### 9.4 What the feedback catches

- vLLM issue 45238: a hybrid model can report zero hits with no error ([R2](evidence/research/r2-vllm-20261007T022033Z.md)). The ratio falls to about zero and affinity turns off.
- Bonsai-demo issue 147: `cached_tokens` stayed 0 when a multimodal projector was loaded ([R4](evidence/research/r4-local-engines-20261007T022141Z.md), reported, cause unproven).
- A hybrid model on llama.cpp that pays a cold prefill after each turn (reported in R4). Spike 7 reproduced the cause on Qwen3.5-2B. An early edit causes a cold prefill of the whole prompt (section 9.6).
- A prompt that changes between turns, for example a harness that rewrites the system prompt.

The proxy also feeds the ratio to cold-turn detection in [08](08-observability-and-admin.md).

### 9.5 Limits

Reuse can be low because the cache is full and not because affinity fails. A node with more conversations than seats evicts. The ratio then falls and the proxy turns affinity off, which makes the eviction worse. Section 11 lists this as a risk. The number of seats is `warm_capacity` ([04](04-admission-control-and-queueing.md) section 4.5). It can exceed the slot count on llama-server with `--cache-ram`.

### 9.6 Cost of a miss on a hybrid node (spike 7)

[Spike 7](evidence/spikes/s7-hybrid-cache/README.md) gives the cost rule for the feedback and the move rules. It used llama-server b11459 and brew 0.5.0 on Qwen3.5-2B Q4_K_M. The rule is PROVEN for this model. It is an ASSUMPTION for Qwen 3.8 Flash-Next and 27B. Speeds are NOT TRANSFERABLE.

- A hybrid node recomputes the whole prompt for an edit more than 516 tokens before the end. A user-message checkpoint before the edit is the only exception. The recompute is `N` minus the largest checkpoint at or below the edit. [05](05-engine-behaviour.md) section 4.5 has the tables.
- The cost is a cliff. At N = 6000 a one-token shift changed the recompute from 6000 tokens (15.0 s) to 516 tokens (1.3 s). A dense model degrades linearly.
- A move of a conversation to another node always costs the full prompt on a hybrid node. The same holds for compaction, trimmed tool results and a changed system prompt.
- The token share is a poor guide to the cost. A turn that reuses 5484 of 6000 tokens has a ratio of 0.91 and costs 1.3 s. A turn that reuses 133 of 6075 tokens has a ratio of 0.02 and costs about 22 s. The feedback window therefore also counts the turns whose `timings.prompt_n` is far above the expected new tokens (PRX-AFF-033).
- With `--cache-ram` above 0, a second conversation on one slot stays warm (201 tokens recomputed per turn). With `--cache-ram 0` every turn is cold.

## 10. Requirements

### 10.1 Keys

| ID | Requirement |
|---|---|
| PRX-KEY-001 | The proxy must read the key sources of a route in the order of its key map. |
| PRX-KEY-002 | The proxy must use the default order of section 3.1 when a route has no key map. |
| PRX-KEY-003 | The proxy must use the first source that gives a non-empty value. |
| PRX-KEY-004 | The proxy must not merge two sources into one key, except the pair kinds `header_pair`. |
| PRX-KEY-005 | The proxy must match header names without regard to case. |
| PRX-KEY-006 | The proxy must trim white space at both ends of a header value. |
| PRX-KEY-007 | The proxy must treat a header value that is empty after trimming as absent. |
| PRX-KEY-008 | The proxy must read the first line when a header occurs more than once. |
| PRX-KEY-009 | The proxy must make the Claude Code key from the session id and the agent id together. |
| PRX-KEY-010 | The proxy must make the Claude Code key from the session id alone when the agent id is absent. |
| PRX-KEY-011 | The proxy must read the body hash parts as section 4.1 defines for OpenAI chat completions and for Anthropic Messages. |
| PRX-KEY-012 | The proxy must treat the roles `system` and `developer` as the system part. |
| PRX-KEY-013 | The proxy must ignore every message after the first non-system message when it derives a key. |
| PRX-KEY-014 | The proxy must read only the role and the text content of a part. |
| PRX-KEY-015 | The proxy must ignore tool call ids, `name`, reasoning fields, thinking blocks, `cache_control`, `metadata` and unknown fields when it derives a key. |
| PRX-KEY-016 | The proxy must replace a non-text content part by the marker `[type]` when it derives a key. |
| PRX-KEY-017 | The proxy must canonicalise text in the order: NFC, white space collapse, trim, cut to the limit of the part. |
| PRX-KEY-018 | The proxy must cut canonical text at a character boundary. |
| PRX-KEY-019 | The proxy must give the system part and the first part separate byte budgets. |
| PRX-KEY-020 | The proxy must make the key as the first 16 bytes of HMAC-SHA-256 with a random secret that it makes at start. |
| PRX-KEY-021 | The proxy must give two requests the same key when they differ only in white space or line endings. The proxy must also do so when they differ only in content shape, tool call ids or reasoning fields. |
| PRX-KEY-022 | The proxy must give two requests different keys when their first user texts differ after canonicalisation. |
| PRX-KEY-023 | The proxy must give a request no key when it has no header key, no messages and no credential. |
| PRX-KEY-024 | The proxy must mark a key from a header as STRONG. |
| PRX-KEY-025 | The proxy must mark a key from two message parts as DERIVED. |
| PRX-KEY-026 | The proxy must mark a key from one message part or from the credential as WEAK. |
| PRX-KEY-027 | The proxy must use the credential source only when no header and no body hash gives a key. |
| PRX-KEY-028 | The proxy must include the alias name in the table key. |
| PRX-KEY-029 | The proxy must include the credential hash in the table key when the route sets `scope: [alias, credential]`. |
| PRX-KEY-030 | The proxy must not store a raw session value, a header value or message text in the table. |
| PRX-KEY-031 | The proxy must load `key_text_limit_system` from the registry with the default 32768 bytes and `key_text_limit_first` with the default 8192 bytes. |
| PRX-KEY-032 | The key of every turn of a recorded conversation must equal the key of its first turn. If it does not, the test must report the harness as unstable. The recordings cover pi, Claude Code, opencode, DeepSeek Harness, Codex, the SDKs and Open WebUI. |
| PRX-KEY-033 | The proxy must reject a registry key map that names an unknown source kind. |
| PRX-KEY-034 | The proxy must leave the request body unchanged when it reads a key. |
| PRX-KEY-035 | The proxy must read the body hash parts of a `/v1/responses` request from the field `instructions` and the field `input` as section 4.1 defines. |
| PRX-KEY-036 | The proxy must treat a string `input` of a Responses request as one user message. |
| PRX-KEY-037 | The proxy must drop each block of the Anthropic `system` array whose text starts with `x-anthropic-billing-header:` when it derives a key. |
| PRX-KEY-038 | The proxy must keep a wrapper-only message and join its text in front of the next message. The proxy must stop at the first message that has text outside the wrapper tags. |
| PRX-KEY-039 | The proxy must treat a message as wrapper-only when all its text parts start with a wrapper tag. The tags are `<environment_context>`, `<system-reminder>`, `<user_instructions>` and `<skills_instructions>`. |
| PRX-KEY-040 | The proxy must read the parts of type `text`, `input_text` and `output_text` as text and must join the text parts of one message with one space. |
| PRX-KEY-041 | The proxy must skip the parts of type `thinking`, `redacted_thinking`, `reasoning`, `tool_use`, `tool_result`, `function_call` and `function_call_output` when it derives a key. |
| PRX-KEY-042 | The proxy must skip the tool items of a Responses request when it looks for the first part. |
| PRX-KEY-043 | The proxy must use the role of the first scanned message as the role field of the canonical string. |
| PRX-KEY-044 | The proxy must build the canonical string as the form in section 4.4. The form is `v1`, US, `S` and the system text, US, `F` and the role, US, and the first text. |
| PRX-KEY-045 | The proxy must not offer a key from the first 512 bytes of the request, not even as a configuration option. |
| PRX-KEY-046 | The proxy must accept a system text limit of 262144 bytes and must refuse a limit below 256 bytes. |
| PRX-KEY-047 | The proxy must read the headers `session-id` (rank 4 of section 3.1) and `x-session-id`, and must allow `thread-id` and the body field `prompt_cache_key` in a key map. |
| PRX-KEY-048 | The proxy must not add `x-claude-code-parent-agent-id` to the Claude Code key. |
| PRX-KEY-049 | The proxy must not use `x-client-request-id` as a request id or as a key source by default. |
| PRX-KEY-050 | The proxy must give a retried request the key of its first attempt, and must not read `x-stainless-retry-count` for any key. |
| PRX-KEY-051 | The proxy must treat the key of a DERIVED conversation as a new key after compaction. |
| PRX-KEY-052 | The proxy must not count a key as a conversation in the conversation count and the cold-turn statistics until the key has two successful requests. This is DEC-067, pending owner confirmation. |
| PRX-KEY-053 | The proxy must pass the 19 key vectors of spike C when both text limits are 8192 (FIX-305). |
| PRX-KEY-054 | The proxy must load the list of wrapper tags from the registry. |
| PRX-KEY-055 | The proxy must read the key sources of a `/v1/responses` request in this default order. The order is the header `session-id`, the header `thread-id`, the field `prompt_cache_key` and the derived key. |
| PRX-KEY-056 | The proxy must not use the header `x-codex-window-id` as a key source. |
| PRX-KEY-057 | The proxy must read the headers of a Responses request first and must not parse the body when a header gives the key. |
| PRX-KEY-058 | The proxy must tolerate the request header `Content-Encoding: zstd` when it reads the body for a key. When it cannot decode the body, it must use no body key and must pass the body unchanged. |

### 10.2 Affinity

| ID | Requirement |
|---|---|
| PRX-AFF-001 | The proxy must keep a table that maps the pair of alias and key to a node, with a last-seen time. |
| PRX-AFF-002 | The proxy must keep the table in memory only and must start with an empty table. |
| PRX-AFF-003 | The proxy must send a request with a table entry to the node of the entry when that node is available. |
| PRX-AFF-004 | The proxy must create an entry when it places a key for the first time. |
| PRX-AFF-005 | The proxy must set `last_seen` only when a request ends with a success status and a complete response. |
| PRX-AFF-006 | The proxy must not refresh `last_seen` after an error. |
| PRX-AFF-007 | The proxy must remove an entry whose `last_seen` is older than `table_ttl` when `in_flight` is zero. |
| PRX-AFF-008 | The proxy must load `table_ttl` from the registry with the default 600 s. |
| PRX-AFF-009 | The proxy must hold at most `table_cap` entries, default 10000. |
| PRX-AFF-010 | The proxy must remove the entry with the oldest `last_seen` and `in_flight` zero when the table is full. |
| PRX-AFF-011 | The proxy must keep `in_flight` and `done_count` per entry. |
| PRX-AFF-012 | The proxy must move a conversation to another node only when its node is unavailable, except when the registry sets `spill_after`. |
| PRX-AFF-013 | The proxy must treat a full node as available. |
| PRX-AFF-014 | The proxy must write a `moved` event on a move with the old node, the new node and the reason. |
| PRX-AFF-015 | The proxy must reset `done_count` to zero on a move. |
| PRX-AFF-016 | The proxy must create no entry that points to a node with affinity off. |
| PRX-AFF-017 | The proxy must remove the entries that point to a node when its affinity turns off. |
| PRX-AFF-018 | The proxy must send a request with a DERIVED key to another node and must leave the entry unchanged. This applies when the node of the entry is at its cap and another request of that key is in flight. |
| PRX-AFF-019 | The proxy must not apply a sibling spill to a STRONG key. |
| PRX-AFF-020 | The proxy must remove an aborted request from `in_flight` at once. |
| PRX-AFF-021 | The proxy must read cache reuse from the response field of the engine as the table in section 9.1 defines. |
| PRX-AFF-022 | The proxy must not use `tokens_cached` of llama-server as a reuse count. |
| PRX-AFF-023 | The proxy must count a turn for feedback only when all four conditions hold. The entry has `done_count` of 1 or more. The node is the same as before. The prompt has at least `feedback_min_tokens` tokens. The request succeeded. |
| PRX-AFF-024 | The proxy must compute the reuse of a node as reused tokens divided by prompt tokens over the last `feedback_window` counted turns. |
| PRX-AFF-025 | The proxy must switch affinity off for a node when reuse is below `reuse_floor` after at least `feedback_min_turns` counted turns. |
| PRX-AFF-026 | The proxy must route a node with affinity off by least load with no protected window. |
| PRX-AFF-027 | The proxy must switch affinity on again for a test after `reprobe_after` seconds. |
| PRX-AFF-028 | The proxy must keep the affinity mode of a node when the engine gives no cache field, and must write one warning event. |
| PRX-AFF-029 | The proxy must accept a forced `on` or `off` mode per node from the registry. |
| PRX-AFF-030 | The proxy must write an event when the affinity mode of a node changes, with the reuse value and the number of turns. |
| PRX-AFF-031 | The proxy must pass the request byte for byte to the node, with the node patches of [05](05-engine-behaviour.md) only. |
| PRX-AFF-032 | The proxy must estimate the cost of a move or a spill to a node of kind `hybrid` as a full prefill of the prompt. It must treat any edit more than about 516 tokens before the end of the previous prompt as a full prefill. |
| PRX-AFF-033 | The proxy must count a counted turn as cold when `timings.prompt_n` exceeds the expected new tokens by more than the allowance of PRX-ENG-038. The proxy must report the cold count beside the weighted reuse. |
| PRX-AFF-034 | The proxy must set the number of seats of a node from `warm_capacity` and not from the slot count alone ([04](04-admission-control-and-queueing.md) section 4.5). |

## 11. Risks and open points

| Item | Text | Settled by |
|---|---|---|
| Key stability | PROVEN on every turn in pi, Claude Code, opencode, DeepSeek Harness, Codex and the SDKs (spike C). Open WebUI is NOT TESTED. The key breaks at compaction and for same-prompt conversations with no header | An Open WebUI recording, PRX-KEY-032 |
| Agent id meaning | PROVEN (spike C): a Claude Code child keeps the session id and adds a distinct agent id. Interactive subagents with many children are NOT TESTED | A recording of an interactive Claude Code run |
| Billing block | Newer Claude Code versions can add `cch=` or change the block (spike C saw none in 2.1.291) | A recording per Claude Code version |
| Codex scope | Spike C ran Codex plain, two-session and resume only, against a fake server. Compaction, subagents, retries and long loops are NOT TESTED. Engine support for the path is in [05](05-engine-behaviour.md) section 14 (from r7, no live Codex run) | A Codex recording of each case, and spike F (owner decision pending) |
| Codex subagent key | The default order puts `session-id` before `thread-id`. If a subagent shares the parent `session-id`, parent and children get one key. R7 suggests `thread-id`. NOT TESTED | A Codex subagent recording. The owner can set `thread-id` first |
| Same first prompt | Two header-less conversations with one first prompt share a key (known limit) | A trace of real use |
| Open WebUI chat id | Documentation only, NOT TESTED | An Open WebUI run |
| Feedback loop | Low reuse from a full cache turns affinity off and makes eviction worse. Spike 7 showed warm swaps with `--cache-ram` for two conversations. Eviction beyond that is NOT TESTED | A probe with many conversations, spike B |
| Ollama feedback | No cache field found | An Ollama capture |
| Thresholds | `reuse_floor`, `feedback_window` and `table_ttl` carry the label PROPOSED | A trace replay |
| Body field `session_id` | OpenRouter reads it. The proxy leaves the body unchanged, so the engine gets an unknown field. OPEN: check that engines ignore it | An engine test |
| Sibling spill | The rule is a heuristic and has no measurement | A trace with concurrent same-key requests |
| Spill after a wait | The break-even is NOT TESTED | A timed test with a cold prefill |
| Hybrid cache validity | A hit needs a checkpoint at the right depth (PROVEN for Qwen3.5-2B: 516 and 4 tokens before the end). The ratio shows the result, not the cause | Spike B for the large models. [05](05-engine-behaviour.md) |

Items for [14](14-open-questions-and-risks.md): key stability for Open WebUI, Codex coverage, billing block, same first prompt, sibling spill, feedback loop, `session_id` forwarding.

## Sources

- [R3 SGLang and routing notes](evidence/research/r3-sglang-routing-20261007T022127Z.md): OpenRouter, LiteLLM, Envoy, HAProxy, SGLang router, llm-d, vLLM RFC 219. URLs inside the note.
- [R5 gateways notes](evidence/research/r5-gateways-20261007T055410Z.md): LiteLLM source lines, HAProxy configuration lines, OpenRouter guide.
- [R6 routers notes](evidence/research/r6-routers-20261007T055357Z.md): Olla `sticky.go`, SMG `manual.rs`, vLLM router program scheduling.
- [R2 vLLM notes](evidence/research/r2-vllm-20261007T022033Z.md): issue 45238, llm-d benchmark.
- [R4 local engines notes](evidence/research/r4-local-engines-20261007T022141Z.md): prefill cost, Bonsai-demo issue 147.
- [R7 Responses API and Codex](evidence/research/r7-responses-api-20261007T081115Z.md): Codex headers, key order, cache field.
- [S7 hybrid cache](evidence/spikes/s7-hybrid-cache/README.md): checkpoint positions, cost of edits, warm swaps.
- [llama.cpp spike notes](evidence/spikes/notes-llamacpp-20261007T054753Z.md) and [mlx notes](evidence/spikes/notes-mlx-20261007T054857Z.md): response fields.
- [s1b pi lease gaps](evidence/spikes/s1b-pi-lease-gaps-README.md): pi affinity header and compaction.
- [Spike C, key stability](evidence/spikes/sC-key-stability/README.md), its [analysis](evidence/spikes/sC-key-stability/analysis-20261007T065841Z.txt), the [reference code](evidence/spikes/sC-key-stability/keyv1.py) and the [19 test vectors](evidence/spikes/sC-key-stability/test-vectors-keyv1-20261007T065515Z.json).
- [Spike decisions](../decisions/2026-10-spike-decisions.md): Claude Code headers.
- [Open WebUI notes](../../docs/horizon/openwebui.md): chat id header.
- Sibling files: [04](04-admission-control-and-queueing.md), [05](05-engine-behaviour.md), [07](07-registry-and-configuration.md), [08](08-observability-and-admin.md), [09](09-restart-and-failure.md), [13](13-test-fixtures-and-scenarios.md), [14](14-open-questions-and-risks.md), [01](01-decisions.md).
