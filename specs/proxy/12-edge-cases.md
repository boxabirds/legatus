# Edge cases

Status: draft for review, 2026-10-07. Part of the Legatus proxy baseline. This file catalogues the situations that break routers and caches in practice. Each row is a test case.

## Reading guide

Every row has an ID of the form `EDGE-<AREA>-<NNN>`. The columns do not change.

- **Situation:** the input or event, concrete enough to script.
- **Expected proxy behaviour:** one sentence that a test can check.
- **Evidence or reason:** a label followed by a source. The four labels are PROVEN, PROPOSED, ASSUMPTION and NOT TESTED.
- **Fixture:** the scenario ID in [the fixture catalogue](13-test-fixtures-and-scenarios.md). A row can name a stub engine, a harness script, a fault script or a capture. These have the IDs below FIX-100.

PROVEN means measured or read from source. PROPOSED means a design choice that no one measured. ASSUMPTION means believed and not checked. NOT TESTED means no evidence exists.

Where a row says PROPOSED, the test pins the design choice. If the owner changes the choice, the row and the test change together. The open choices are in [open questions and risks](14-open-questions-and-risks.md).

Area codes:

- KEY: keys. AFF: affinity. ADM: admission and hold.
- PRO: protocol. STR: streaming. ENG: engines. CAC: caches.
- RST: restart. HST: hosted nodes. OBS: observability and admin. HAR: harness behaviour.

Default values below come from the decision log in [01-decisions](01-decisions.md). The protected window is 180 s, the hold limit is 250 s and the table expiry is 600 s of idle time. All three are PROPOSED. The owner approved the table expiry on 2026-10-07.

Time in all rows is in whole milliseconds. Spike s2 showed that timer resolution is 1 ms and that offsets below 1 ms do not order events.

### Evidence labels

| Label | File |
|---|---|
| R1 to R7 | [r1 models](evidence/research/r1-models-20261007T022035Z.md), [r2 vLLM](evidence/research/r2-vllm-20261007T022033Z.md), [r3 SGLang and routing](evidence/research/r3-sglang-routing-20261007T022127Z.md), [r4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md), [r5 gateways](evidence/research/r5-gateways-20261007T055410Z.md), [r6 routers](evidence/research/r6-routers-20261007T055357Z.md), [r7 Responses API and Codex](evidence/research/r7-responses-api-20261007T081115Z.md) |
| S1B | [pi header mechanism](evidence/spikes/s1b-pi-lease-gaps-README.md) |
| S2 | [paused time](evidence/spikes/s2-paused-time-README.md) |
| S3 | [slots and concurrency](evidence/spikes/s3-slots-README.md) |
| S4 | [tool-call smoke](evidence/spikes/s4-smoke-README.md) |
| S5 | [pi retry and restart](evidence/spikes/s5-pi-restart-README.md) |
| SC | [spike C, key stability](evidence/spikes/sC-key-stability/README.md) |
| SD | [spike D, hold tolerance](evidence/spikes/sD-hold-tolerance/README.md) |
| S6 | [checkpoint policy](evidence/spikes/s6-checkpoint-source-README.md), with per-engine notes [llama.cpp](evidence/spikes/notes-llamacpp-20261007T054753Z.md), [vLLM](evidence/spikes/notes-vllm-20261007T054827Z.md), [SGLang](evidence/spikes/notes-sglang-20261007T054827Z.md), [gufo](evidence/spikes/notes-gufo-20261007T054857Z.md), [mlx-lm](evidence/spikes/notes-mlx-20261007T054857Z.md) |
| S7 | [spike 7, hybrid cache](evidence/spikes/s7-hybrid-cache/README.md) |
| RND1 | [spike round 1 decisions](../decisions/2026-10-spike-decisions.md) |
| OWUI | [Open WebUI notes](../../docs/horizon/openwebui.md) |
| BRIEF | the owner decisions in [the decision log](01-decisions.md) |

## Keys

The key rules are in [affinity and keys](03-affinity-and-keys.md). These rows test the boundaries of those rules.

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-KEY-001 | A request carries `x-session-affinity: abc` and a body | The proxy must use `abc` as the key and ignore the body for the key. | PROVEN S1B 1c | FIX-100 |
| EDGE-KEY-002 | The header is present with an empty value | The proxy must treat the header as absent and derive the key from the body. | PROPOSED: an empty key groups strangers | FIX-106 |
| EDGE-KEY-003 | The header value is 10 KiB long | The proxy must use a fixed-length digest of the value as the key and must not reject the request. | PROPOSED: OpenRouter limits ids to 256 characters (R3) | FIX-106 |
| EDGE-KEY-004 | The header name uses other case, for example `X-Session-Affinity` | The proxy must give the same key as the lower-case name. | PROVEN: HTTP names ignore case (standard) | FIX-106 |
| EDGE-KEY-005 | The header appears twice with two different values | The proxy must use the first value and must count the event in the log. | PROPOSED | FIX-106 |
| EDGE-KEY-006 | The header value has leading and trailing spaces | The proxy must trim the spaces before it uses the value. | PROPOSED | FIX-106 |
| EDGE-KEY-007 | Both `x-session-affinity` and `x-claude-code-session-id` are present with different values | The proxy must use the first header in the key map order of the route. | PROPOSED | FIX-100 |
| EDGE-KEY-008 | Claude Code subagent: same session id, different agent id | The proxy must give a different key when the route key map joins the session id and the agent id. | PROVEN: both headers exist (RND1 3.6). Join rule PROPOSED | FIX-104 |
| EDGE-KEY-009 | Two conversations with identical first messages and no header, for example both start with "hi" | The proxy must give both the same key, place both on one node and keep the node cap. | PROVEN: derivation rule (R5 OpenRouter). Collision is a known cost | FIX-101 |
| EDGE-KEY-010 | A request with a system message and no user message | The proxy must derive the key from the system message alone and mark the key weak in the log. | PROPOSED | FIX-101 |
| EDGE-KEY-011 | A request with a user message and no system message | The proxy must derive the key from the first non-system message alone. | PROPOSED | FIX-101 |
| EDGE-KEY-012 | A request with an empty `messages` array | The proxy must treat the request as keyless and forward it so the node returns its own error. | PROPOSED | FIX-143 |
| EDGE-KEY-013 | The first message is multimodal: a text part and a 100 KB base64 image part | The proxy must derive one stable key from a bounded part of the content and give the same key on every turn. | PROPOSED: bound set in 03 | FIX-101 |
| EDGE-KEY-014 | Turn 1 sends the first message content as a string. Turn 2 sends the same text as an array with one text part | The proxy must give the same key for both forms. | PROPOSED: the tokens are equal | FIX-101 |
| EDGE-KEY-015 | The first message holds the text with a JSON escape (`é`) on turn 1 and the raw character on turn 2 | The proxy must hash the decoded text so both forms give one key. | PROPOSED | FIX-101 |
| EDGE-KEY-016 | The JSON body lists `tools` before `messages` on one turn and after on the next | The proxy must give the same key because it reads the `messages` field by name. | PROVEN: HAProxy body keys fail here (R5) | FIX-101 |
| EDGE-KEY-017 | A harness reorders the tool definitions between turns | The proxy must keep the key and pass the bytes unchanged, and the log must show the cold turn that follows. | PROVEN: tokens change so the cache breaks (S6) | FIX-159 |
| EDGE-KEY-018 | The system prompt holds a time stamp that changes each turn and there is no header | The proxy must treat each turn as a new conversation and count the key churn per harness. | PROPOSED | FIX-102 |
| EDGE-KEY-019 | A title request has the same first user message as the main conversation and a different system prompt | The proxy must give the title request a different key. | PROVEN: side requests are other conversations (BRIEF) | FIX-103 |
| EDGE-KEY-020 | A compaction call carries the key header of the old conversation | The proxy must route the request to the node of that key and must count the cold turn from the cache fields. | PROPOSED: the header decides, the cache fields judge | FIX-187 |
| EDGE-KEY-021 | The same header value arrives with two different `model` aliases | The proxy must keep two entries, one for each alias. | PROVEN: OpenRouter keys per model (R3). Scope PROPOSED | FIX-107 |
| EDGE-KEY-022 | Two users with different credentials send the same header value, for example `1` | The proxy must give both the same key unless the route key map adds the credential label. | ASSUMPTION: see OPEN-017 | FIX-107 |
| EDGE-KEY-023 | A harness reuses one header value after a new conversation starts | The proxy must keep the node and the cold-turn log must show a cold turn. | PROPOSED | FIX-102 |
| EDGE-KEY-024 | The first message is 2 MB of text | The proxy must hash a bounded prefix and must not copy the message again. | PROPOSED | FIX-144 |
| EDGE-KEY-025 | The route key map names a header that no harness sends | The proxy must fall back to the body derivation and log the fallback once per start. | PROPOSED | FIX-104 |
| EDGE-KEY-026 | The test replays the recorded turns of each harness | The proxy must give one key for all turns of each recorded conversation. | PROVEN for pi, Claude Code, opencode, DeepSeek Harness, Codex and the SDKs (SC). Open WebUI NOT TESTED | FIX-108 |
| EDGE-KEY-027 | Two Codex conversations in one directory send an environment item first and different prompts second | The proxy must give the two conversations different keys. | PROVEN: SC. The old rule collided | FIX-308 |
| EDGE-KEY-028 | Claude Code sends a billing block `x-anthropic-billing-header:` first in `system`, and its value differs per conversation | The proxy must drop the block and must give the same key as without the block. | PROVEN: SC, vector V10 | FIX-309 |
| EDGE-KEY-029 | Two conversations of DeepSeek Harness or of an SDK client have the same first prompt and no header | The proxy must give both the same key. | PROVEN: SC. Known limit. A header separates conversations in pi, Claude Code, opencode and Codex | FIX-310 |
| EDGE-KEY-030 | Two system prompts differ only at byte 17198 | The proxy must give two keys with the default limit of 32768 bytes. | PROVEN: the working directory sits at 17198 in Codex and 8663 in opencode (SC) | FIX-320 |
| EDGE-KEY-031 | A harness compacts a conversation and has no session header | The proxy must give the conversation a new key. | PROVEN: SC, vector V12. By design | FIX-317 |
| EDGE-KEY-032 | A harness retries a request after a 503 | The proxy must give the retry the key of the first attempt. | PROVEN: the body and the header are byte-identical in every harness (SC) | FIX-312 |
| EDGE-KEY-033 | A key map names the first 512 bytes of the body as a source | The proxy must refuse the registry at load with a message that names the source. | PROVEN: the 512 bytes were constant in four harnesses (SC). Refusal PROPOSED | FIX-305 |
| EDGE-KEY-034 | A Claude Code child has the session id of its parent and its own agent id | The proxy must give the child a key different from the parent. | PROVEN: SC | FIX-321 |
| EDGE-KEY-035 | A Claude Code child of a child sends `x-claude-code-parent-agent-id` | The proxy must use the session id and the agent id of the child and must ignore the parent agent id. | PROVEN for the headers (SC). The key rule is PROPOSED | FIX-321 |
| EDGE-KEY-036 | A Responses request has a string `input` and an `instructions` field | The proxy must read the string as one user message and must give the same key as the list form. | PROVEN: SC, Codex vectors. String form PROPOSED | FIX-322 |
| EDGE-KEY-037 | A Codex request has `session-id` and a different body `prompt_cache_key` | The proxy must use the header `session-id`. | PROPOSED. Both had one value in SC | FIX-322 |
| EDGE-KEY-038 | pi sends `x-client-request-id` with the session id as value | The proxy must not read it as a request id. | PROVEN: SC | FIX-102 |
| EDGE-KEY-039 | A message has only `<system-reminder>` blocks and the next message holds the prompt | The proxy must join the blocks in front of the prompt text. | PROVEN: SC, vectors V10 and V14 | FIX-305 |

## Affinity

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-AFF-001 | First request of a key | The proxy must place the key by the admission rules and store the key, the node and the last-seen time. | PROPOSED | FIX-110 |
| EDGE-AFF-002 | The key returns while its node has no free cap place | The proxy must hold the request for that node and must not move the conversation. | PROVEN: strict stickiness (BRIEF) | FIX-111 |
| EDGE-AFF-003 | The key returns after the table expiry | The proxy must place the key as a new conversation. | PROPOSED | FIX-117 |
| EDGE-AFF-004 | The key returns after the protected window but before the table expiry, and another conversation took its seat | The proxy must send the request to the same node and wait for a cap place there. | PROPOSED | FIX-122 |
| EDGE-AFF-005 | The node of a key goes unavailable | The proxy must move the key to another node on the next request and log the move with its reason. | PROVEN: BRIEF | FIX-111 |
| EDGE-AFF-006 | The old node returns after the key moved | The proxy must keep the key on the new node. | PROPOSED: a second move costs a second prefill | FIX-114 |
| EDGE-AFF-007 | A conversation whose node the registry no longer lists | The proxy must treat the entry as unavailable, place the key again and not fail on the missing node. | PROPOSED | FIX-113 |
| EDGE-AFF-008 | The registry removes a node while a request of a key runs on it | The proxy must let the request end and must drop the entry when it ends. | PROPOSED | FIX-113 |
| EDGE-AFF-009 | The table is at its cap and a new key arrives | The proxy must evict the entry with the oldest last-seen time and keep the table at or below its cap. | PROVEN: BRIEF cap rule | FIX-112 |
| EDGE-AFF-010 | The table is full and every entry has a running request | The proxy must not evict an entry with a running request. | PROPOSED | FIX-112 |
| EDGE-AFF-011 | A node has poor measured cache reuse | The proxy must route to that node by least load and must not apply the protected window. | PROVEN: BRIEF | FIX-115 |
| EDGE-AFF-012 | The node returns no cache fields, for example vLLM without the details flag | The proxy must record reuse as unknown and must keep affinity. | PROVEN: flag needed (S6 vLLM) | FIX-179 |
| EDGE-AFF-013 | Measured reuse crosses the poor threshold up and down | The proxy must change mode at most once for each measurement window. | PROPOSED | FIX-115 |
| EDGE-AFF-014 | The first turn after a move | The proxy must log the turn as cold with the reason "moved". | PROPOSED | FIX-116 |
| EDGE-AFF-015 | A node leaves one alias but stays in the registry | The proxy must treat entries of that alias on that node as unavailable. | PROPOSED | FIX-118 |
| EDGE-AFF-016 | One node serves two aliases | The proxy must apply one cap to the node across both aliases. | PROPOSED | FIX-119 |
| EDGE-AFF-017 | A request arrives at expiry minus 1 ms, at expiry and at expiry plus 1 ms | The proxy must hit the entry at the first two times and place the key anew at the third. | PROPOSED: idle longer than the limit expires | FIX-117 |
| EDGE-AFF-018 | The sweeper and a request run at the same virtual millisecond | The proxy must give the same result on every run. | PROVEN: same-instant order is FIFO (S2 F4) | FIX-117 |
| EDGE-AFF-019 | A request to the node fails with a node error | The proxy must not refresh the last-seen time. | PROVEN: OpenRouter does the same (R5) | FIX-110 |
| EDGE-AFF-020 | A request runs 230 s, longer than the protected window | The proxy must count idle time from the end of the last response. | PROPOSED | FIX-135 |
| EDGE-AFF-021 | The first turn of a new conversation has zero cache reuse | The proxy must exclude the first turn from the reuse measure. | PROPOSED | FIX-115 |
| EDGE-AFF-022 | A fifth conversation evicts the engine cache of a fourth on a four-slot node | The proxy must keep the table entry and must log the next turn as cold with the reason "engine eviction". | PROVEN: slot reuse evicts (S6 llama.cpp) | FIX-189 |
| EDGE-AFF-023 | Two keys land on one node and a third node is idle | The proxy must keep both keys on the node while the node has free seats. | PROPOSED | FIX-122 |
| EDGE-AFF-024 | An admin read lists the table | The proxy must not change any last-seen time because of the read. | PROPOSED | FIX-243 |
| EDGE-AFF-025 | A conversation on a hybrid node must move because its node is down | The proxy must count the move as a full prefill of the prompt in its cost estimate. | PROVEN for Qwen3.5-2B: every checkpoint is lost (S7). ASSUMPTION for larger models | FIX-333 |

## Admission and hold

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-ADM-001 | Two requests reach a serial node (cap 1) in the same millisecond | The proxy must run one and hold the other until the first ends. | PROVEN: Ollama serial (S3) | FIX-120 |
| EDGE-ADM-002 | Five requests reach a four-slot llama-server | The proxy must run four and hold the fifth until a cap place frees. | PROVEN: overlap equals slots (S3) | FIX-120 |
| EDGE-ADM-003 | mlx_lm admits 8 requests, the cap is 1 | The proxy must hold the second request. | PROVEN: batching does not raise the cap (S3) | FIX-120 |
| EDGE-ADM-004 | The registry says 4 slots and `/props` says `total_slots` 2 | The proxy must use the cap 2. | PROVEN: the stricter value wins (S3) | FIX-129 |
| EDGE-ADM-005 | A conversation is idle 180 000 ms, then 180 001 ms | The proxy must protect the seat at 180 000 ms and must allow takeover at 180 001 ms. | PROPOSED | FIX-121 |
| EDGE-ADM-006 | A new conversation and three kinds of seat: free and warm, free and cold, idle beyond the window | The proxy must choose in this order: free warm, idle longest beyond the window, else hold. | PROPOSED | FIX-122 |
| EDGE-ADM-007 | All seats hold conversations idle for less than the window | The proxy must hold the new request and must not evict. | PROVEN: BRIEF | FIX-122 |
| EDGE-ADM-008 | A free seat and an idle conversation beyond the window both exist | The proxy must use the free seat. | PROPOSED: no eviction when a free seat exists | FIX-122 |
| EDGE-ADM-009 | A held request waits 250 s | The proxy must answer with the hold-limit error and keep the count of running requests unchanged. | PROVEN: every harness retries 503 (spike D, OPEN-003, DEC-064 pending owner approval) | FIX-123 |
| EDGE-ADM-010 | A cap place frees at the hold limit minus 1 ms, at the limit, and at the limit plus 1 ms | The proxy must serve the request in the first two cases and answer the error in the third. | PROPOSED | FIX-124 |
| EDGE-ADM-011 | The hold limit expires and the node frees at the same instant | The proxy must answer the held request exactly once. | PROPOSED | FIX-124 |
| EDGE-ADM-012 | The harness disconnects while the proxy holds its request | The proxy must remove the request from the queue, take no cap place and write one log record. | PROPOSED | FIX-125 |
| EDGE-ADM-013 | The harness disconnects at the instant the proxy assigns the cap place | The proxy must release the cap place and must not count a running request. | PROPOSED | FIX-125 |
| EDGE-ADM-014 | Three requests wait and one cap place frees | The proxy must serve the oldest request that can use that node. | PROPOSED | FIX-126 |
| EDGE-ADM-015 | A held request belongs to a conversation on node A and node B has a free seat | The proxy must keep the request held for A. | PROVEN: strict stickiness (BRIEF). The waste is RISK-008 | FIX-126 |
| EDGE-ADM-016 | Parents hold idle seats and their children wait for seats | The proxy must free a parent seat when the window ends, so no hold lasts longer than the window plus the longest run. | PROPOSED. Deadlock source: R3 | FIX-127 |
| EDGE-ADM-017 | Twenty children arrive at once on a one-slot node | The proxy must serve them in order of arrival with no starvation. | PROVEN: Open WebUI runs 20 in parallel (OWUI). Not run | FIX-127 |
| EDGE-ADM-018 | The same conversation sends a second request while the first runs | The proxy must count both against the cap and must keep both on one node. | PROPOSED | FIX-128 |
| EDGE-ADM-019 | The harness cancels while the node keeps prefill running | The proxy must count the request as running until the node connection closes. | PROVEN: mlx_lm ignores cancel (S3) | FIX-134 |
| EDGE-ADM-020 | A stream ends normally | The proxy must free the cap place when the body ends and not when the headers arrive. | PROPOSED | FIX-134 |
| EDGE-ADM-021 | The node answers an error status at once | The proxy must free the cap place and leave the running count at zero. | PROPOSED | FIX-134 |
| EDGE-ADM-022 | A reload lowers a cap below the running count | The proxy must let running requests end and admit nothing until the count is below the new cap. | PROPOSED | FIX-129 |
| EDGE-ADM-023 | A reload raises a cap while the proxy holds requests | The proxy must start held requests at once up to the new cap. | PROPOSED | FIX-129 |
| EDGE-ADM-024 | The proxy holds 5000 requests | The proxy must keep its memory below the held-queue bound and must answer the overflow with an error at once. | PROPOSED. Bound is OPEN-014 | FIX-132 |
| EDGE-ADM-025 | Every node of the alias is unavailable | The proxy must hold the request until a node returns or the hold limit ends. | PROPOSED | FIX-130 |
| EDGE-ADM-026 | The node of the conversation returns while a request waits | The proxy must start the request on that node when a cap place is free. | PROPOSED | FIX-131 |
| EDGE-ADM-027 | The registry gives a node 0 slots | The proxy must refuse the registry at load with a message that names the node. | PROPOSED | FIX-208 |
| EDGE-ADM-028 | The registry sets the hold-limit error status to 503, then 504, then 529, then 429 | The proxy must send the status that the registry sets, with the default 503. The registry must refuse 429. The test must record which status each harness stub retries. | PROVEN: every harness retries 503, 502, 504, 500 and 529. Codex fails at once on 429 (SD). The registry refuses 429 | FIX-133 |
| EDGE-ADM-029 | A 250 s hold ends with an error and the harness retries three times | The proxy must treat each retry as a new request that enters the queue behind earlier requests, with no duplicate cap place. | PROVEN: pi retries at 2, 4 and 8 s (S5) | FIX-136 |
| EDGE-ADM-030 | A running request ends in the same millisecond as the window limit of an idle conversation, and the proxy holds a request | The proxy must make the same decision on every replay of the seed. | PROVEN: replay by seed (S2 F5) | FIX-280 |
| EDGE-ADM-031 | A side request (title) arrives on a node whose only seat holds a protected conversation | The proxy must hold the side request. | PROVEN: a side request evicts the main cache on a one-slot hybrid (R6) | FIX-122 |
| EDGE-ADM-032 | The proxy holds a request 250 s with no byte for each harness of the matrix | The proxy must serve the request and every harness must complete at the first attempt. | PROVEN on fakes: SD, table 2 | FIX-306 |
| EDGE-ADM-033 | The hold limit ends with 503 and then with 429 for a Codex stub | The proxy must use 503. The Codex stub must retry 503 and must fail at once on 429. | PROVEN: SD | FIX-307 |
| EDGE-ADM-034 | The registry sets the proxy to send an early response head with keep-alive during the hold | The proxy must refuse the setting. v1 sends nothing during a hold. | PROVEN: DeepSeek Harness still gives up at 299 s and Codex needs a real event (SD). Rejection by DEC-064 | FIX-315 |
| EDGE-ADM-035 | The refusal carries `Retry-After` of 30 s, then 90 s | The proxy must send 30 s or less. A Claude Code stub must retry at 30 s and must fail at once at 90 s. | PROVEN: SD | FIX-313 |
| EDGE-ADM-036 | The hold limit plus the time until the node head is 300 s | The proxy must write a warning at start for a hold limit above 290 s. The pi stub gives up at 299 s. | PROVEN limit: SD. The warning is PROPOSED | FIX-323 |
| EDGE-ADM-037 | DeepSeek Harness or opencode sends a title call and the main request at session start, and a cap place is free for one only | The proxy must hold the second request and must not count the title call as a conversation. | PROVEN: both send two requests (SD, SC) | FIX-311 |
| EDGE-ADM-038 | Each harness of the matrix gets 503 at the hold limit and all attempts fail | The proxy must hold each retry again. The wait is at most the attempts times the hold limit. | PROVEN: attempts are 3, 4, 7, 9, 30 and 11 (SD). Worst case is derived | FIX-316 |
| EDGE-ADM-039 | A llama-server node with 2 slots and `warm_capacity` 4 serves 4 conversations | The proxy must keep 4 seats and must run at most 2 requests at once. | PROVEN for the warm swap with 2 conversations on 1 slot (S7). The count 4 is a test value | FIX-336 |
| EDGE-ADM-040 | A node with one sequence for each slot and no host cache has 2 slots | The proxy must keep 2 seats. | PROPOSED (DEC-066). Ollama default and mlx_lm.server | FIX-336 |

## Protocol

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-PRO-001 | The `model` field names an alias | The proxy must replace the model value with the node model name and change no other byte of the body. | PROVEN: BRIEF byte-for-byte rule | FIX-142 |
| EDGE-PRO-002 | A tool definition holds a parameter named `model` | The proxy must change the top-level `model` field only. | PROPOSED | FIX-142 |
| EDGE-PRO-003 | The `model` field is missing | The proxy must answer 400 with an error that has none of the words pi retries on. | PROVEN: error text decides pi retry (S5) | FIX-143 |
| EDGE-PRO-004 | The `model` field is a number | The proxy must answer 400 in the error shape of the protocol. | PROPOSED | FIX-143 |
| EDGE-PRO-005 | The alias is unknown | The proxy must answer 404 in the error shape of the protocol. | PROVEN: pi does not retry 404 (S5) | FIX-142 |
| EDGE-PRO-006 | The body is not valid JSON | The proxy must answer 400 and must not forward it. | PROVEN: llama-server answers 500 and pi retries it (RND1 3.5) | FIX-143 |
| EDGE-PRO-007 | The body is a JSON array and not an object | The proxy must answer 400. | PROPOSED | FIX-143 |
| EDGE-PRO-008 | The body is larger than the body limit | The proxy must answer 413 before it reads the rest. | PROPOSED | FIX-144 |
| EDGE-PRO-009 | The harness sends a chunked body with no `content-length` | The proxy must read the body to its end and forward it intact. | PROPOSED | FIX-158 |
| EDGE-PRO-010 | The harness sends `Expect: 100-continue` | The proxy must answer the interim response and then read the body. | ASSUMPTION | FIX-158 |
| EDGE-PRO-011 | The harness sends `Accept-Encoding: gzip` | The proxy must forward the header and must not change the body bytes of the answer. | ASSUMPTION | FIX-150 |
| EDGE-PRO-012 | The request holds hop-by-hop headers (`Connection`, `TE`, `Upgrade`) | The proxy must not forward hop-by-hop headers. | PROVEN: HTTP rule (standard) | FIX-150 |
| EDGE-PRO-013 | Claude Code sends 11 beta headers | The proxy must forward every unknown header to a local node on the Messages path. | PROVEN: RND1 3.6 | FIX-141 |
| EDGE-PRO-014 | The Messages `system` field is a list of blocks with `cache_control` markers that move between turns | The proxy must derive the key from the text and ignore the markers. | PROPOSED. See OPEN-011 | FIX-105 |
| EDGE-PRO-015 | A request holds a mid-conversation `system` message | The proxy must pass it unchanged and use the first system message for the key. | PROVEN: Claude Code sends them (RND1 3.6) | FIX-105 |
| EDGE-PRO-016 | A harness calls `/v1/embeddings` | The proxy must pass the request by path to a node of the alias that serves embeddings. | ASSUMPTION. Approved by the owner (DEC-004). The path list is OPEN-004 | FIX-153 |
| EDGE-PRO-017 | A harness calls `/v1/models` | The proxy must answer with the aliases of the registry or a documented error. | NOT TESTED: see OPEN-004 | FIX-154 |
| EDGE-PRO-018 | A harness requests `/v1/messages/count_tokens` | The proxy must pass the request to a node that serves it. | NOT TESTED: RND1 section 6 | FIX-155 |
| EDGE-PRO-019 | REMOVED 2026-10-07. A harness calls `/v1/responses` and gets 404 | Superseded: see EDGE-PRO-027 | Owner decision DEC-062 supersedes DEC-057 | FIX-162 |
| EDGE-PRO-020 | The request has no `stream_options` and the engine needs it for usage | The proxy must add no field except a patch set in the registry. | PROVEN: BRIEF byte-for-byte rule | FIX-161 |
| EDGE-PRO-021 | The harness sends a field that the node patch also sets, for example `chat_template_kwargs` | The proxy must apply the node patch and the result must be equal on every turn. | PROPOSED | FIX-160 |
| EDGE-PRO-022 | The registry changes a node patch while a conversation runs | The proxy must apply the new patch to the next request and log the change. | PROPOSED. The cache breaks once | FIX-160 |
| EDGE-PRO-023 | An error from the proxy itself, in each protocol | The proxy must use the OpenAI error shape on the OpenAI path and the Messages error shape on the Messages path. | ASSUMPTION | FIX-163 |
| EDGE-PRO-024 | The proxy error body for a status that pi does not retry | The body must not hold the words "503", "timeout", "rate limit", "overloaded" or `terminated`. | PROVEN: S5 caveat | FIX-262 |
| EDGE-PRO-025 | An HTTP/1.0 client with no keep-alive | The proxy must answer and close the connection. | ASSUMPTION | FIX-158 |
| EDGE-PRO-026 | Non-streaming response with usage fields | The proxy must pass the body unchanged and read the usage for the log. | PROPOSED | FIX-151 |
| EDGE-PRO-027 | A harness calls `/v1/responses` with an alias that has a Responses node | The proxy must route the request, rewrite `model` and forward the response byte for byte. | PROVEN for Codex against a fake (SC, SD). Engine support in [05](05-engine-behaviour.md) section 14 (R7, source only) | FIX-318 |
| EDGE-PRO-028 | A harness calls `/v1/responses` with an alias that has no Responses node | The proxy must answer 404 in the OpenAI error shape (PRX-PROTO-056). | PROPOSED (DEC-065, owner decision pending). R7: mlx_lm.server has no `/v1/responses` | FIX-318, FIX-337 |
| EDGE-PRO-029 | A node cuts a Responses stream in mid-stream | The proxy must close the connection with no terminator and no error event. | PROVEN: Codex retries a dropped connection with 30 requests (SD) | FIX-319 |
| EDGE-PRO-030 | A refusal body of status 400 or 409 holds the word `server_error` | The test must fail the build. pi and opencode retry on that word. | PROVEN: SD | FIX-314 |
| EDGE-PRO-031 | A Responses request carries `previous_response_id` and `store` | The proxy must pass both fields unchanged to a node that does not ignore the id. | PROPOSED. Codex 0.160.1 sends `store: false` and no `previous_response_id` on a local provider (R7). Live node behaviour NOT TESTED | FIX-318 |
| EDGE-PRO-032 | A node sets `x-codex-turn-state` on a response and Codex replays it | The proxy must pass the header on the response and on the next request unchanged. | PROVEN as source reading (R7). I: only the OpenAI backend sets it today | FIX-338 |
| EDGE-PRO-033 | A Codex request carries `session-id`, `thread-id`, `x-codex-window-id`, `x-codex-turn-metadata` and `x-codex-parent-thread-id` | The proxy must pass every header unchanged to a local node. | PROVEN for the first four against a fake (SC). The parent header is source only (R7) | FIX-338 |
| EDGE-PRO-034 | A Responses body holds a `reasoning` item with `summary: null`, an `encrypted_content` and `include: ["reasoning.encrypted_content"]` | The proxy must pass the body byte for byte, except the `model` value. | PROVEN as Codex behaviour (R7). The engine can answer 400 (llama-server issue 29159) and the proxy passes that answer | FIX-318 |
| EDGE-PRO-035 | A request carries `previous_response_id` and the target node has `ignores_previous_response_id` | The proxy must answer 400 in the OpenAI error shape. | PROPOSED (OPEN-041). Ollama ignores the id with no error (R7, source) | FIX-014 |
| EDGE-PRO-036 | A request carries `previous_response_id` that a stateful node made | The proxy must send the request to that node. | PROPOSED. R7 inferred | FIX-330 |
| EDGE-PRO-037 | The proxy restarts and loses the table of response ids | The proxy must pass the error of the node that does not know the id. | PROPOSED. The cost class equals a cold turn | FIX-330 |
| EDGE-PRO-038 | A node of engine mlx_lm has `responses` true | The proxy must warn at load and must pass the 404 of the node. | PROVEN by source: mlx_lm.server answers 404 on the path (R7) | FIX-337, FIX-015 |
| EDGE-PRO-039 | A harness requests `/v1/responses/compact` | The proxy must route it by the rules of `/v1/responses` and pass the answer, also an error. | PROVEN as a Codex request (R7). llama-server does not serve it (unverified) | FIX-318 |
| EDGE-PRO-040 | A Responses request has `Content-Encoding: zstd` and no header key | The proxy must use no body key and must pass the body unchanged. | PROPOSED. Codex 0.160.1 does not compress for custom providers (R7, source) | FIX-339 |
| EDGE-PRO-041 | The hold limit ends on a Responses request | The proxy must answer 503, never 429. | PROVEN: Codex fails at once on 429 and retries 503 (SD, R7) | FIX-307 |

## Streaming

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-STR-001 | An OpenAI stream with comments, `\r\n` line ends and multi-line data | The proxy must pass every byte unchanged. | PROVEN: byte-identical in the prototype (RND1 3.2) | FIX-140 |
| EDGE-STR-002 | A Messages stream with `ping` events | The proxy must pass every byte unchanged. | PROVEN: RND1 3.2 | FIX-141 |
| EDGE-STR-003 | The stream ends without a final usage chunk | The proxy must close the harness stream, log the usage as unknown and keep the cold-turn flag empty. | PROPOSED | FIX-145 |
| EDGE-STR-004 | The usage chunk splits across two reads at a byte inside a number | The proxy must read the usage from the joined bytes and forward the bytes without delay. | PROPOSED | FIX-145 |
| EDGE-STR-005 | The usage chunk comes after the `finish_reason` chunk and before `[DONE]` | The proxy must read it and forward all chunks in order. | PROPOSED | FIX-145 |
| EDGE-STR-006 | The stream ends without `[DONE]` | The proxy must pass what arrived, close the harness side, free the cap place and log a short stream. | PROVEN: pi retries "Stream ended without finish_reason" (S5) | FIX-146 |
| EDGE-STR-007 | The node drops the connection in mid-stream on the chat path | The proxy must close the harness connection at once and must not send a made-up end. | PROVEN: pi retries and discards the partial (S5 row 3b) | FIX-148 |
| EDGE-STR-008 | A node answers 200 with empty content, no tool call and `finish_reason` `stop` | The proxy must pass the answer unchanged and log "empty completion". | PROVEN: truncation gives odd answers (S3). Log field PROPOSED | FIX-147 |
| EDGE-STR-009 | The node answers a 4xx or 5xx status with a body | The proxy must pass the status and body unchanged. | PROVEN: BRIEF no retries | FIX-149 |
| EDGE-STR-010 | The node answers 429 with `Retry-After: 120` | The proxy must pass the header unchanged. | PROVEN: pi ignores it (S5 2b) | FIX-149 |
| EDGE-STR-011 | The node refuses the connection | The proxy must answer 502 in the protocol error shape and free the cap place. | PROPOSED | FIX-148 |
| EDGE-STR-012 | The node accepts the connection and sends nothing | The proxy must send nothing, add no timeout and close the node connection when the harness closes. | PROVEN: BRIEF no timeouts. Pi cuts at 300.6 s (S5 row 4) | FIX-148 |
| EDGE-STR-013 | The node resets the connection at accept | The proxy must answer 502 and keep the node available after one reset. | PROPOSED | FIX-148 |
| EDGE-STR-014 | A pooled connection to llama-server that its 5 s keep-alive closed | The proxy must not send a request on a connection older than the node idle limit. | PROVEN: one failure seen (S3 finding 9). Rule PROPOSED | FIX-157 |
| EDGE-STR-015 | The harness reads one chunk per second | The proxy must slow its read from the node and must not buffer without bound. | PROVEN: backpressure is deterministic (S2 F3) | FIX-156 |
| EDGE-STR-016 | The harness closes the stream after 3 chunks | The proxy must close the node connection and count the request as ended when the node closes. | PROPOSED | FIX-134 |
| EDGE-STR-017 | The first byte comes after 5, 20 and 60 s | The proxy must pass the bytes at once and add no delay of its own. | PROVEN: pi waits without retry (S5 row 4) | FIX-033 |
| EDGE-STR-018 | Added latency on a passthrough stream | The proxy must add no more than the prototype figures: 0.06 ms to the first byte and 0.05 ms per chunk. | PROVEN for the prototype (RND1 3.2). Rust proxy NOT TESTED | FIX-290 |
| EDGE-STR-019 | An Ollama request queued behind another one | The proxy must hold the request so the harness sees the same wait as a header delay at the proxy. | PROVEN: Ollama sends no headers while queued (S3 finding 2) | FIX-001 |
| EDGE-STR-020 | A mid-stream error event of type `overloaded_error` from a hosted node | The proxy must pass the event unchanged. | PROVEN: both harnesses retry it (RND1 3.2) | FIX-221 |
| EDGE-STR-021 | The node drops the connection in mid-stream on the Messages path | The proxy must send one `overloaded_error` event and then close. | PROPOSED: approved default (DEC-058). Spike D tests the abrupt close and can change it | FIX-304 |
| EDGE-STR-022 | A node cuts a stream and the proxy sends an OpenAI-style error event | The test must fail the build. The SDKs, pi on the OpenAI path and DeepSeek Harness then fail with no retry. | PROVEN: SD, table 4 | FIX-319 |
| EDGE-STR-023 | A Codex stub gets an SSE comment line during a long wait | The proxy must not rely on comments. Codex gives up at 300 s with a comment only. | PROVEN: SD. The proxy sends nothing during a hold | FIX-315 |

## Engines

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-ENG-001 | Ollama with a 4096 context receives a prompt of about 3000 tokens | The proxy must answer a context error and must not forward the request. | PROVEN: Ollama cut to about 2050 tokens with status 200 (S3, RND1 3.5) | FIX-170 |
| EDGE-ENG-002 | The prompt is just under the truncation limit by estimate but the reply reports far fewer prompt tokens | The proxy must log "truncation suspected" with both counts. | PROPOSED | FIX-170 |
| EDGE-ENG-003 | The token estimate is 10 percent wrong in each direction | The proxy must still meet the guard rule at the estimate error bound. | NOT TESTED: see OPEN-021 | FIX-194 |
| EDGE-ENG-004 | The registry declares Ollama with 4 parallel and the truth is 1 | The proxy must take the stricter measured cap. | PROVEN: Ollama has no discovery (S3 finding 1) | FIX-172 |
| EDGE-ENG-005 | Ollama unloaded the model after its keep-alive and a request arrives | The proxy must wait for the node and must not count the load time as a cold turn. | ASSUMPTION: keep-alive 5 min (R4) | FIX-171 |
| EDGE-ENG-006 | llama-server with `-np 4 -c 16384` gets a 6850-token prompt | The proxy must answer a context error from the per-slot context, or pass the node 400 unchanged. | PROVEN: 400 `exceed_context_size_error` (S3 finding 3) | FIX-173 |
| EDGE-ENG-007 | llama-server answers 500 for a valid JSON body it dislikes | The proxy must pass status and body unchanged and keep the node available. | PROVEN: RND1 3.5 | FIX-174 |
| EDGE-ENG-008 | `/metrics` is not enabled on llama-server | The proxy must use its own running count and must not poll `/slots`. | PROVEN: S3 finding 3 | FIX-176 |
| EDGE-ENG-009 | llama-server runs with `--sleep` and is asleep | The proxy must not poll `/slots`, and the first request must not count the wake time as a cold turn. | PROVEN: `/slots` wakes the engine (RND1 3.5) | FIX-175 |
| EDGE-ENG-010 | mlx_lm drops the connection on an invalid request | The proxy must close the harness connection and keep the node available. | PROVEN: RND1 3.5 | FIX-177 |
| EDGE-ENG-011 | mlx_lm wedges under heavy load | The proxy must show the stuck running count in the admin read and must take no action. | PROVEN: seen once (RND1 6). v1 has no timeouts | FIX-178 |
| EDGE-ENG-012 | The registry says the engine is Ollama and the node answers like llama-server | The proxy must log the mismatch and use the measured profile. | ASSUMPTION | FIX-191 |
| EDGE-ENG-013 | The engine version changes while the proxy runs | The proxy must log the change and report the node as "needs calibration". | PROVEN: Ollama updated mid-test (RND1 D-5) | FIX-191 |
| EDGE-ENG-014 | Two nodes of one alias use different thinking patches | The proxy must apply each patch for its node only and must not mix them in one conversation. | PROVEN: patches fixed per node (BRIEF) | FIX-160 |
| EDGE-ENG-015 | The node model name in the answer differs from the one sent | The proxy must pass the answer unchanged. | ASSUMPTION | FIX-142 |
| EDGE-ENG-016 | An engine restarts and the proxy keeps its table | The proxy must keep affinity and the next turn must show as cold with the reason "engine restart" or "miss". | PROPOSED | FIX-191 |
| EDGE-ENG-017 | A node agent reports a temperature limit and thermal throttling | The proxy must show the reading in the admin read and must not change routing. | PROVEN: BRIEF reports only | FIX-012 |
| EDGE-ENG-018 | Prefill speed of 250 tokens per second and a 7000-token prompt on two slots at once | The stub must give both requests a first token about twice as late as one alone. | PROVEN: one shared prefill pool (S3 finding 7) | FIX-193 |

## Caches

Reuse in these rows means the cache fields of the reply: `timings.cache_n` and `prompt_n` on llama.cpp and gufo, `prompt_tokens_details.cached_tokens` on the others. Closed forms are in [spike s6](evidence/spikes/s6-checkpoint-source-README.md).

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-CAC-001 | llama.cpp: the template strips the reasoning of the last answer, so the common prefix ends before the stored end | The proxy must log `cache_n` and `prompt_n` so the lost tokens show, and must keep affinity. | PROVEN: loss is the generated tokens plus 4 (S6 llama.cpp) | FIX-181 |
| EDGE-CAC-002 | llama.cpp: a request whose common prefix is below the last checkpoint | The stub must remove the later checkpoints and the oracle must match. | PROVEN: S6 llama.cpp step 4 | FIX-192 |
| EDGE-CAC-003 | llama.cpp: `tokens_cached` in a non-stream result | The proxy must read reuse from `timings.cache_n`, not from `tokens_cached`. | PROVEN: `tokens_cached` is the stored length (S6) | FIX-188 |
| EDGE-CAC-004 | llama.cpp: 4 slots, 32 checkpoints each of 63 to 214 MiB and `--cache-ram` 8 GiB | The proxy must report falling reuse from the fields and must not claim the cause. | ASSUMPTION: overflow cause. Sizes second-hand (R4) | FIX-182 |
| EDGE-CAC-005 | vLLM without `--enable-prompt-tokens-details` | The proxy must record reuse as unknown and must not mark the node poor. | PROVEN: flag default off (S6 vLLM) | FIX-179 |
| EDGE-CAC-006 | vLLM align mode: the one checkpoint lies in the unique tail, so reuse is 0 percent with status 200 | The proxy must measure the zero reuse and must switch the node to least load after the measure rule applies. | PROVEN: issue 45238 (R2) | FIX-180 |
| EDGE-CAC-007 | vLLM hybrid with a 479-token prompt and block size 528 | The proxy must exclude prompts below one block from the reuse measure. | PROVEN: issue 40696 (R2) | FIX-190 |
| EDGE-CAC-008 | SGLang: reuse has a period-256 pattern in decode | The proxy must measure reuse over a window of requests and not from one request. | PROVEN: sawtooth by source (S6 SGLang) | FIX-183 |
| EDGE-CAC-009 | gufo: a harness edits a middle message | The proxy must keep affinity and the logged reuse must equal the engine fields. | PROVEN: exact-prefix reuse (S6 gufo) | FIX-184 |
| EDGE-CAC-010 | mlx-lm: 4 conversations and a prompt cache limit of 10 entries | The proxy must show reuse from the fields as the engine evicts "assistant" entries first. | PROVEN: S6 mlx-lm | FIX-185 |
| EDGE-CAC-011 | A node runs MTP and prefix caching on a hybrid model | The proxy must use measured reuse and ignore the declared cache setting. | NOT TESTED: corruption issues exist (R2). See OPEN-010 | FIX-186 |
| EDGE-CAC-012 | A compaction call rewrites the history | The proxy must log the next turn as cold on the same node with no move. | PROVEN: compaction misses the cache (R4) | FIX-187 |
| EDGE-CAC-013 | A harness uses the slot save and restore endpoint | The proxy must not treat a restore as a move path. | PROVEN: hybrid checkpoints are not saved (R4, issue 25913) | FIX-181 |
| EDGE-CAC-014 | A reply has no cache field at all, for example an older engine | The proxy must record reuse as unknown. | PROPOSED | FIX-188 |
| EDGE-CAC-015 | A conversation moves to a node with cold cache at 60000 tokens | The stub must charge the full prefill time of its speed model. | PROVEN: 47 s on gufo and 230 s on llama.cpp (R4) | FIX-116 |
| EDGE-CAC-016 | All stub policies run the same random conversation set | Each stub must equal the closed form `N - max{c in C : c <= D}` on every request. | PROVEN: closed forms from source (S6) | FIX-192 |
| EDGE-CAC-017 | SGLang router baseline with a cache tree that records a prefix as cached after the engine evicts it | The benchmark must record the miss from the reply fields. | PROVEN: tree has no feedback (R3) | FIX-296 |
| EDGE-CAC-018 | Hybrid llama.cpp, N = 6000: the common prefix is 5483, then 5484 | The oracle must give 6000 recomputed tokens for 5483 and 516 for 5484. | PROVEN: S7, 3 of 3 repetitions (15.0 s against 1.3 s) | FIX-333 |
| EDGE-CAC-019 | Hybrid llama.cpp: a turn appends 200 tokens after 300 generated tokens | The stub must recompute 201 tokens. | PROVEN: S7, hybrid and dense | FIX-333 |
| EDGE-CAC-020 | Two conversations A, B, A, B on one slot with `--cache-ram` 8192 | Each turn after the first must recompute the new tokens plus 1. | PROVEN: S7, `-np 1` and `-np 2` | FIX-334 |
| EDGE-CAC-021 | The same two conversations with `--cache-ram` 0 | Every turn must be cold and the proxy must report the finding for `--cache-ram` 0. | PROVEN: about 23 s a turn at 6k (S7) | FIX-334 |
| EDGE-CAC-022 | Hybrid chat route, 6075 tokens: a word in message 1 changes (D = 194) | The stub must recompute 5942 tokens and the cold-turn event must fire. | PROVEN: S7, about 22 s | FIX-333 |
| EDGE-CAC-023 | Hybrid chat route, 12281 tokens: a word at D = 9354 changes | The stub must recompute 3877 tokens and the cold-turn event must fire although the share is 32 percent. | PROVEN: S7, about 16.5 s. PRX-ENG-038 | FIX-333 |
| EDGE-CAC-024 | Dense control, any edit at D | The stub must recompute `N - D`. | PROVEN: S7 qwen3:1.7b | FIX-192 |
| EDGE-CAC-025 | A streamed llama-server response has no `timings` | The proxy must record reuse as unknown for that turn and must not mark the node poor. | NOT TESTED: S7 checked the non-stream shape (OPEN-044) | FIX-332 |
| EDGE-CAC-026 | `--checkpoint-min-step 1024` on a hybrid node | The stub must make user-message checkpoints about 1.2k tokens apart. | PROVEN for 1 repetition at 12k (S7). About 19 MiB for each checkpoint for each slot | FIX-004 |
| EDGE-CAC-027 | A template rewrites the last 2 tokens of the previous prompt | The stub must reuse the `N - 4` checkpoint and recompute the new tokens plus the discarded tail (332 in S7). | PROVEN: S7, synthetic history | FIX-181 |

## Restart

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-RST-001 | A request arrives while the proxy starts | The proxy must hold the request and serve it when the proxy is ready. | PROVEN: pi tolerates a hold up to 300 s (S5) | FIX-200 |
| EDGE-RST-002 | The proxy is not ready at the hold limit | The proxy must answer the hold-limit error. | PROPOSED | FIX-204 |
| EDGE-RST-003 | The proxy restarts with streams running | The harness stub must see a closed connection and the retry must work on the new process. | PROVEN: pi gets `terminated` and retries (S5 row 3b) | FIX-201 |
| EDGE-RST-004 | The table is empty after the restart and a known key returns | The proxy must place the key by the admission rules and do no recovery work. | PROVEN: BRIEF soft state | FIX-203 |
| EDGE-RST-005 | The engines still run requests from before the restart | The proxy must seed the running count from the engine signal where one exists. | PROPOSED: signals exist on llama-server only (S3) | FIX-209 |
| EDGE-RST-006 | 300 harnesses connect during the start | The proxy must accept and hold them up to the bound and must report the count. | NOT TESTED: backlog at scale (S5) | FIX-202 |
| EDGE-RST-007 | A second proxy starts on the same port | The second process must exit with a message and the first must not change. | PROPOSED | FIX-205 |
| EDGE-RST-008 | SIGTERM arrives with 3 requests running | The proxy must stop accepting, let the running requests end and exit when none runs. | PROPOSED | FIX-207 |
| EDGE-RST-009 | The registry file is invalid at start | The proxy must exit before it listens and name the error. | PROPOSED | FIX-208 |
| EDGE-RST-010 | The registry file is invalid at a reload | The proxy must keep the running configuration and show the error in the admin read. | PROPOSED | FIX-208 |
| EDGE-RST-011 | The wall clock jumps back 1 hour and then forward 1 hour | The proxy must not expire or extend any entry because of the jump. | PROVEN for leases in code (RND1 3.3). The proxy NOT TESTED | FIX-206 |
| EDGE-RST-012 | The machine sleeps for 10 minutes and wakes | The proxy must not count the sleep as idle time for every entry. | NOT TESTED: sleep clock (RND1 6) | FIX-206 |
| EDGE-RST-013 | The proxy crashes with 5 requests running | The log must hold no record for those 5 requests and the next start must not write one. | PROPOSED: the proxy writes a record at the end | FIX-201 |
| EDGE-RST-014 | A reload removes a node that held requests wait for | The proxy must move the held requests to the remaining nodes of the alias or answer the hold-limit error. | PROPOSED | FIX-130 |
| EDGE-RST-015 | Ready time with a registry of 5 nodes and a slow node | The proxy must not wait for a slow node and must treat unknown health as available. | PROPOSED | FIX-200 |

## Hosted nodes

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-HST-001 | An inbound `authorization` and `x-api-key` header reach a hosted node route | The proxy must remove both and set its own key. | PROVEN: BRIEF key custody | FIX-220 |
| EDGE-HST-002 | An inbound `x-legatus-lease` and any other `x-legatus-*` header | The proxy must remove every `x-legatus-*` header before the hosted node. | PROVEN: BRIEF | FIX-152 |
| EDGE-HST-003 | The key variable is missing at the start | The proxy must refuse to load the hosted node and must name the variable but not any value. | PROPOSED | FIX-226 |
| EDGE-HST-004 | The harness sends its own hosted key and the proxy has no key for that node | The proxy must not forward the inbound credential and must answer an error. | PROPOSED | FIX-226 |
| EDGE-HST-005 | The hosted node answers 401 | The proxy must pass the status and body unchanged. | PROVEN: BRIEF no retries | FIX-221 |
| EDGE-HST-006 | The hosted node answers 429 with `Retry-After` | The proxy must pass the header. | PROVEN: S5 2b | FIX-225 |
| EDGE-HST-007 | The hosted node answers 529 with an `overloaded_error` body | The proxy must pass status and body unchanged. | PROVEN: both harnesses retry it (RND1 3.2) | FIX-221 |
| EDGE-HST-008 | The error body of a hosted node echoes a masked key | The proxy must write no key text to any log or admin output. | ASSUMPTION: APIs echo masked keys | FIX-227 |
| EDGE-HST-009 | A Messages request has `cache_control` markers | The proxy must forward the body byte for byte. | PROVEN: BRIEF | FIX-222 |
| EDGE-HST-010 | A pool holds a local node and a hosted node and the hosted conversation has its node removed | The proxy must move the conversation to the local node and log the move. | PROPOSED | FIX-223 |
| EDGE-HST-011 | REMOVED 2026-10-07 | The owner decided that a hosted node has no running cap in v1. The limits of the provider apply (DEC-050). | Decided by owner | none |
| EDGE-HST-012 | The TLS certificate of the hosted node is not valid | The proxy must answer 502 and must not retry over plain HTTP. | PROPOSED | FIX-221 |
| EDGE-HST-013 | The harness sends `x-session-affinity` to a hosted node route | The proxy must follow the route rule and the rule must be tested. | NOT TESTED: see OPEN-013 | FIX-220 |
| EDGE-HST-014 | Three users share one hosted key and one user sends many requests | The proxy must pass the hosted 429 to the user who triggers it and must log the share. | NOT TESTED: see RISK-024 | FIX-223 |
| EDGE-HST-015 | The hosted node answers with an SSE `ping` every 15 s | The proxy must pass the pings unchanged. | ASSUMPTION | FIX-141 |
| EDGE-HST-016 | The registry defines a hosted node and has no `client_tokens_ref` | The proxy must refuse to load the registry with the error `hosted_needs_client_tokens`. | Decided by owner (DEC-059) | FIX-301 |
| EDGE-HST-017 | The registry defines a hosted node and a request has no client token or a wrong one | The proxy must answer 401 on every alias, local or hosted, and must send nothing to a node. | Decided by owner (DEC-059) | FIX-302 |
| EDGE-HST-018 | The registry defines local nodes only and a request has no client token | The proxy must serve the request. | Decided by owner (DEC-059) | FIX-303 |

## Observability and admin

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-OBS-001 | An admin request has no token | The proxy must answer 401 and show no data. | PROVEN: BRIEF | FIX-240 |
| EDGE-OBS-002 | An admin request has a wrong token | The proxy must answer 401 with the same body as the no-token case. | PROPOSED | FIX-240 |
| EDGE-OBS-003 | The token is in the query string | The proxy must refuse it. | PROPOSED: URLs reach logs | FIX-240 |
| EDGE-OBS-004 | An admin path with an unknown version, `/legatus/v9/` | The proxy must answer 404. | PROPOSED | FIX-242 |
| EDGE-OBS-005 | A `POST`, `PUT` or `DELETE` on any admin path | The proxy must answer 405 and change no state. | PROVEN: v1 has no control actions (BRIEF) | FIX-250 |
| EDGE-OBS-006 | A session header holds the value `SECRET-SESSION-123` | The value must appear in no log line, admin answer or metric. | PROVEN: BRIEF privacy rule | FIX-241 |
| EDGE-OBS-007 | Two conversations in the admin read | The proxy must show two different non-reversible references and a harness label. | PROPOSED | FIX-241 |
| EDGE-OBS-008 | Prompt, tool result and system text with the marker `CANARY-PROMPT-7f3a` | The marker must appear in no log, admin answer, metric label or proxy error body. | PROVEN: BRIEF | FIX-247 |
| EDGE-OBS-009 | A request succeeds or fails, a harness cancels it while the proxy holds it, the hold limit ends it or it is malformed | The log must hold exactly one record for each request. | PROVEN: BRIEF invariant | FIX-246 |
| EDGE-OBS-010 | Free disk falls below the guard | The proxy must pause the log, write one "paused" marker and keep serving requests. | PROVEN: BRIEF guard | FIX-245 |
| EDGE-OBS-011 | Free disk rises above the guard again | The proxy must resume the log and write a "resumed" marker with the count of missed requests. | PROPOSED | FIX-245 |
| EDGE-OBS-012 | The log write stalls for 5 s | The proxy must not stall any stream. | PROPOSED. See OPEN-016 | FIX-245 |
| EDGE-OBS-013 | A header value holds a new line, a quote and a control character | The log must stay one valid JSON line per record. | PROVEN: log is JSON lines (BRIEF) | FIX-248 |
| EDGE-OBS-014 | The dashboard process stops | The proxy and the admin API must continue. | PROVEN: separate process (BRIEF) | FIX-249 |
| EDGE-OBS-015 | The first turn of each conversation | The cold-turn count must exclude first turns. | PROPOSED | FIX-116 |
| EDGE-OBS-016 | 50 admin reads in one second during load | The request latency of the proxy must stay within the passthrough figure. | NOT TESTED | FIX-243 |
| EDGE-OBS-017 | An admin read when a node has the running count equal to its cap | The admin answer must show the count, the cap and the held list length. | PROPOSED | FIX-243 |

## Harness behaviour

| ID | Situation | Expected proxy behaviour | Evidence or reason | Fixture |
|---|---|---|---|---|
| EDGE-HAR-001 | pi gets a 503 and retries at 2, 4 and 8 s | The proxy must treat each retry as a new request of the same key. | PROVEN: S5 | FIX-260 |
| EDGE-HAR-002 | pi sends a compaction call with no session header | The proxy must derive a key from the body and treat the request as a new conversation. | PROVEN: S1B 1d | FIX-261 |
| EDGE-HAR-003 | A 409 or 400 body holds the word "overloaded" | The test must fail the proxy build if a non-retry error holds a retry word. | PROVEN: S5 caveat | FIX-262 |
| EDGE-HAR-004 | The proxy holds a request 120 s, then 250 s | The pi stub must finish with no retry in both cases. | PROVEN: S5 row 4 and SD | FIX-271 |
| EDGE-HAR-005 | The proxy holds a request 249 s and then the node needs 60 s of prefill | The proxy must serve it, and the pi stub must time out at 299 s and the proxy must free the cap place. | PROVEN: pi cuts at 299 s to 300.6 s (S5, SD). The sum is RISK-005 | FIX-271 |
| EDGE-HAR-006 | The pi stub times out and retries 2 s later while the old request still waits | The proxy must remove the old held request and queue the retry. | PROVEN: S5 row 4 | FIX-125 |
| EDGE-HAR-007 | Claude Code sends a title request with a different `model` string | The proxy must route it by the alias table or answer 404 for an unknown alias. | PROVEN: Claude Code accepts any model string (RND1 3.6) | FIX-264 |
| EDGE-HAR-008 | Claude Code retries after a mid-stream error with a non-streaming request | The proxy must treat the retry as a new request of the same key. | PROVEN: RND1 3.2 | FIX-265 |
| EDGE-HAR-009 | Open WebUI sends 20 sub-agent requests with 20 chat ids | The proxy must hold them in order and show 20 conversations. | PROVEN for docs only (OWUI). NOT TESTED | FIX-266 |
| EDGE-HAR-010 | Open WebUI forwards `X-OpenWebUI-Chat-Id` on every turn | The proxy must use it as the key when the route key map names it. | PROVEN for docs only (OWUI) | FIX-267 |
| EDGE-HAR-011 | DeepSeek Harness sends no session header | The proxy must derive the key from the body. | PROVEN: RND1 3.6 and SC | FIX-268 |
| EDGE-HAR-012 | The user presses Escape and the harness closes the connection | The proxy must close the node connection and free the cap place when the node ends. | PROPOSED | FIX-269 |
| EDGE-HAR-013 | pi subagents send `parent#child` affinity values | The proxy must treat each value as its own key. | PROVEN: S1B 3g | FIX-270 |
| EDGE-HAR-014 | pi on the Messages path with default settings | The proxy must derive the key from the body because pi sends no header there. | PROVEN: S1B 2a' | FIX-106 |
| EDGE-HAR-015 | A harness changes its system prompt in the middle of a conversation | The proxy must keep the header key and the log must show the cold turn. | PROPOSED | FIX-102 |
| EDGE-HAR-016 | A retry repeats the same bytes while the first request still runs on a hung node | The proxy must send the retry to the same node and count two running requests. | PROVEN: pi retry returns to the same node (RND1 3.1) | FIX-128 |
| EDGE-HAR-017 | Each recorded harness replays through the proxy | The proxy must give the same status and bytes as a direct request to the stub node. | NOT TESTED: needs captures for Open WebUI and for the Codex cases that SC did not run | FIX-284 |
| EDGE-HAR-018 | Codex sends an environment item first in `input` | The proxy must give the key of the real prompt that follows. | PROVEN: SC | FIX-308 |
| EDGE-HAR-019 | opencode and DeepSeek Harness send a title call at session start | The proxy must not count the request as a second conversation. | PROVEN: both send a request with its own system prompt (SC, SD) | FIX-311 |
| EDGE-HAR-020 | Claude Code in the TUI sends a title call with the session header | The proxy must give the request the key of the conversation. | PROVEN: SC | FIX-311 |
| EDGE-HAR-021 | The proxy holds a request of each harness of the matrix with no byte until the harness gives up | The stub must record the give-up time. The times are in table 8.1 of [04](04-admission-control-and-queueing.md): 299 s to 600 s, and never for curl and Codex. | PROVEN: SD, table 1 | FIX-316 |
| EDGE-HAR-022 | Claude Code runs with `API_TIMEOUT_MS=1800000` | The proxy must keep the hold limit below 290 s. The setting does not raise the wait. | PROVEN: SD. Cause NOT TESTED | FIX-316 |
| EDGE-HAR-023 | A 503 refusal carries `Retry-After` of 120 s | The test must fail the build. Claude Code fails at once. pi, DeepSeek Harness and opencode ignore the value. | PROVEN: SD | FIX-313 |
| EDGE-HAR-024 | A refusal on status 409 has an OpenAI body of type `server_error` | The pi and opencode stubs retry. The proxy must not send such a body for a refusal that must stop. | PROVEN: SD | FIX-314 |
| EDGE-HAR-025 | A 429 refusal reaches a Codex stub | The stub fails at once. The proxy must not use 429. | PROVEN: SD | FIX-307 |
| EDGE-HAR-026 | Each harness retries after a 503 | The retry must have the same body bytes and the same session header as the first attempt. | PROVEN: SC | FIX-312 |
| EDGE-HAR-027 | A harness of the matrix loses the connection at 5 s | The harness must retry. The attempts are 3, 4, 7, 9, 30 and 11. curl and `fetch` fail at once. | PROVEN: SD | FIX-316 |
| EDGE-HAR-028 | Codex sends the full history every turn with `store: false` | The proxy must key the conversation and must not rely on server state. | PROVEN against a fake (SC) and by source (R7) | FIX-326 |
| EDGE-HAR-029 | A Codex subagent sends its own `thread-id` | The proxy must follow the key order of PRX-KEY-055. If the subagent shares the `session-id`, parent and child share a key. | NOT TESTED (OPEN-042) | FIX-339 |
| EDGE-HAR-030 | Codex compaction raises the window number of `x-codex-window-id` | The proxy must keep the key and must log the next turn as cold. | ASSUMPTION from R7 (compaction not captured) | FIX-339 |

## Coverage and gaps

The table counts the rows of each area. The total is 293 rows. A row marked REMOVED counts.

| Area | Rows |
|---|---|
| KEY | 39 |
| AFF | 25 |
| ADM | 40 |
| PRO | 41 |
| STR | 23 |
| ENG | 18 |
| CAC | 27 |
| RST | 15 |
| HST | 18 |
| OBS | 17 |
| HAR | 30 |

Known gaps. These situations have no row yet because the evidence does not exist.

- Inbound TLS on the model port. The client token rule is in EDGE-HST-016 to EDGE-HST-018.
- HTTP/2 clients.
- Requests with `n` greater than 1.
- The behaviour of the proxy under memory pressure on a real machine.
- Linux and CUDA engine timings.

## Sources

- [Spike s2 report](evidence/spikes/s2-paused-time-README.md) for time rules.
- [Spike s3 report](evidence/spikes/s3-slots-README.md) for engine concurrency and queue facts.
- [Spike C report](evidence/spikes/sC-key-stability/README.md) for key stability facts.
- [Spike D report](evidence/spikes/sD-hold-tolerance/README.md) for hold, status and retry facts.
- [Spike s5 report](evidence/spikes/s5-pi-restart-README.md) for pi retry and hold facts.
- [Spike s1b report](evidence/spikes/s1b-pi-lease-gaps-README.md) for header facts.
- [Spike s6 notes](evidence/spikes/s6-checkpoint-source-README.md) for checkpoint policies.
- [Spike 7 report](evidence/spikes/s7-hybrid-cache/README.md) for measured hybrid cache behaviour.
- [Research r7](evidence/research/r7-responses-api-20261007T081115Z.md) for the Responses API and Codex.
- [Round 1 decisions](../decisions/2026-10-spike-decisions.md) for engine, router and harness facts.
- Research notes r1 to r7 in the folder [evidence/research](evidence/research/r4-local-engines-20261007T022141Z.md).
- [Open WebUI notes](../../docs/horizon/openwebui.md).
