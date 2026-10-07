# Test fixtures and scenarios

Status: draft for review, 2026-10-07. Part of the Legatus proxy baseline. This file defines the simulated cluster, the invariants, the recorded captures, the scenario catalogue and the benchmark method.

## Purpose and scope

The proxy is a reverse proxy with soft state. Most of its risk is in time, order and engine behaviour. Real engines are slow, noisy and hard to fault. The test design therefore has three layers.

1. Virtual mode: the real proxy code on paused time with stub engines in memory. It proves logic and boundaries.
2. Scaled mode: the real proxy binary on real sockets with a compressed clock. It proves the wire path.
3. Real mode: real engines and real harnesses at pinned versions. It proves that the stubs are honest and that the claims hold.

Stub engines copy measured behaviour from the spikes. Recorded captures keep the stubs honest. The edge cases that use these fixtures are in [edge cases](12-edge-cases.md). The open items are in [open questions and risks](14-open-questions-and-risks.md).

Labels in this file: PROVEN (measured or read from source, with a reference), PROPOSED (a design choice), ASSUMPTION (believed, not checked), NOT TESTED. Spike s7 (S7, hybrid cache measurement) reported on 2026-10-07 for llama-server b11459 and 0.5.0 on Qwen3.5-2B Q4_K_M and a dense control. Its token counts are PROVEN for that model. Its times are NOT TRANSFERABLE. Spike F (real Codex conversations on real engines) has not run: owner decision pending.

## Test modes and what each proves

| Mode | Clock | Transport | Proves | Cannot prove |
|---|---|---|---|---|
| Virtual | Paused tokio time, one thread | In-memory duplex streams | Exact instants and boundaries, same-instant order, forced interleavings at named gaps, thousands of seeds in milliseconds, replay by seed | Real sockets, reset and half-close, backpressure of the OS, fsync, real signals, sub-millisecond order, real engine timing |
| Scaled | Real clock, durations divided by a factor | Real sockets and real processes | The wire path, real HTTP parsing, SIGKILL and restart, real file input and output, byte identity over sockets | Exact order, exact boundaries, durations below timer jitter, production values |
| Real | Real clock | Real engines and harnesses | Engine behaviour, harness behaviour, the benchmark claims, stub honesty | Repeatable timing (the host noise was 2 times between runs in s3) |

Evidence: [spike s2](evidence/spikes/s2-paused-time-README.md) measured the virtual and scaled modes. In scaled mode the expiry lateness was at most 11 ms with a 10 ms sweep. A same-instant race split 25 to 15 over 40 trials, so scaled tests must use bands and never exact order. Real sockets in virtual mode made the clock jump in 300 of 300 requests, so the no-socket rule is mandatory.

## Seams the proxy must expose

These requirements come from [spike s2](evidence/spikes/s2-paused-time-README.md). Each is PROVEN there on a prototype and NOT TESTED on the Rust proxy.

| ID | Requirement |
|---|---|
| PRX-TEST-001 | The proxy must read monotonic time only through `tokio::time::Instant`. |
| PRX-TEST-002 | The build must forbid `std::time::Instant`, `std::thread::sleep` and `SystemTime` in proxy code, with a lint rule. |
| PRX-TEST-003 | The proxy must reach wall time through one `WallClock` trait. |
| PRX-TEST-004 | The proxy must call nodes through one `UpstreamTransport` trait with one method: send a request and return status, headers and a body stream. |
| PRX-TEST-005 | The proxy library must expose a function that builds the request router and must not own the listener. |
| PRX-TEST-006 | The proxy must write log lines through a `LogSink` trait that a test can run in the same task. |
| PRX-TEST-007 | The proxy must spawn background tasks on the current runtime and return their handles. |
| PRX-TEST-008 | The proxy must visit tasks in sorted key order, never in hash map order. |
| PRX-TEST-009 | The proxy must mark the gaps that matter with named `sim_point` calls that cost nothing when unarmed. |
| PRX-TEST-010 | The proxy library path used by tests must not bind a socket or read a file. |
| PRX-TEST-011 | A virtual test must move time with a driver `sleep` and not with one `advance` call. |
| PRX-TEST-012 | A virtual test must use whole-millisecond offsets to order events. |
| PRX-TEST-013 | A virtual test must not use real sockets, `spawn_blocking` or `tokio::fs`. |

Reasons. One `advance` call is a single jump. In spike s2, a timer touched every 600 s with an idle limit of 1800 s expired wrongly after `advance(3600 s)`. The 1800 s is the spike value and not the table expiry of the proxy.

Timer resolution is 1 ms and deadlines round up. Offsets from 0 to 50 ns gave an unordered result. An armed point holds every task that reaches it, so a gate needs "arm after N reaches". The named gaps for the proxy are PROPOSED: `table_lookup`, `admit_decide`, `slot_free`, `hold_expire`, `stream_end`, `log_append`, `sweep_after_due` and `reload_apply`.

## Stub engines

A stub engine is a program in the test kit that speaks the HTTP of one engine and has configurable behaviour. Each stub runs in memory in virtual mode and as a process in scaled mode. A stub never runs a model. It models time and cache with the rules below.

| ID | Stub | Behaviour to copy | Main configuration | Evidence |
|---|---|---|---|---|
| FIX-001 | Serial Ollama | One slot by default. A FIFO queue. Headers sent only when service starts. No error up to 8 queued. No load field except `/api/ps` (loaded model, memory, expiry). Silent truncation of a prompt above about half the loaded context with status 200. Model unload after keep-alive | `num_parallel`, `ctx`, `max_queue`, `keep_alive`, `truncate_ratio`, speed model | PROVEN [S3](evidence/spikes/s3-slots-README.md), RND1 3.5. Queue overflow 503 is documented only (R4) |
| FIX-002 | Multi-slot llama-server | `total_slots` from `/props`. Queued requests get headers early. `/metrics` with `requests_processing` and `requests_deferred` when enabled. `/slots` with `is_processing` and `n_ctx`. Per-slot context equals `c / np` with an explicit `-np`. 400 `exceed_context_size_error`. 500 on malformed JSON. Keep-alive close after 5 s idle. `--sleep` mode: `/slots` wakes it, `/metrics`, `/health` and `/props` do not. Slot choice by prefix similarity above 0.1, else least recently used | `slots`, `ctx`, `kv_unified`, `metrics_on`, `sleep_on`, `cache_policy`, `cache_ram_mib` (0 turns the host cache off), `checkpoint_min_step`, `ctx_checkpoints` | PROVEN S3, RND1 3.5, [S6 llama.cpp](evidence/spikes/notes-llamacpp-20261007T054753Z.md), [S7](evidence/spikes/s7-hybrid-cache/README.md) |
| FIX-003 | Batching mlx_lm | All requests admitted up to the decode concurrency. Headers at once. No `/props`, `/metrics` or `/slots` (404). Aggregate speed flat, so each request gets 1/N of the speed. Drops the connection on an invalid request. Does not honour a cancelled prefill. Prompt cache limit of 10 entries. A wedge fault | `decode_concurrency`, `prompt_concurrency`, `cache_entries`, `wedge_after` | PROVEN S3 finding 4, RND1 3.5, [S6 mlx](evidence/spikes/notes-mlx-20261007T054857Z.md). Wedge seen once |
| FIX-004 | Hybrid llama.cpp | Checkpoint policy of the table below. With `cache_ram_mib` above 0 an idle slot state goes to the host cache and a second conversation stays warm. With 0 every turn of a second conversation is cold | Policy constants, `cache_ram_mib` | PROVEN S6 and S7 (Qwen3.5-2B, b11459 and b11146) |
| FIX-005 | Hybrid vLLM align | Checkpoint policy of the table below. `cached_tokens` only with the details flag | `block_size`, `details_flag`, `retention_interval` | PROVEN S6 |
| FIX-006 | Hybrid SGLang | Checkpoint policy of the table below | `grid`, `track_interval`, `strategy` | PROVEN by source, inferred closed form (S6) |
| FIX-007 | Hybrid gufo | Checkpoint policy of the table below. Fields `timings.cache_n` and `prompt_n` | `grid`, `max_entries` | PROVEN by source and docs (S6) |
| FIX-008 | Hybrid mlx-lm | Checkpoint policy of the table below | `cache_entries` | PROVEN S6 |
| FIX-009 | Hosted Messages node | Messages SSE with `message_start`, `ping`, deltas and `message_stop`. Accepts only the test key. Records every header it received. Scripted 401, 429 with `Retry-After`, 529 with `overloaded_error` as status or as a mid-stream event. A prompt-cache time model with a time to live | `key`, `ttl`, `error_script`, `rate_limit` | PROVEN for the error behaviour (S5, RND1 3.2). TTL anchors from R3 |
| FIX-010 | Dense control | Exact trimmable prompt cache, so reuse equals the common prefix. Used as the control for hybrid stubs | `capacity_tokens` | PROPOSED |
| FIX-011 | Embeddings node | `/v1/embeddings` with fixed vectors | `dims` | ASSUMPTION |
| FIX-012 | Node agent | Reports health, load and machine readings (temperature, thermal throttling, power) on a script | `readings_script` | PROPOSED |
| FIX-013 | Responses node, Codex-ready | `POST /v1/responses`. Accepts a body of 20 to 40 KB with unknown fields, `store: false` and `reasoning` items with `summary: null`. Streams `response.created`, `response.output_item.added`, `response.output_item.done` with full items, `response.completed` with optional `usage`. Sends `x-codex-turn-state` on a script. Closes the stream after `response.completed` | `emit_usage`, `turn_state`, `idle_gap`, item script | PROVEN as the minimum list of [R7](evidence/research/r7-responses-api-20261007T081115Z.md) section 5.5 (source and fake runs) |
| FIX-014 | Responses node, stateful or ignoring | Mode `stateful`: keeps `response.id` and answers 404 for an unknown `previous_response_id`. Mode `ignores`: accepts `previous_response_id` and ignores it (Ollama). Mode `rejects`: 400 (llama-server) | `mode` | PROVEN as engine behaviour by source (R7). NOT TESTED live |
| FIX-015 | Node with no Responses path | Answers 404 on `/v1/responses` (mlx_lm.server) | none | PROVEN by source (R7) |

### Speed models

A speed model gives the time of a request from its token counts and the number of running requests. Its data come from [spike s3](evidence/spikes/s3-slots-README.md), measured on an M2 with 16 GB that other agents shared. The run-to-run noise was 2 times, so a test must use shapes and ratios.

- Prefill is one shared pool. With a rate of R tokens per second and N prompts in prefill, each prompt gets a rate of R divided by N. PROVEN S3 finding 7 (R was 250 to 270 here).
- The first token time for a request is the queue wait plus the recompute tokens divided by the prefill rate.
- Decode uses an aggregate curve. The per-request rate is the aggregate divided by N.
- The prefill rate can fall with context length. The stub takes a table of points and interpolates. The points below come from R4. They are second-hand figures from the owner's repo.
  - Strix Halo llama.cpp: 351 tok/s at 2k tokens and 172 tok/s at 120k tokens.
  - gufo on ROCm: 1228 to 1266 tok/s from 32k to 120k tokens.
  - 4090 llama.cpp: 1870 to 2220 tok/s.
  - M5 Max: 1290 to 1420 tok/s.

| Aggregate decode tok/s, decode-bound | N=1 | N=2 | N=4 | N=8 |
|---|---|---|---|---|
| llama-server 4 slots | 28 | 34 | 46 | 49 |
| Ollama 4 parallel | 27 | 33 | 43 | 49 |
| llama-server 1 slot | 34 | 25 | 27 | 25 |
| Ollama default | 49 | 27 | 24 | 25 |
| mlx_lm | 55 | 58 | 53 | 51 |

### Cache stubs and the oracle

A cache stub keeps, for each stored sequence, the token ids and the set C of retained state positions. A request has N tokens and a common prefix D with the stored sequence of length L plus G generated tokens. The closed form of every policy is `recompute(D) = N - max{c in C : c <= D}`, and `N` if C has no such member ([spike s6](evidence/spikes/s6-checkpoint-source-README.md)). Conversation generators build token ids so that D is exact.

| Stub | Set C (constants from source) | Reuse field |
|---|---|---|
| llama.cpp | L-516 and L-4. On the chat route also the first user-message start (133 tokens in S7) and the last user-message start. Other user-message starts only at least 8192 after the previous checkpoint, at most 32 kept. The live state L+G only while the slot lives. No checkpoint after generation. The engine removes later checkpoints after a divergence. PROVEN by S7 | `timings.cache_n`, `prompt_n`. `usage.prompt_tokens_details.cached_tokens` equals `cache_n` (S7) |
| vLLM align | One state per resident prompt at `floor((P-1)/B)*B`, with B from 528 to 2096. Junction states. Nothing for generated tokens. A miss in the state group vetoes the whole hit | `prompt_tokens_details.cached_tokens` only with the flag |
| SGLang | Chunk-end grid points, decode multiples of 256, branch points, the final donated state. Eviction frees interior states first | `prompt_tokens_details.cached_tokens` |
| gufo | At most 4 grid points on 2048, a stable boundary, the full prompt, learned branches, the live frontier. 128 entries | `timings.cache_n`, `prompt_n` |
| mlx-lm | System end, user-segment end (about 11 tokens before the end), the finished sequence. 10 entries, "assistant" entries evicted first | `prompt_tokens_details.cached_tokens` |

PRX-TEST-014: The test kit must hold an oracle for each cache stub that computes `recompute(D)` from the stored sequence alone.

PRX-TEST-015: A property test must compare every stub reply with its oracle on at least 10000 seeded requests.

PRX-TEST-016: A conversation generator must support a mode that rewrites the last answer, so that D falls below L+G.

PRX-TEST-077: The oracle of the llama.cpp stub must reproduce the sweep tables of [S7](evidence/spikes/s7-hybrid-cache/README.md).

At N = 6000, the values D = 100, 5000, 5480 and 5483 give 6000. The values D = 5484, 5490 and 5900 give 516. The value D = 5997 gives 4.

At N = 12000, the values D = 8200, 11480 and 11483 give 12000. The values D = 11484 and 11490 give 516. The value D = 11997 gives 4. The dense oracle must give `N - D`.

The vLLM block sizes and the SGLang defaults come from a source read at a given commit. They can change. The stubs must take them as configuration with the commit recorded.

## Harness scripts

| ID | Script | Behaviour |
|---|---|---|
| FIX-020 | pi harness | Sends `x-session-affinity` on the OpenAI path only. Retries 3 times at 2, 4 and 8 s on connection errors and on the errors whose status and body text match its pattern. Ignores `Retry-After`. Gives up at 299 s with no byte. Compaction calls have no header. PROVEN S5, SD |
| FIX-021 | Claude Code harness | Messages only. Sends `x-claude-code-session-id`, an agent id, 11 beta headers and mid-conversation system messages. Falls back to a non-streaming request after an `api_error`. Retries on `overloaded_error` and on a dropped connection. Gives up at 360 s with no byte. Honours `Retry-After` to 60 s and fails at once at 90 s. Adds `x-claude-code-parent-agent-id` for a nested child. PROVEN RND1 3.6, SC, SD |
| FIX-022 | Open WebUI harness | Sends `X-OpenWebUI-Chat-Id` when enabled. Starts up to 20 sub-agent requests at once with the parent model. Docs only (OWUI) |
| FIX-023 | Conversation generator | Grows a history by turns with tool calls, side requests (title, summary), compaction, cancel and edit modes. Seeded |
| FIX-024 | Headerless harness | Sends no session header, as DeepSeek Harness does (RND1 3.6) |
| FIX-025 | Raw HTTP client | Sends malformed, chunked, `Expect`, oversized and HTTP/1.0 requests |
| FIX-324 | opencode harness | Sends `x-session-affinity` and `x-session-id` with one `ses_` value. Sends a title call and the main request at session start. Gives up at 300 s with no byte. Makes 9 attempts. Ignores `Retry-After`. Retries on error text, also on 400 and 409 with an OpenAI body. PROVEN SC, SD |
| FIX-325 | DeepSeek Harness | Sends no session header. Sends a title call and the main request at session start. Gives up at 299 s, also with keep-alive. Makes 7 attempts. Ignores `Retry-After`. Does not retry 408 on the OpenAI path. Fails with no retry after an OpenAI-style error event. PROVEN SC, SD |
| FIX-326 | Codex harness | Responses API. Sends `session-id`, `thread-id`, `x-codex-window-id` and `prompt_cache_key`. Sends an environment item first in `input`. Never gives up on a hold. Fails at once on 429. Makes 30 requests. Needs a real SSE event to keep alive. PROVEN SC, SD |
| FIX-327 | SDK clients | openai-node 7.30.0, anthropic-node 0.131.0, openai-python 3.26.0, anthropic-python 1.11.0. Give up at 301 s (Node) and 600 s (Python). Make 3 attempts. Honour `Retry-After` as the table in 04 says. PROVEN SD |

## Fault scripts

A fault script is a list of rules of the form "when this request matches, do this". Each rule has a count or a time window. A script is data and a seed can choose between rules.

| ID | Fault | Behaviour |
|---|---|---|
| FIX-030 | Refuse | The node does not listen for a time window |
| FIX-031 | Hang | The node accepts and sends nothing |
| FIX-032 | Mid-stream drop | The node sends N chunks and closes. A variant ends cleanly without the end marker |
| FIX-033 | Slow first byte | The first byte comes after 5, 20, 60 or 120 s |
| FIX-034 | Status script | The node answers a chosen status and body, with or without `Retry-After` |
| FIX-035 | Empty 200 | The node answers 200 with empty content |
| FIX-036 | Reset at accept | The node resets the connection once or always |
| FIX-037 | Stale keep-alive | The node closes an idle connection after 5 s |
| FIX-038 | Log sink fault | Free disk falls below the guard, a write stalls or a write fails |
| FIX-039 | Clock jump | The wall clock jumps back and forward. The monotonic clock does not |

## Invariants

The harness must check each invariant after every step of every scenario in virtual and scaled mode. A violation ends the run and prints the seed.

| ID | Invariant |
|---|---|
| PRX-TEST-020 | The proxy must never have more running requests on a node than the cap of the node. |
| PRX-TEST-021 | A conversation key must never have running requests on two nodes at the same instant. |
| PRX-TEST-022 | The proxy must answer a held request exactly once. |
| PRX-TEST-023 | The table must never hold more entries than its cap. |
| PRX-TEST-024 | The log must hold exactly one record for each request. |
| PRX-TEST-025 | No log line, admin answer, metric or proxy error body must hold prompt text. |
| PRX-TEST-026 | The body sent to a node must equal the body received, except the `model` value and the patch of the node. |
| PRX-TEST-027 | The running count of every node must be zero when no request is open. |
| PRX-TEST-028 | A held request must leave the queue by the hold limit plus 1 ms. |
| PRX-TEST-029 | A conversation must keep its seat while it is idle for no longer than the window, unless its node is unavailable. |
| PRX-TEST-030 | No request to a hosted node must carry an inbound credential or an `x-legatus-*` header. |
| PRX-TEST-031 | A conversation must move only when its node is unavailable. |
| PRX-TEST-032 | The proxy must not forward a request for an alias to a node outside that alias. |

The planted markers are `CANARY-PROMPT-7f3a` in prompts and tool results, `SECRET-SESSION-123` in session values and a fake key `sk-test-0000` in the hosted node. PRX-TEST-025 checks all three in every output.

## Seeded schedules

A seed chooses the arrival times, the conversation shapes, the fault points and the order of events inside one millisecond window. The run is a function of the seed.

PRX-TEST-040: The harness must print the seed of every failed run and must replay the run from the seed with identical logs.

PRX-TEST-041: The harness must draw event offsets from whole milliseconds, 0 to 7 ms, to order competing events. Spike s2 showed that three actors all win across seeds with this range.

PRX-TEST-042: The harness must report a run with an unreleased gate as "stalled", not as a hang.

PRX-TEST-043: The continuous integration run must use at least 1000 seeds for each concurrency scenario. PROPOSED. The nightly run uses at least 100000.

In the s2 prototype, 300 seeds ran in 65 ms and the winners were split `{A:117, B:116, C:67}`. The proxy numbers are NOT TESTED.

## Recorded captures

A capture is a recording of real traffic that a stub or a replay test uses. Captures keep stubs honest. A stub that disagrees with its capture is wrong until proven otherwise.

### Engine captures

| ID | Engine | Record |
|---|---|---|
| FIX-040 | Ollama | Queue depth 1, 2, 4 and 8, header and first-token times, a truncated prompt, `/api/ps` before and during, unload and reload |
| FIX-041 | llama-server | `/props`, `/metrics`, `/slots` at rest and under load, a queued request, a context overflow, a malformed body, sleep and wake, hybrid turns with `timings` |
| FIX-042 | mlx_lm | Batched requests at N from 1 to 8, an invalid request, a cancel in prefill, the cache log line |
| FIX-043 | vLLM | Hybrid turns with and without the details flag, metrics for prefix hits |
| FIX-044 | SGLang | Hybrid turns with `cached_tokens` over a long conversation |
| FIX-045 | gufo | Turns with `timings`, a divergence, an edited message |
| FIX-332 | llama-server, streamed | A streamed hybrid turn with `stream_options.include_usage`. Record whether the terminal chunk carries `timings` and `cached_tokens`. S7 checked the non-stream shape only |
| FIX-049 | Hosted Messages | A stream, each error status, headers on the wire. Use a low-cost model |

For each engine the capture lists the engine version, the flags, the model, the quantisation and the machine. It lists the prompt tokens, the reply tokens and the cache fields of each turn. It also holds the per-chunk times, the response headers and the sampled load endpoints.

### Harness captures

| ID | Harness | Record |
|---|---|---|
| FIX-046 | pi 1.0.3 | Header set and order, a long agent loop, compaction, a subagent, a cancel, each retry |
| FIX-047 | Claude Code | The same, with a title request, a subagent and the non-streaming fallback |
| FIX-048 | Open WebUI | A conversation, a title task, a 20-way delegation, the chat id header |
| FIX-050 | opencode | The same list |
| FIX-051 | DeepSeek Harness | The same list |
| FIX-328 | Codex 0.160.1 | Plain, two-session and resume runs exist (SC). Collect compaction, a subagent, a retry and a long loop |
| FIX-329 | SDK clients | Chat and Messages shapes of the four SDKs, with the retry count header |

Each harness capture lists the version, the settings, the request headers and the body structure. It lists the size of each turn in tokens and the times between requests. The capture must show whether the first system message and the first user message stay equal across turns.

### Codex fixtures to reuse

Source: [R7](evidence/research/r7-responses-api-20261007T081115Z.md) section 5.4, read at tag rust-v0.160.1. The Codex repository is under the Apache License 2.0. The proxy kit ports the JSON shapes and the behaviour descriptions. It does not link the Rust crates and does not copy code. Copied code needs the licence text, the copyright header and a note of changes.

| Source file in the Codex repository | Use for the stub kit |
|---|---|
| `codex-rs/core/tests/common/responses.rs` | The SSE builders: `sse(events)`, `ev_response_created`, `ev_completed`, `ev_completed_with_tokens`, `ev_assistant_message`, `ev_output_text_delta`, `ev_reasoning_item` (with a base64 `encrypted_content` of 550 bytes or more), `ev_function_call`, `ev_custom_tool_call`, `ev_apply_patch_custom_tool_call`, `sse_failed`. Port the JSON shapes into FIX-013 |
| `codex-rs/core/tests/common/streaming_sse.rs` | A raw TCP SSE server with a gate before each chunk. Use the idea for the hold and idle fixtures (FIX-316, FIX-319) |
| `core/tests/suite/stream_no_completed.rs` | A scenario description: an early close then a retry (FIX-319) |
| `core/tests/suite/retry_after.rs`, `stream_error_allows_next_turn.rs`, `cli_stream.rs` | Scenario descriptions for `Retry-After` and a stream error |

PRX-TEST-078: The stub kit must take Responses event shapes from the Codex test shapes in this table. The kit must record the Codex tag and the licence.

PRX-TEST-079: The kit must run the real Codex 0.160.1 against the stub as a contract test, as spikes C and D did.

PRX-TEST-080: A real-mode test must run real Codex conversations against an engine before the project claims Codex support for it. The conversations need tool loops, a freeform tool call and compaction (FIX-331). Spike F is that test. The owner decision is pending.

### Redaction rule

PRX-TEST-050: A capture must keep structure, lengths and token counts and must remove all text.

PRX-TEST-051: The redaction tool must replace each text value with a filler of the same byte length. Text values are prompts, completions, tool definitions, tool arguments and tool results.

PRX-TEST-052: The tool must map equal strings to equal fillers with a keyed hash. It must map different strings to different fillers, so key tests still work.

PRX-TEST-053: The tool must replace each credential, cookie, session value and user name with a keyed token that keeps equality.

PRX-TEST-054: The tool must discard the hash key after each run.

PRX-TEST-055: A scan must fail the commit of a capture if any string of 16 or more characters from the source appears in the output. The length 16 is PROPOSED.

PRX-TEST-056: Nobody must commit an unredacted capture.

Redacted fillers change the token count if the engine tokenises them. So the capture keeps the original token counts from the usage fields, and a stub uses those counts and never tokenises.

## Tests from lessons

Each requirement below comes from a lesson in [11-lessons-learned.md](11-lessons-learned.md).

| ID | Requirement |
|---|---|
| PRX-TEST-081 | A real-engine test on a node with a ROCm runtime must send two different conversations to one slot. The test must compare the second reply with a reply from a fresh slot, so that carried state shows. |
| PRX-TEST-082 | A test must prove a cache hit from the cache fields of the response and never from a comparison of output text. |
| PRX-TEST-083 | A stub engine must close an idle connection after a set time, with the default 5 s. A test must show that the proxy opens a new connection after 4 s of idle time. |
| PRX-TEST-084 | A real-engine test must check that `content` holds no thinking text when the node patch turns thinking off. A test must not judge a reply by the status code alone. |
| PRX-TEST-085 | A key test must show that a system message after the first non-system message does not change the key. |
| PRX-TEST-086 | The test kit must write each run and each capture to a new file with a timestamp in the name. It must refuse to truncate an existing file. |
| PRX-TEST-087 | The test kit must give a `sim_point` gate an option to arm after N reaches, because an armed point holds every task that reaches it. |

## Scenario catalogue

Mode: V means virtual time, S means scaled time, R means real time. A scenario passes only if all invariants hold. The pass condition names the extra checks. The edge case rows that use each scenario are in [edge cases](12-edge-cases.md).

### Keys

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-100 | Header precedence | Send requests with one, two and no key headers, in the route key map order | The key equals the first listed header present | V |
| FIX-101 | Derivation vectors | At least 30 bodies: no user, no system, multimodal, string versus array, escapes, key order, long text | Each body gives its expected key and equal bodies give equal keys | V |
| FIX-102 | Key stability | 30 turns with growth, then variants: changing system prompt, time stamp, reused header | The key is equal for each stable run and the churn count is right | V |
| FIX-103 | Side request keys | Main conversation plus title and summary requests | Side requests have other keys | V |
| FIX-104 | Route key maps | Maps for Claude Code, Open WebUI, the OpenAI `user` field | The key follows the configured map | V |
| FIX-105 | Messages system field | String, list and `cache_control` forms, mid-conversation system messages | Key ignores the markers and the bytes pass unchanged | V |
| FIX-106 | Header faults | Empty, 10 KiB, case, repeated, spaces, Anthropic path without header | Results as in the edge rows | V |
| FIX-107 | Key scope | Same value under two aliases and two credentials | Two entries for two aliases. Credential rule as configured | V |
| FIX-108 | Capture key check | Replay FIX-046 to FIX-051 through the key function | One key for each recorded conversation | V |
| FIX-305 | Key vectors of spike C | The 19 vectors of [the vector file](evidence/spikes/sC-key-stability/test-vectors-keyv1-20261007T065515Z.json) with both text limits at 8192 | Each canonical string equals `expected_canonical`. Each `same_key_as` pair gives equal keys. The other vectors give distinct keys | V |
| FIX-308 | Codex environment item | Two Codex conversations in one directory with different prompts (vectors V14 and V15) | Different keys. Same key for later turns (V16) | V |
| FIX-309 | Billing block | Claude Code first and later turns with different `x-anthropic-billing-header:` values (V10, V11) | Equal keys. After compaction (V12) a new key | V |
| FIX-310 | Same first prompt | Two conversations with one first prompt, with and without a session header | One key without a header. Two keys with a header | V |
| FIX-312 | Identical retry | Each harness of the matrix retries after a 503 | The body bytes and the session header equal the first attempt. The key is equal | V |
| FIX-317 | Compaction | Recorded compaction of pi, Claude Code, opencode and DeepSeek Harness | The derived key changes. A header key stays | V |
| FIX-320 | System text limit | Two system prompts that differ only at byte 17198, and only at byte 40000 | Two keys for the first pair at the default limit. One key for the second pair | V |
| FIX-321 | Claude Code agent ids | A parent, a child and a nested child with the three headers | Three keys. The parent agent id is not in the key | V |
| FIX-322 | Responses key | `instructions` with a list `input` and with a string `input`. Header `session-id` against `prompt_cache_key` | The list and the string give one key. The header wins | V |

#### Key vectors of spike C

The test suite keeps the 19 vectors of spike C as a fixture (FIX-305). The vectors are in [the vector file](evidence/spikes/sC-key-stability/test-vectors-keyv1-20261007T065515Z.json). The reference code is [keyv1.py](evidence/spikes/sC-key-stability/keyv1.py). The vectors V01 to V17b cover white space, parts against strings and the developer role. They also cover NFC, the billing block and mid-array system messages. The last cases are thinking blocks, compaction, Codex and the text limit.

PRX-TEST-071: The test suite must run the 19 vectors of FIX-305 on the key function. It must fail the build when one vector fails.

PRX-TEST-072: The test must compare the canonical string of each vector with `expected_canonical` and not only the final key.

PRX-TEST-073: The test must set `key_text_limit_system` and `key_text_limit_first` to 8192 bytes for the 19 vectors. Spike C made the vectors V17 and V17b with that value.

PRX-TEST-074: The test must check the default limit of 32768 bytes with FIX-320 in a separate case.

PRX-TEST-075: The test must check that each pair with a `same_key_as` field gives equal keys and that every other vector gives a distinct key.

PRX-TEST-076: The test suite must run FIX-306, FIX-307, FIX-313 and FIX-316 with the harness scripts of every harness. A harness is PROVEN only after these cases pass.

### Affinity

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-110 | Table basics | Place, hit, refresh, failed request | Last-seen times follow the rules | V |
| FIX-111 | Strictness | Node of the conversation busy, then down | Hold while busy and move only when down | V |
| FIX-112 | Table cap | Churn of 10 times the cap in keys | Size never exceeds the cap and running entries stay | V |
| FIX-113 | Node removal | Remove a node by reload with entries and a running request | No panic. The entries become unavailable | V |
| FIX-114 | Node return | Down, move, up | The key stays on the new node | V |
| FIX-115 | Reuse feedback | A stub with good then poor reuse, no fields, oscillation | Mode follows the rule with the hysteresis | V |
| FIX-116 | Cold turns | Moves, engine eviction, first turns | Reasons and counts are right | V |
| FIX-117 | Expiry boundary | Requests at limit minus 1, equal to and plus 1 ms, with a sweeper in the same ms | Results as in the edge rows and equal on 20 runs | V |
| FIX-118 | Alias change | Remove a node from an alias | Entries become unavailable for that alias | V |
| FIX-119 | Shared node | One node in two aliases | One cap applies | V |

### Admission and hold

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-120 | Cap by engine kind | Stubs FIX-001, 002, 003 with 2 times the cap in requests | Running never exceeds the cap | V, S |
| FIX-121 | Window boundary | Idle for 179999, 180000 and 180001 ms | Protect up to 180000 ms and allow takeover after | V |
| FIX-122 | Placement order | Free warm, free cold, idle beyond window, all protected | The order of the rule | V |
| FIX-123 | Hold limit | One cap place, a held request, no free cap place for 250 s | One error at the limit and the count unchanged | V |
| FIX-124 | Hold race | Free at limit minus 1, limit, limit plus 1 ms | Served, served, error. Exactly one answer | V |
| FIX-125 | Held disconnect | Disconnect while held and at the assign instant | No slot taken or leaked. One record | V, S |
| FIX-126 | Queue order | Three held requests, with and without a table entry | Oldest eligible first. No other rule orders the queue | V |
| FIX-127 | Fan-out | 20 children on 1 and 4 slots, parents idle | No starvation. Holds end by the window plus the longest run | V |
| FIX-128 | Same conversation twice | Two requests of one key at once | Both count. One node | V |
| FIX-129 | Cap changes | Reload up and down. `/props` lower than declared | Rules of the edge rows | V |
| FIX-130 | All unavailable | All nodes down while requests wait | Hold until return or the limit | V |
| FIX-131 | Node of the conversation returns | Return during a hold | Request starts on the node | V |
| FIX-132 | Hold bound | 5000 held requests | Memory bounded. Overflow errors at once | V, S |
| FIX-133 | Status matrix | Hold-limit error as 503, 504, 529 and 429 with FIX-020, FIX-021 and FIX-324 to FIX-327 | The table of retried statuses equals table 8.3 of [04](04-admission-control-and-queueing.md). The registry refuses 429 | V, R |
| FIX-134 | Slot accounting | Normal end, error, harness cancel, node ignores cancel | Running count is zero at the end and equals the node view in between | V |
| FIX-135 | Long prefill | A 230 s run beyond the window | Idle time counts from the end | V |
| FIX-136 | Retry storm | FIX-020 against a 250 s hold error | Each retry is one new request. No leaked slot | V |
| FIX-306 | Hold of 250 s by harness | Each harness of FIX-020, 021, 324 to 327 waits 250 s with no byte, then gets a normal answer | Every harness completes at the first attempt | V, R |
| FIX-307 | 503 against 429 | A Codex stub gets 503 and then 429 at the hold limit | Retry after 503. Failure at once after 429 | V |
| FIX-311 | Title calls | Two stubs, DeepSeek Harness and opencode, send a title call and a main request at session start. The pool has one free seat | The proxy holds the second request. The count of conversations excludes the title call | V |
| FIX-313 | Retry-After | A refusal with `Retry-After` of 0, 30, 60, 90 and 120 s | The proxy sends 30 s or less. The Claude Code stub retries at 30 s and fails at 90 s | V |
| FIX-314 | Error body words | Each proxy refusal body on 400 and 409 on all protocol paths | No word of PRX-PROTO-032 appears. pi and opencode stubs do not retry | V |
| FIX-315 | Early head rejected | A registry that asks for an early head, and a held request with a comment line or a `ping` event | The registry is refused. The held request gets no byte before the answer | V |
| FIX-316 | Give-up times | Each harness held with no byte until it gives up, then a dropped connection at 5 s | The times and the attempts equal tables 8.1 and 8.4 of 04 within 2 s. curl and `fetch` fail at once on a drop | S, R |
| FIX-323 | Hold budget | A hold limit of 280 s and 300 s with a node head after 15 s | A start-up warning above 290 s. The pi stub gives up at 299 s when the sum is 300 s | V |

### Protocol and streaming

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-140 | OpenAI byte identity | Recorded stream with comments and `\r\n` | Harness bytes equal node bytes | V, S |
| FIX-141 | Messages byte identity | Messages stream with pings and beta headers | Harness bytes equal node bytes. Headers pass | V, S |
| FIX-142 | Model rewrite | Alias, nested `model`, unknown alias | Only the top-level value changes. 404 for unknown | V |
| FIX-143 | Malformed bodies | Bad JSON, array, missing and numeric model, empty messages | 400 or pass as listed. No retry words | V |
| FIX-144 | Body size | Body over the limit and a 2 MB first message | 413 at the limit. Bounded hash | V, S |
| FIX-145 | Usage parse | Usage last, absent, split across reads | Usage parsed or marked unknown. No delay | V |
| FIX-146 | No end marker | Stream ends without `[DONE]` | Harness closed, cap place freed, one record | V |
| FIX-147 | Empty 200 | FIX-035 | Pass unchanged. Log flag set | V |
| FIX-148 | Node faults | FIX-030, 031, 032, 036 | Statuses and closes as listed. No proxy timeout | V, S |
| FIX-149 | Status pass | Node 4xx and 5xx with `Retry-After` | Status, headers and body equal | V |
| FIX-150 | Header hygiene | Hop-by-hop headers, `Accept-Encoding` | As listed | V, S |
| FIX-151 | Non-stream usage | Non-stream reply | Body equal. Usage logged | V |
| FIX-152 | Header strip | Inbound credentials and `x-legatus-*` at local and hosted nodes | Hosted nodes see none | V |
| FIX-153 | Other paths | `/v1/embeddings` to FIX-011 | Passes by path to a node that serves it | V |
| FIX-154 | Model list | `/v1/models` | As the decision says | V |
| FIX-155 | Count tokens | `/v1/messages/count_tokens` | Passes to a node that serves it | R |
| FIX-156 | Slow reader | Harness reads 1 chunk per second | Node emission follows the reader. Memory bounded | V |
| FIX-157 | Stale connection | FIX-037 | No request lost on a stale pooled connection | S |
| FIX-158 | Wire forms | Chunked body, `Expect`, HTTP/1.0 | Bodies intact | S |
| FIX-159 | Tool order | Tool definitions reordered between turns | Key equal. Bytes pass. Cold turn logged | V |
| FIX-160 | Patches | Patch equal on all turns, harness conflict, change by reload | Rules of the edge rows | V |
| FIX-161 | No added fields | Request without `stream_options` | Body to node equals body in | V |
| FIX-162 | REMOVED 2026-10-07 | The 404 test of the Responses path | Superseded by FIX-318 (DEC-062) | V |
| FIX-163 | Error shapes | Proxy errors on each path | Shape matches the protocol | V |
| FIX-318 | Responses path | FIX-326 against an alias with a Responses node and an alias without one. A request with `previous_response_id` and `store` | Routed, `model` rewritten, bytes relayed, both fields unchanged. A 404 in the OpenAI shape for the alias without a node (PRX-PROTO-056) | V |
| FIX-330 | Responses id routing | FIX-014 in mode `stateful` on two nodes. A request with `previous_response_id` after a first response. Then a restart of the proxy | The follow-up goes to the node that made the id. After the restart the node answers with its own error and the proxy passes it (PRX-PROTO-061) | V |
| FIX-337 | Responses flag | An alias of one node with `responses` false and one with true. A pool of FIX-015 nodes only | Requests go only to the flagged node. A pool with no flagged node answers 404 (PRX-PROTO-056, PRX-ENG-062) | V |
| FIX-338 | Codex headers | FIX-326 with `x-codex-turn-state` set by the node on a response | The header passes on the response and on the next request. The other Codex headers pass unchanged (PRX-PROTO-057, 058) | V |
| FIX-339 | Responses key order | FIX-326 with `session-id`, `thread-id`, `prompt_cache_key` and a changing `x-codex-window-id` | The key follows the order of PRX-KEY-055 and ignores the window id | V |
| FIX-319 | Responses mid-stream cut | FIX-326 against a stub that closes a Responses stream after N chunks | Abrupt close with no event. The Codex stub retries. No OpenAI-style error event | V |

### Engines and caches

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-170 | Ollama truncation | Prompts below, near and above the limit | Guard answers above. Mismatch logged near the limit | V, R |
| FIX-171 | Ollama unload | Idle past keep-alive | Load time not counted as a cold turn | V |
| FIX-172 | Ollama parallel | Declared 4, real 1 | Stricter cap used | V |
| FIX-173 | Per-slot context | `-np 4 -c 16384` and 6850 tokens | Context error | V, R |
| FIX-174 | 500 on valid JSON | FIX-002 fault | Pass unchanged. Node stays available | V |
| FIX-175 | Sleep mode | Asleep node | No `/slots` poll. Wake time not a cold turn | V |
| FIX-176 | Metrics off | `/metrics` 404 | Own count used | V |
| FIX-177 | mlx invalid request | Connection drop | Harness closed. Node available | V |
| FIX-178 | mlx wedge | Stuck node | Count shown. No action | V |
| FIX-179 | vLLM no details | Flag off | Reuse unknown. Node not poor | V |
| FIX-180 | vLLM silent zero | Tail checkpoint in unique tokens | Zero reuse measured. Mode changes by rule | V |
| FIX-181 | Template strip | Last answer rewritten | Loss equals the discarded tail plus 4 (S7: 332 tokens for 200 new tokens, because the divergence lay 2 tokens before the end). Affinity kept | V |
| FIX-182 | Cache memory | Checkpoints exceed the host cache | Reuse falls. Fields reported | V, R |
| FIX-183 | SGLang sawtooth | Period 256 in decode | Window measure is stable | V |
| FIX-184 | gufo exact prefix | Edited middle message | Reuse equals fields | V |
| FIX-185 | mlx-lm entries | 4 conversations, 10 entries | Eviction order as source | V |
| FIX-186 | MTP and cache | Node declares MTP | Measured reuse used | R |
| FIX-187 | Compaction | History rewritten | Cold turn on same node. No move | V, R |
| FIX-188 | Cache fields | `tokens_cached` and missing fields | Right field used or unknown | V |
| FIX-189 | Engine eviction | 5 conversations, 4 slots | Cold turn reason "engine eviction". Entry kept | V |
| FIX-190 | Short prompts | Prompt below one block | Excluded from the reuse measure | V |
| FIX-191 | Version drift | Version change and restart | Logged. "Needs calibration" | V |
| FIX-192 | Oracle test | All five policies, random conversations | Equal to the closed form (PRX-TEST-015) | V |
| FIX-193 | Speed model | Two prompts at once | First token about 2 times later | V |
| FIX-194 | Estimator | Estimates against engine counts on captures | Error bound measured and written down | R |
| FIX-331 | Codex real conversations | Real Codex 0.160.1 against llama-server, Ollama, vLLM and gufo with tool loops, a freeform tool call, reasoning items and compaction | The conversation completes. Each failure names the engine gap (R7 section 2). Spike F, owner decision pending | R |
| FIX-333 | Hybrid history rewrite | A 6k history on the hybrid llama.cpp stub. Edits at 194, 4730, 5484, 5483 tokens, and a rewrite of tool results | Recompute follows the oracle. Affinity keeps the node. The cold-turn event fires when `prompt_n` is far above the expected new tokens (PRX-ENG-038) | V |
| FIX-334 | Interleaved conversations | Two conversations A, B, A, B on one slot, with `cache_ram_mib` 8192 and with 0 | With 8192 each turn after the first recomputes the new tokens plus 1. With 0 every turn recomputes the whole prompt (S7) | V |
| FIX-335 | Warm capacity calibration | The probe steps 11 to 13 against FIX-004 with different `cache_ram_mib` and slots | The probe finds the checkpoint distances 516 and 4, the warm count and the state size. `warm_capacity` follows PRX-ADM-050 | V |
| FIX-336 | Seats and warm capacity | Nodes with `warm_capacity` above the slot count and with equal values. 5 conversations on 2 slots | Seats follow `warm_capacity`. The cap still bounds the running requests (PRX-ADM-009, 049, 051) | V |

### Restart

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-200 | Hold until ready | Requests before ready, ready at 5, 20 and 120 s | Served with no harness retry | V, S |
| FIX-201 | Restart with streams | Kill during streams | Harnesses see a close. Retry works | S |
| FIX-202 | Backlog | 300 harnesses during start | Held up to the bound. Count reported | S |
| FIX-203 | Table loss | Restart with known keys | Placement by rules. No recovery work | V, S |
| FIX-204 | Late ready | No ready at 250 s | Hold-limit error | V |
| FIX-205 | Second instance | Same port | Second exits | S |
| FIX-206 | Clock jumps | FIX-039 and a sleep gap | No entry changes | V |
| FIX-207 | SIGTERM | 3 running | Drain then exit | S |
| FIX-208 | Registry checks | Invalid file at start and at reload, 0 slots | Exit at start. Keep old at reload | S |
| FIX-209 | Busy engines | Restart while engines run requests | Count seeded from the signal where it exists | S, R |

### Hosted nodes

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-220 | Credential strip | Inbound `authorization`, `x-api-key`, `x-legatus-*`, `x-session-affinity` | Node saw only the proxy key. Affinity rule as decided | V |
| FIX-221 | Hosted errors | 401, 429, 529, mid-stream event, bad TLS | Pass or 502 as listed | V |
| FIX-222 | Cache markers | `cache_control` in the body | Bytes equal | V |
| FIX-223 | Mixed pool | Local and hosted nodes, one key shared by 3 users | Moves as listed. Share logged | V |
| FIX-224 | REMOVED 2026-10-07 | The owner decided that a hosted node has no running cap in v1 (DEC-050) | none | none |
| FIX-225 | Retry-After | 429 with header | Header passes | V |
| FIX-226 | Missing key | Variable missing and inbound credential present | Load refused. Inbound credential never forwarded | S |
| FIX-227 | Key in logs | Key echoed in an error body | Key in no output | V |
| FIX-301 | Hosted needs client token | Registry with a hosted node and no `client_tokens_ref` | Load refused with `hosted_needs_client_tokens` | S |
| FIX-302 | Client token check | Registry with a hosted node. Requests with no token, a wrong token and a good token on a local alias and a hosted alias | 401 for no and wrong tokens. Good token served. No byte reaches a node on a 401 | V, S |
| FIX-303 | Local-only open | Registry with local nodes only. Request with no token | Served | V |
| FIX-304 | Messages stream cut | FIX-021 against a stub that closes a Messages stream after N chunks | One `overloaded_error` event, then close. Outcome `node_stream_cut` | V |

### Observability and admin

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-240 | Admin auth | No, wrong and query-string tokens | 401 or refusal as listed | S |
| FIX-241 | Reference privacy | Planted session value | Value in no output. References differ | V |
| FIX-242 | Version path | `/legatus/v9/` | 404 | S |
| FIX-243 | Admin under load | 50 reads per second during FIX-120 | No change to table times and latency | S |
| FIX-245 | Log guard | FIX-038 | Pause and resume markers. Serving continues | V, S |
| FIX-246 | One record | Each request outcome | One record each (PRX-TEST-024) | V |
| FIX-247 | Prompt canary | Planted marker | Marker in no output | V, S |
| FIX-248 | Log injection | New lines and quotes in headers | One valid JSON line each | V |
| FIX-249 | Dashboard down | Stop the dashboard | No change in the proxy | S |
| FIX-250 | Write methods | `POST`, `PUT`, `DELETE` | 405. No change | S |

### Harness behaviour

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-260 | pi retries | FIX-020 against 503 | Timeline of 0, 2, 6 and 14 s | V |
| FIX-261 | pi compaction | Request without header | New key from the body | V |
| FIX-262 | Retry words | Each proxy error body | No retry word in non-retry errors | V |
| FIX-263 | Claude Code traffic | FIX-021 with agent ids | Keys as the map says | V, R |
| FIX-264 | Side model names | Title request with another model string | Alias or 404 | V |
| FIX-265 | Non-stream fallback | After a mid-stream error | New request, same key | V |
| FIX-266 | 20-way fan-out | FIX-022 on 1 and 4 slots | In order. Errors counted | V |
| FIX-267 | Chat id | Header on each turn | Key equals the id | V |
| FIX-268 | No header | FIX-024 | Body key | V |
| FIX-269 | Cancel | Harness closes mid-stream | Node closed. Cap place freed at node end | V, S |
| FIX-270 | Subagent keys | `parent#child` values | One key each | V |
| FIX-271 | Hold versus harness timeout | Holds of 120, 249 and 260 s with 60 s prefill | 120 s passes. 249 s plus 60 s times out at 299 s with the cap place freed | V |

### Property and replay

| ID | Scenario | Setup and steps | Pass condition | Mode |
|---|---|---|---|---|
| FIX-280 | Seeded interleaving | Random arrivals, faults and reloads on 5 nodes of mixed kinds | All invariants for all seeds | V |
| FIX-281 | Long run | 5 virtual hours of mixed traffic | Invariants hold. Memory bounded | V |
| FIX-282 | Table churn | 100000 keys over a small cap | PRX-TEST-023 holds | V |
| FIX-283 | Model oracle | A small reference placement model against the proxy | Equal decisions on all seeds | V |
| FIX-284 | Capture replay | Replay FIX-046 to FIX-051 against stubs | Same status and bytes as direct requests. Keys stable | V, S |

## Benchmark methodology

This section defines how to test the measurable claims of [the vision](00-vision-and-scope.md). The claim "better than anything else" is only a claim until a baseline row shows it. The stub tier measures only the proxy overhead. The cache and queue claims need real engines or stubs that the captures check.

### Claims

The owner decided the benchmark rule on 2026-10-07. Run the baselines first. The proxy must beat every baseline on the cold-turn rate and the queue wait. The proxy must stay within 2 times of the best baseline on the added latency and the resident memory. The pass numbers follow the benchmark.

| ID | Claim | Metric | Pass rule | Label |
|---|---|---|---|---|
| FIX-290 | Low added latency | Added time to first byte and added time per chunk against a direct request | Within 2 times of the best baseline. Run the baselines first. The numbers follow the benchmark (owner rule, 2026-10-07) | Prototype figures 0.06 ms and 0.05 ms PROVEN (RND1 3.2). Proxy NOT TESTED |
| FIX-291 | Small memory | Resident memory at 0, 100 and 10000 table entries | Within 2 times of the best baseline. The numbers follow the benchmark (owner rule, 2026-10-07). The prototype used 3 to 6 MB | Prototype PROVEN. Rule decided by owner |
| FIX-292 | Lower time to first token under load | Median and 95th percentile of the time to first token by workload | Lower than every baseline on workloads B, C and D and not higher on A | PROPOSED. The owner rule gives no pass test for this metric. The benchmark reports it |
| FIX-293 | Lower cold-turn rate | Share of turns after the first with recompute above a threshold | Lower than every baseline (owner rule, 2026-10-07). The numbers follow the benchmark | Rule decided by owner |
| FIX-294 | Fewer tokens recomputed | Mean recompute tokens per turn | Lower than round robin and not higher than any other baseline | PROPOSED. The owner rule gives no pass test for this metric. The benchmark reports it |
| FIX-295 | Short queue wait | Median and 95th percentile of the hold time and the count of hold-limit errors | Median and 95th percentile of the queue wait lower than every baseline with the same caps (owner rule, 2026-10-07). No more errors than any baseline | Rule decided by owner. Numbers follow the benchmark |
| FIX-296 | Standards and baseline fidelity | Pass rate of each recorded harness with no harness change, and the decision log of each baseline against the cache fields of the replies | All recorded harnesses complete their scenario and each baseline miss shows in the fields | PROPOSED |

Spike 7 shows that affinity pays on llama-server for append-only traffic on Qwen3.5-2B. It shows a cold prefill when history is rewritten. Whether it pays on the other engines and models is NOT TESTED. If it does not pay for an engine, the claim for that engine is "no worse than least load", and the benchmark must say so.

### Baselines

| Baseline | Version rule | Configuration | Known limit |
|---|---|---|---|
| Direct | None | The generator calls one node | No pool |
| Plain round robin | A small proxy on the same HTTP stack as the proxy | Rotate nodes, no state | No affinity and no cap |
| Olla | Pin the release and commit | Sticky sessions by header, then by body prefix. Same node list | No cap, no hold queue, no spill ([R6](evidence/research/r6-routers-20261007T055357Z.md)) |
| LiteLLM | Pin the release | Session affinity on, one deployment for each node, a cap of the node slots | Answers 429 at the cap and does not hold ([R5](evidence/research/r5-gateways-20261007T055410Z.md)) |
| HAProxy | Pin the release | The sketch below | Body key limits. GPL licence (R5) |
| SGLang router | Pin the release | The `manual` policy keyed by the header, and the cache-aware policy as a second row | Cache-aware may not apply to non-SGLang nodes (R6) |

HAProxy sketch (PROPOSED, to check against the pinned version):

```
backend pool
  balance leastconn
  stick-table type string len 64 size 10000 expire <table expiry>
  stick on req.hdr(x-session-affinity)
  timeout queue <hold limit>
  server n1 <addr> maxconn <cap>
```

PRX-TEST-060: Each baseline must receive the best key the generator can give it, in the header name that baseline reads.

PRX-TEST-061: Each baseline must run with the same node caps and the same hold limit as the proxy where it can.

PRX-TEST-062: A baseline that cannot do a function must appear in the report as "cannot", not as a zero.

### Workloads

The numbers of turns, sizes and gaps come from the harness captures, not from this file. The shapes do not change.

| ID | Workload | Shape | Stress |
|---|---|---|---|
| FIX-297 | A | One user, one long agent loop that grows to a long context, with tool pauses | Overhead and warm reuse |
| FIX-298 | B | Three users, each with an agent loop and two subagents, on 5 nodes of 4 slots | Affinity, caps and queue order |
| FIX-299 | C | Subagent burst: one parent starts 8 and then 20 children at once | Hold queue and deadlock |
| FIX-300 | D | Compaction storm: all conversations compact in the same minute | Cold prefill and hold limit |

### Metrics and how to collect them

| Metric | Definition | Source |
|---|---|---|
| Added latency | Proxy time to first byte minus direct time to first byte, same node | Harness timestamps, A/B/A order |
| Memory | Resident set size sampled once per second | Process table |
| Time to first token | Request start to first content chunk | Harness timestamps |
| Cold-turn rate | Turns after the first with recompute tokens above a threshold, divided by all such turns | Reply cache fields |
| Tokens recomputed | Recompute tokens per turn | `prompt_n` or prompt tokens minus cached tokens |
| Queue wait | Request start to start of service on the node | Proxy log, or the harness first-byte time for baselines |
| Errors | Count by status and by cause | Harness |

### Reporting

PRX-TEST-063: A benchmark run must use frozen engine versions and record every version and flag.

PRX-TEST-064: A benchmark must repeat each cell at least 5 times, interleave the systems in A/B/A order and report the median and the range.

PRX-TEST-065: A benchmark must discard a warm-up phase of fixed length that the report names.

PRX-TEST-066: The report must show one table for each workload with a row for each system and a column for each metric.

PRX-TEST-067: The report must publish the generator seed, the configuration of every baseline and the raw logs.

PRX-TEST-068: The report must mark a result NOT TESTED when a baseline or an engine was not run.

PRX-TEST-069: The report must give the noise measured in the run. Spike s3 saw noise of 2 times between identical runs.

PRX-TEST-070: The benchmark must run and record every baseline before it applies a pass rule to the proxy.

## Sources

- [Spike C: key stability](evidence/spikes/sC-key-stability/README.md)
- [Spike D: hold tolerance](evidence/spikes/sD-hold-tolerance/README.md)
- [Spike s2: paused time](evidence/spikes/s2-paused-time-README.md)
- [Spike s3: slots and concurrency](evidence/spikes/s3-slots-README.md)
- [Spike s5: pi retry and restart](evidence/spikes/s5-pi-restart-README.md)
- [Spike s1b: header mechanism](evidence/spikes/s1b-pi-lease-gaps-README.md)
- [Spike s7: hybrid cache](evidence/spikes/s7-hybrid-cache/README.md), [research r7: Responses API and Codex](evidence/research/r7-responses-api-20261007T081115Z.md)
- [Spike s6: checkpoint policy](evidence/spikes/s6-checkpoint-source-README.md), [llama.cpp notes](evidence/spikes/notes-llamacpp-20261007T054753Z.md), [vLLM notes](evidence/spikes/notes-vllm-20261007T054827Z.md), [SGLang notes](evidence/spikes/notes-sglang-20261007T054827Z.md), [gufo notes](evidence/spikes/notes-gufo-20261007T054857Z.md), [mlx notes](evidence/spikes/notes-mlx-20261007T054857Z.md)
- [Local engines research](evidence/research/r4-local-engines-20261007T022141Z.md), [gateways research](evidence/research/r5-gateways-20261007T055410Z.md), [routers research](evidence/research/r6-routers-20261007T055357Z.md)
- [Spike round 1 decisions](../decisions/2026-10-spike-decisions.md)
- [Older roadmap with the simulated cluster epic](../baseline/roadmap.md)
- [Open WebUI notes](../../docs/horizon/openwebui.md)
