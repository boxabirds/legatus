# Engine behaviour

Status: draft for review. Written 2026-10-07, updated with spike 7 and r7 the same day. Requirement IDs: PRX-ENG-001 to PRX-ENG-075. Style: STE-style (not STE-compliant). Glossary: [15-glossary.md](15-glossary.md).

This file records how each inference engine behaves where it matters to a proxy. It also sets the proxy rules that follow from that behaviour. Routing and affinity rules are in [03-affinity-and-keys.md](03-affinity-and-keys.md). Admission rules are in [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md). The registry fields that carry these facts are in [07-registry-and-configuration.md](07-registry-and-configuration.md).

## 1. How to read the labels

- PROVEN: measured in a spike on this machine, or read from source code with a file and line reference.
- PROPOSED: a design choice. Nobody measured it.
- ASSUMPTION: believed, not checked. Most vLLM and SGLang runtime facts have this label, because no spike ran those engines.
- NOT TESTED: nobody tried it.
- [D] and [I] come from [R1](evidence/research/r1-models-20261007T022035Z.md). [D] means documented at a URL. [I] means computed from a configuration file.

Spike absolute numbers come from an Apple M2 with 16 GB, shared with other processes. Run-to-run drift was 2 times. Trust ratios and shapes, not absolute speeds ([S3](evidence/spikes/s3-slots-README.md)).

Spike 7 ([S7](evidence/spikes/s7-hybrid-cache/README.md)) measured the hybrid cache. It used llama-server b11459 and brew 0.5.0 (b11146). The hybrid model was Qwen3.5-2B Q4_K_M. The dense control was qwen3:1.7b. Token counts in S7 are deterministic. Times are noisy because the machine was under load.

The model is small. The rules that S7 supports transfer to Qwen 3.8 Flash-Next and 27B as ASSUMPTION. Speeds and sizes are NOT TRANSFERABLE. Spike B measures them (DEC-060). The owner runs spike B on the Strix Halo and the M5 Max.

## 2. Versions tested

| Engine | Version used | Latest seen (2026-10-07) | Evidence |
| --- | --- | --- | --- |
| Ollama | 0.35.1 (frozen copy) | v0.40.0, released 2026-10-06 | [S3](evidence/spikes/s3-slots-README.md), [S4](evidence/spikes/s4-smoke-README.md) |
| llama-server | 0.5.0, build b11146 (S3). Build b11459 and b11146 (S7) | v0.6.0, build b11462 is the source head | [S3](evidence/spikes/s3-slots-README.md), [S7](evidence/spikes/s7-hybrid-cache/README.md), [clones](evidence/spikes/notes-clones-and-versions-20261007T054753Z.md) |
| mlx_lm | 0.32.0 | 0.32.0 | [S3](evidence/spikes/s3-slots-README.md) |
| vLLM | not run, source main 43b4aaea | release v0.31.0 (2026-10-05) | [vLLM notes](evidence/spikes/notes-vllm-20261007T054827Z.md) |
| SGLang | not run, source main f47d8956 | not checked | [SGLang notes](evidence/spikes/notes-sglang-20261007T054827Z.md) |
| gufo | not run, source 33d1b208 | same | [gufo notes](evidence/spikes/notes-gufo-20261007T054857Z.md) |
| LM Studio | not run, not read | not checked | none |

The Ollama v0.40.0 release notes say that models run on MLX by default on Apple Silicon (R4, labelled [U] there). If that is true, the Ollama 0.35.1 results do not describe Ollama 0.40.0 on a Mac. NOT TESTED.

- PRX-ENG-001: The proxy must record the engine name and version of each node in the node record.
- PRX-ENG-002: The proxy must report a node whose engine version differs from the version tested in this file. The report must be a warning in the admin read.
- PRX-ENG-003: The proxy must run the calibration probe of section 12 again when the engine version of a node changes.

## 3. Summary matrix

| Topic | llama-server | Ollama | mlx_lm | vLLM | SGLang | gufo |
| --- | --- | --- | --- | --- | --- | --- |
| Default slots | 4, from `/props total_slots` | 1 | 1 by throughput, 8 or more in flight | `max_num_seqs` | auto, `--max-running-requests` | `--sessions N` |
| Extra requests | queue, headers early | queue, headers late | none, all admitted | queue, FCFS | queue | bounded queue |
| Overflow | 400 | silent cut, 200 | none, memory grows | ASSUMPTION: 400 | ASSUMPTION: error | not read |
| Load signal | `/metrics`, `/props` | `/api/ps` only | none | ASSUMPTION: `/metrics` | ASSUMPTION: `/metrics` | `/slots` |
| Reuse field | `timings.cache_n` | cached tokens in usage, name not checked | `prompt_tokens_details.cached_tokens` | same, needs a flag | same | `timings.cache_n` |
| Reasoning field | `reasoning_content` | `reasoning` | `reasoning` | not read | not read | not read |
| Thinking switch | `chat_template_kwargs.enable_thinking` | `reasoning_effort` or `think` | not tested | not tested | not tested | not tested |

Sources for each cell are in the engine sections below.

## 4. llama-server

| Item | Fact | Label and source |
| --- | --- | --- |
| Slots | `/props total_slots` equals the real number of slots. Default is 4 with a unified KV cache. | PROVEN, S3 finding 3 |
| Real concurrency | Overlap equals the slot count. Gain at 4 slots is 1.4 to 1.8 times on decode-bound traffic and about 1.3 times on prefill-bound traffic. | PROVEN, S3 finding 5 |
| Queue | Request 5 and later wait in a deferred queue. Response headers arrive early, after 40 to 120 ms. Tokens start when a slot frees. | PROVEN, S3 finding 3 |
| Context per slot | Default (auto) uses a unified cache, so each slot can use the whole `-c`. An explicit `--parallel N` splits `-c` by N. `-c 16384 -np 4` gives 4096 per slot. | PROVEN, S3 finding 3 |
| Overflow | A prompt above the slot context gets HTTP 400 with `exceed_context_size_error`. A 6850-token prompt failed at 4096 per slot. | PROVEN, S3 finding 3 |
| Load signals | `/metrics` needs `--metrics`. `requests_processing` and `requests_deferred` were exact in every run. `/props` is static. `/slots` wakes an engine that sleeps (`--sleep`). `/health`, `/props` and `/metrics` do not. | PROVEN, S3 finding 3 and round 1 |
| Reuse fields | `timings.cache_n` is the reused count. `timings.prompt_n` is the count prefilled now. The field `tokens_cached` is `N + G`, not the reused count. | PROVEN, source b11460 server-common.cpp:84-96, server-task.cpp:349-356 |
| Streaming reuse | The terminal chunk carries `timings`. `return_progress` adds `prompt_progress` with `total`, `cache` and `processed`. | PROVEN, source |
| Tool calls | `--jinja` is on by default in 0.5.0. With `--no-jinja` a request with tools gets HTTP 500 `tools param requires --jinja flag`. | PROVEN, S4 section "Which settings matter" |
| Thinking | `chat_template_kwargs.enable_thinking` false removes thinking blocks. The output field is `reasoning_content`. | PROVEN, S4 and round 1 |
| Sampling defaults | temperature 0.8, top_k 40, min_p 0.05. The proxy does not change them. | PROVEN, S4 |
| Keep-alive | The engine closes an idle connection after 5 s. A harness that reuses it gets `fetch failed` after 5 ms. | PROVEN, S3 finding 9 |
| Malformed JSON | Version 0.5.0 answers HTTP 500. pi retries it. | PROVEN, round 1 |
| Version | 0.5.0, build b11146. Source read at b11460 and b11146. | PROVEN, [clones](evidence/spikes/notes-clones-and-versions-20261007T054753Z.md) |

### 4.1 Prompt cache and checkpoint policy

All line references are to commit 78651c41 (tag b11460, 2026-10-07) in `tools/server/server-context.cpp`, unless the text names the file. The installed build b11146 (commit 7fe450e1) has the same logic at shifted lines. Source: [llama.cpp notes](evidence/spikes/notes-llamacpp-20261007T054753Z.md) and [errata](evidence/spikes/notes-llamacpp-errata-20261007T054827Z.md).

Defaults (PROVEN, common/common.h and common/arg.cpp):

| Flag | Default | Reference |
| --- | --- | --- |
| `--ctx-checkpoints` (`-ctxcp`) | 32 per slot | common.h:635, arg.cpp:1701 |
| `--checkpoint-min-step` (`-cms`) | 8192 tokens | common.h:637, arg.cpp:1709 |
| `--cache-ram` | 8192 MiB | common.h:638, arg.cpp:1719 |
| `--slot-prompt-similarity` | 0.1 | common.h:701 |
| `--cache-idle-slots` | on | arg.cpp:1737 |
| n_ubatch | 512 | common.h:457 |

When the engine makes a checkpoint (PROVEN):

- PROVEN by S7 (b11459 and b11146): the positions below match what the logs show. The prediction from the S6 source code matched the measurement.
- The engine makes checkpoints only for completion tasks. It also needs sequence removal type FULL or recurrent, or a model with sliding window layers (3708-3721). A Gated DeltaNet hybrid is type FULL (common.cpp:1553-1585).
- The engine makes the checkpoint before it decodes a batch, so a checkpoint at `n_tokens_start` holds the state after tokens 0 to `n_tokens_start - 1` (3901-3904).
- The engine splits a batch at the start of a user message in two cases (3819-3850). The first case is the last user message. The second case is a message more than 8192 tokens after the last checkpoint. The chat parser supplies the message spans (common/parsers/qwen3-coder.cpp:32-39). Tool results have role TOOL, so they do not start a checkpoint.
- The engine also splits at `L - 516` (4 plus n_ubatch) and at `L - 4` (3819-3850).
- The engine makes no checkpoint after generation. The state at `L + G` exists only as the live slot state, or inside a `--cache-ram` entry (create_checkpoint has one call site, 3904).
- When the list is full, the engine removes older checkpoints that lie within the minimum step of a predecessor. Then it removes the oldest checkpoint (2513-2577).

How the engine chooses a checkpoint for a new prompt (PROVEN, 3495-3640):

1. The engine finds the common prefix `D` of the stored sequence (length `L + G`) and the new prompt (3444).
2. If the live recurrent state lies at or after `D`, the state cannot roll back. The engine searches checkpoints from newest to oldest and takes the first with `n_tokens <= D` (3580-3592). The position meaning is an inference (llama-memory-recurrent.cpp:390).
3. On a hit, the engine loads the state and sets `n_past` to the checkpoint length (3598-3607). On a miss, it logs "forcing full prompt re-processing" and sets `n_past` to 0 (3610).
4. The engine removes every checkpoint beyond `D` (3619-3628). After a divergence, later checkpoints are gone.
5. The engine then removes sequence cells from `n_past` on (3674). A partial removal destroys the recurrent state, so the checkpoint step must come first.
6. If `n_past` equals the new prompt length `N`, the engine reprocesses one token (3635-3640).

The host cache `--cache-ram` stores the whole sequence state at `L + G`, with the checkpoint list of that prompt (server-context.cpp:308-331, server-task.cpp:1700-1920). The state includes the recurrent state and the KV. A load needs `f_keep` (common prefix over stored length) of at least 0.25. The engine removes the oldest entries first. Slot choice uses prefix similarity above 0.1, else the least recently used slot (1692-1740). If the engine keeps less than half the slot content, it saves the slot to the host cache (1770-1786).

### 4.2 The cost formula (s6)

Let `N` be the new prompt length. Let `D` be the common prefix with the stored sequence of length `L + G`. Let `C` be the set of checkpoint lengths of that slot or cache entry.

- `c*(D)` is the largest `c` in `C` with `c <= D`, or 0 when none exists. If `D = L + G` and the live state is intact, `c*` is `L + G`.
- `recompute(D) = N - c*(D)`. Add 1 if `c*` equals `N`.
- For llama.cpp, `C` is `{L - 516, L - 4}` plus user-message starts at least 8192 tokens apart (at most 32 kept), plus the live state `L + G`.

The result is a staircase, flat between checkpoints. It is not a sawtooth. The largest realistic cliff is the difference between a full replay (`D = L + G`) and any edit in the generated text (`D < L + G`). That edit loses `G + 4` tokens. S7 measured this formula (section 4.5). The measured recompute equals `N - c*(D)` in every case.

### 4.3 Known bugs and issues

| Issue | Summary | Label |
| --- | --- | --- |
| ggml-org/llama.cpp#19794, #20225, #21831 | Cold prefill of the whole prompt on every turn for Qwen3-Coder-Next and Qwen3.5 27B. #19794 and #21831 closed not planned. | PROVEN that the issues exist (R4) |
| #22384 | Checkpoint search used the sliding window check and never matched for recurrent models. Closed. Older code had a 64-token minimum. Current source has none. | PROVEN, source grep and R4 |
| #25913 | `/slots` save and restore on a hybrid model does not keep checkpoints. The restore reports success and then recomputes everything. | PROVEN as reported (R4) |
| #28425 | Open on 2026-09-05. Partial prefix reuse on the qwen4exp branch without speculative decoding. The second request that shares a prefix can crash or leak. | PROVEN as reported (R4) |
| #29092 | ROCm: Gated DeltaNet state carries across requests on a reused slot. | PROVEN as reported (R1) |
| (no number) | A checkpoint failure gives HTTP 200 and shows only at `-lv 4`. One rejected request removed 15 checkpoints (2.19 GiB). | [U] second-hand blog, R4 |

### 4.4 Rules

- PRX-ENG-004: The proxy must read `/props total_slots` from a llama-server node at join and use it as the declared slot count when the registry says `auto`.
- PRX-ENG-005: The proxy must read the load of a llama-server node from `/metrics` only.
- PRX-ENG-006: The proxy must not call `/slots` on a node that runs with `--sleep`.
- PRX-ENG-007: The proxy must compute the context of one slot as `-c` divided by `-np` when the node uses an explicit `--parallel`.
- PRX-ENG-008: The proxy must read cache reuse of a llama-server response from `timings.cache_n` and `timings.prompt_n`.
- PRX-ENG-009: The proxy must not use `tokens_cached` as a measure of cache reuse.
- PRX-ENG-010: The proxy must not reuse an upstream connection to llama-server that was idle for more than 4 seconds. PROPOSED limit, below the 5 s engine timeout.
- PRX-ENG-011: The proxy must pass an HTTP 400 `exceed_context_size_error` to the harness without change.

### 4.5 Measured hybrid behaviour (spike 7)

Source: [S7](evidence/spikes/s7-hybrid-cache/README.md). Engine: llama-server b11459 (commit f498f864f) and brew 0.5.0 (b11146). Models: Qwen3.5-2B Q4_K_M (24 layers, 3 linear to 1 full attention) and the dense control qwen3:1.7b.

Defaults: `--ctx-checkpoints` 32, `--checkpoint-min-step` 8192, `--cache-ram` 8192 MiB, `-ub` 512. Both builds gave the same checkpoint positions. All token counts are PROVEN and identical in three repetitions. Times are PROVEN as measured under load.

Token route (raw tokens on `/completion`). A prompt of `N` tokens gets exactly two checkpoints, at `N - 516` and at `N - 4`. No periodic checkpoint appears. The common prefix `D` gives `recompute = N - c*`, where `c*` is the largest checkpoint at or below `D`, or `N` if none exists.

| N | D | Hybrid recompute | Dense recompute |
|---|---|---|---|
| 6000 | 100 | 6000 | 5900 |
| 6000 | 5000 | 6000 | 1000 |
| 6000 | 5480 | 6000 | 520 |
| 6000 | 5490 | 516 | 510 |
| 6000 | 5900 | 516 | 100 |
| 6000 | 5997 | 4 | 3 |
| 12000 | 8200 | 12000 | 3800 |
| 12000 | 11480 | 12000 | 520 |
| 12000 | 11490 | 516 | 510 |
| 12000 | 11997 | 4 | 3 |

The dense control degrades linearly with the distance to the end. The hybrid model has a cliff.

One-token cliff, three of three repetitions. At N = 6000, `D` = 5483 recomputed 6000 tokens in 15.0 s. `D` = 5484 recomputed 516 tokens in 1.3 s. At N = 12000, `D` = 11483 recomputed 12000 tokens in 30.6 s and `D` = 11484 recomputed 516 tokens in 1.4 s. The dense control showed no cliff.

Time for a cold prefill was about 15 to 17 s at 6k and 34 to 38 s at 12k on this M2 under load. A reuse at `N - 4` took about 0.06 s. A reuse at `N - 516` took about 1.5 s.

Chat route (`--jinja`, thinking off, one word edited in message `i`). Checkpoints appear at the start of the first user message (n_tokens 133), the start of the last user message, `N - 516` and `N - 4`. A further user-message start gets a checkpoint only when it lies at least 8192 tokens after the previous checkpoint.

| History | Edit at D | Recompute | Time |
|---|---|---|---|
| 6075 | 194 to 4730 | 5942 | about 22 s |
| 6075 | 5772 to 5936 (last assistant) | 516 | about 2 s |
| 6075 | 6066 (last user) | 20 | 0.15 s |
| 12281 | 342 to 6215 | 12148 | about 47 s |
| 12281 | 9354 or 11692 | 3877 | about 16.5 s |
| 12281 | 12171 | 516 | about 2 s |
| 12281 | 12272 | 20 | 0.31 s |

The dense control reused exactly `D` tokens in every chat case (a linear ramp).

`--checkpoint-min-step 1024` (one repetition, 12k): checkpoints at user-message starts about 1.2k tokens apart, with about 19 MiB for each checkpoint for each slot. This gives a staircase at user-message boundaries (D = 3176 recomputed 9768 tokens, D = 9354 recomputed 3877, D = 11692 recomputed 1503). It costs memory. Not tested at `-np 2` or under memory pressure.

Pure append (300 generated plus 200 new tokens, token route): recompute 201 tokens, about 0.9 s, on the hybrid and on the dense model. Pure append is free.

Template divergence. Qwen3.5 rewrites the end of the previous prompt. The second prompt diverged at the prompt length minus 2. The engine reused the `N - 4` checkpoint and recomputed 332 tokens (200 new plus the discarded tail).

This is cheap only because the divergence lies within about 4 tokens of the end. A divergence 5 to 516 tokens before the end costs 516 tokens. An earlier divergence costs the whole prompt. The history in this test was synthetic (about 9.4k and 19k tokens). The dense control did not reproduce the divergence.

Two interleaved conversations (order A, B, A, B). Each had a 6k history, 100 generated tokens and 200 new tokens per turn. With `-np 1` and with `-np 2`, both stayed warm. Each turn after the first recomputed 201 tokens.

The cause is `--cache-ram`. The engine saves the state of an idle slot in host memory. That took about 129 MiB for a 6k hybrid conversation (91 MiB of state and two checkpoints of about 19 MiB). A swap took about 30 ms.

With `-np 1 --cache-ram 0` every turn was cold, about 23 s. Warmth of more than one conversation for each slot depends on `--cache-ram`. It does not depend on the slot count.

Not tested: streaming usage fields and Qwen3-Next architecture models. Also not tested: eviction beyond 32 checkpoints, a full 8 GiB cache and tool-message boundaries. Also not tested: `-np 2` under memory pressure and more than two builds.

Cold-turn signal. `timings.prompt_n` is the recomputed count and `timings.cache_n` is the reused count. In the chat response, `usage.prompt_tokens_details.cached_tokens` equals `cache_n` (PROVEN, non-stream). The terminal chunk of a stream carries `timings`: NOT TESTED in S7 (the source says yes, section 4).

## 5. Ollama

| Item | Fact | Label and source |
| --- | --- | --- |
| Slots | `OLLAMA_NUM_PARALLEL` default is 1. A value of 2 or 4 starts the runner with `-np N` and `-c ctx*N`. The context per slot stays at 8192 in the test. | PROVEN, S3 finding 1 |
| Discovery | `/api/ps` shows `size_vram` growth (2.36, 3.36, 5.33 GB for 1, 2, 4). It shows no slot, busy or queue field. The proxy cannot read the environment. | PROVEN, S3 finding 1 |
| Queue | FIFO. A queued request gets no headers until service starts. With N = 8 and one slot, headers took 2.8 s to 39.9 s. No error up to N = 8. | PROVEN, S3 finding 2 |
| Queue limit | `OLLAMA_MAX_QUEUE` default 512, then HTTP 503. | Documented in the FAQ (R4), NOT TESTED |
| Real concurrency | Useful concurrency equals `NUM_PARALLEL`. Aggregate speed stays flat after that. | PROVEN, S3 finding 8 |
| Overflow | Silent truncation with HTTP 200. At the default context of 4096, a long prompt was cut to about 2050 tokens. | PROVEN, round 1 ([decisions](../decisions/2026-10-spike-decisions.md) section 3.5) |
| Reuse field | Round 1 saw a cached-token count in `usage`. The exact field name was not recorded in the notes. | NOT TESTED, see section 15 |
| Thinking | `reasoning_effort` set to `none` or `medium` on the OpenAI path. `think` false on the native `/api/chat` path. The output field is `reasoning`. | PROVEN, S4 |
| Thinking-only builds | The `qwen3:4b` library build always thinks. All three switches still produce thinking. With `none` the thinking leaks into `content` with HTTP 200. | PROVEN, S4 |
| Templates | Ollama 0.35.1 ignored a custom tool-less TEMPLATE for qwen3 and kept calling tools. | PROVEN, S4 |
| Keep-alive | Default 5 minutes. A negative value keeps the model loaded. Zero unloads it. | Documented in the FAQ (R4), NOT TESTED |
| Prompt cache | Ollama has its own runner cache, separate from llama-server. A third-party test on 0.34.0 saw about 1,025 tokens recomputed per new conversation for recurrent models on the llama runner. The same test saw 1,019 to 1,080 tokens recomputed per turn in interleaved conversations. | [U] one blog, R4 |
| Headers | Response headers carry nothing useful. | PROVEN, S3 finding 1 |

- PRX-ENG-012: The proxy must use the declared slot count of an Ollama node as its cap, because no signal reports it.
- PRX-ENG-013: The proxy must count requests in flight on an Ollama node and use that count as the only load signal.
- PRX-ENG-014: The proxy must warn in the admin read when the declared slots of an Ollama node are above the measured useful concurrency.
- PRX-ENG-015: The proxy must compare the prompt size of each request to an Ollama node with the loaded context of that node.
- PRX-ENG-016: The proxy must answer with a context error when the prompt estimate is above the loaded context of an Ollama node. The proxy must not forward that request. The error shape is in [06-protocols-and-harnesses.md](06-protocols-and-harnesses.md).
- PRX-ENG-017: The proxy must not read a late first byte of a queued Ollama request as a node failure. Headers arrive only when service starts.
- PRX-ENG-018: The proxy must compare the prompt token count in the usage of an Ollama response with its own estimate. The proxy must record a truncation event when the count is lower by more than 25 percent. PROPOSED threshold.
- PRX-ENG-019: The proxy must not tune the Ollama model keep-alive.

## 6. mlx_lm

| Item | Fact | Label and source |
| --- | --- | --- |
| Concurrency | The engine batches requests (`--decode-concurrency` 32, `--prompt-concurrency` 8 by default). Overlap equaled N up to 8. | PROVEN, S3 finding 4 |
| Concurrency gain | Aggregate decode speed did not grow: 55, 58, 53, 51 tok/s for N = 1, 2, 4, 8. Speed per request fell from 59 to 7 tok/s. Batching is time slicing here. | PROVEN, S3 finding 4 |
| Queue | None. The engine admits every request. Headers arrive in 10 to 40 ms. TTFT at N = 1, 2, 4, 8 was 1.1, 3.5, 7.4, 15.9 s. | PROVEN, S3 finding 4 |
| Load signals | None. `/props`, `/metrics`, `/slots` return 404. | PROVEN, S3 finding 4 |
| Overflow | No limit. Memory grows. | PROVEN, round 1 |
| Reuse field | `usage.prompt_tokens_details.cached_tokens` (server.py:722, 949, 1349-1351, 1598-1600). | PROVEN, source |
| Cache | `LRUPromptCache`, `--prompt-cache-size` 10 entries, `--prompt-cache-bytes` unlimited (server.py:1772, 1901-1910, and cache.py:1599-1700). | PROVEN, source |
| Hybrid cache | Gated DeltaNet caches cannot trim (`ArraysCache`, cache.py:164). Reuse needs a stored key that is a prefix of the new prompt. | PROVEN, source |
| Failures | Drops the connection on an invalid request. Does not stop a cancelled prefill. One wedge under heavy load, not reproduced. | PROVEN, round 1 and S3 |
| Thinking | Output field `reasoning`. The switch was not tested. | NOT TESTED |
| Version | 0.32.0 installed and latest. The source clone is main `a537041a` and can be ahead. | PROVEN, [clones](evidence/spikes/notes-clones-and-versions-20261007T054753Z.md) |

The checkpoint set for the batched path has three members. They are the end of the system segment, the end of the user segment (about `L - 11`), and the finished sequence `L + G`. The references are server.py:830-845, 879-885 and `_tokenize` at 523-631.

The non-batched path stores only the final sequence (946-1010). Eviction takes "assistant" entries before "user" before "system" (cache.py:1608-1640). Source: [mlx notes](evidence/spikes/notes-mlx-20261007T054857Z.md). The formula of section 4.2 applies with `C = {system end, L - 11, L + G}`. A template that strips reasoning loses `G + 11` tokens.

The mlx-vlm path serves Qwen 3.8 Flash-Next, not mlx-lm. That path has prefix caching off by default (apc.py:4933). PROVEN, source.

- PRX-ENG-020: The proxy must set the cap of an mlx_lm node to the declared slots, with a default of 1.
- PRX-ENG-021: The proxy must treat an mlx_lm node as having no load signal except its own in-flight count.
- PRX-ENG-022: The proxy must not read an early response header from an mlx_lm node as proof of a free cap place.
- PRX-ENG-023: The proxy must report the memory growth risk of an mlx_lm node in the admin read, because the node has no context limit.

## 7. vLLM

No spike ran vLLM. Every runtime fact here comes from [R2](evidence/research/r2-vllm-20261007T022033Z.md) and the [source notes](evidence/spikes/notes-vllm-20261007T054827Z.md) at main 43b4aaea. Labels follow those notes.

| Item | Fact | Label |
| --- | --- | --- |
| Concurrency | `max_num_seqs` limits running sequences. `max_num_batched_tokens` limits tokens per step. | [D] R2 |
| Queue | The waiting queue is FCFS by default. A running request is not removed when a new one arrives. | [D] R2 |
| KV exhaustion | vLLM preempts running requests and recomputes them. V1 default is recompute. | [D] R2 |
| Load signals | Prometheus metrics for waiting and running requests, and `vllm:prefix_cache_hits_total` and `queries_total`. Nobody verified the exact names for this version. | ASSUMPTION |
| Reuse field | `usage.prompt_tokens_details.cached_tokens` only with `--enable-prompt-tokens-details`. The default is False (launchers/cli_args.py:148). | PROVEN, source |
| Hybrid mode | Prefix caching on a hybrid model sets `mamba_cache_mode="align"` (models/config.py:754-764). The mode needs chunked prefill. The `all` mode is not in the source clone. | PROVEN, source |
| Checkpoint set | One state per request at `s* = floor((P - 1) / B) * B`, where `P` is the prompt length and `B` is the block size (528 to 2096 for Qwen3.5 sizes, issue 45238). Plus junction states. Nothing for generated tokens. | PROVEN, source (scheduler.py:423-520, kv_cache_coordinator.py:323-339). Inferred: the "nothing for generated tokens" part. |
| Hit rule | Hit is `max{s in S : s <= D, s <= N - 1}`. The Mamba group can veto an attention hit, so all reuse drops to 0 without a metric. | PROVEN, source and issue 45238 |
| Retention | `--prefix-cache-retention-interval` default 0 keeps only semantic checkpoints. `--enable-mamba-shared-prefix-checkpoint` default False. | PROVEN, source (cache.py:158-165, 197) |

Known issues in vllm-project/vllm are in the table below. Source: R2. The status of #47861 and #53912 is [U].

| Issue | Summary |
| --- | --- |
| #45238 | Open. Silent zero hits when the single checkpoint lands in request-unique tokens. |
| #40696 | Prompts under 528 tokens get about 0 percent hits. |
| #51250 | Open. Zero hits on Qwen3.5-35B-A3B. |
| #60008 | Open. Align mode costs 13 to 16 percent output speed with zero hits. |
| #47861, #53912 | Prompt cache with MTP corrupts output on hybrids. |

- PRX-ENG-024: The proxy must treat a vLLM node as having no usable reuse signal unless the registry declares `prompt_tokens_details: true` for that node.
- PRX-ENG-025: The proxy must report in the admin read a vLLM node that runs a hybrid model and has no usable reuse signal.
- PRX-ENG-026: The proxy must treat the cap of a vLLM node as the declared `max_num_seqs`, because no signal reports it. ASSUMPTION that the declared value equals the real value.

## 8. SGLang

No spike ran SGLang. Sources: [R3](evidence/research/r3-sglang-routing-20261007T022127Z.md) and [source notes](evidence/spikes/notes-sglang-20261007T054827Z.md) at main f47d8956.

| Item | Fact | Label |
| --- | --- | --- |
| Concurrency | SGLang derives `--max-running-requests` from memory when unset. When unset, `--max-queued-requests` has no limit. | [D] R3 |
| Queue | Extra requests wait. Policy `--schedule-policy` is fcfs in one document and lpm in another, so the default depends on the version. | [D] R3, conflict |
| Retraction | If the KV pool fills during decode, SGLang returns running requests to the queue. They recompute. | [D] R3 |
| Cache | Unified radix cache with FULL and MAMBA components. A Mamba state sits on tree nodes. The match is the deepest node that has a state. | PROVEN, source |
| Strategy | `--mamba-radix-cache-strategy` default `auto`. It selects `extra_buffer` for Qwen3-Next and Qwen3.5 families. We assume the `disable_overlap_schedule` default. | PROVEN source, inferred default |
| Checkpoints | Prefill chunk ends on a grid, decode multiples of `--mamba-track-interval` (default 256), branching points, and the final state. | PROVEN, source (schedule_batch.py:3037-3110, 2200-2210) |
| Eviction | Mamba LRU. An interior node loses its state. SGLang removes a leaf. | PROVEN, source |
| Reuse field | `usage.prompt_tokens_details.cached_tokens` (protocol.py:192, 2086-2146). We infer whether it counts the hit after Mamba truncation. | PROVEN field, inferred meaning |
| Load signals | Prometheus metrics need a flag. Names unverified. | ASSUMPTION |
| Sessions | `--enable-session-radix-cache` takes a `session_id`. It is soft protection and does not pin memory. | [D] R3 |

Known issues: SGLang model gateway bug smg-project/smg#2804 (the router cannot see requests that run, so a burst piles on one worker, [D] R3). The checkpoint grid and the Mamba hit meaning for Qwen 3.8 Flash-Next are [U].

- PRX-ENG-027: The proxy must not send a `session_id` field to an SGLang node, because the proxy has no rule to close the session.
- PRX-ENG-028: The proxy must read cache reuse of an SGLang response from `usage.prompt_tokens_details.cached_tokens`.

## 9. gufo, LM Studio and others

### 9.1 gufo

The sources are [gufo notes](evidence/spikes/notes-gufo-20261007T054857Z.md) at commit 33d1b208 (public repository) and R4. Lines marked (doc) come from `docs/KV-CACHE.md` or `docs/SERVER.md`, not from code.

| Item | Fact | Label |
| --- | --- | --- |
| Concurrency | `--sessions N`. Each request leases an executor with its own KV and recurrent state. The admission queue has a limit. | [D] R4 |
| Checkpoints | At most 128 records, independent of `--sessions` (text_model_runner.hpp:47, text_model_runner.cpp:586). | PROVEN, source |
| Grid | Up to 4 points on a 2048 grid, spread over the prompt. The last point lies within 2048 of the end (text_model_runner.cpp:569, 690-740). | PROVEN, source |
| Other points | A stable boundary before the generation suffix, the full prompt, learned branch points (at least 512 tokens gained, `kSharedPrefixMinTokens`, hpp:475), and the live frontier `L + G`. | PROVEN source, doc for learned points |
| Lookup | Exact prefix only. Prefer the live frontier, else the longest snapshot that is a prefix. | (doc) |
| Memory | `--cache-ram-bytes` 0 means the smaller of 32 GiB and half the free RAM. `--cache-disk` entries lie at least 2048 tokens apart. | (doc) SERVER.md:178-185 |
| Reuse fields | llama.cpp-compatible `timings.cache_n` and `prompt_n` in the terminal chunk, `usage.gufo`, `/slots`, `prompt_progress`. Logs hold `cache_miss_reason`. | (doc) SERVER.md:676-682, 985-1010, 1129-1135 |
| Measured | 227 requests in 75 minutes at 40 to 96k context: 222 hits, median 28 to 435 new tokens read, TTFT 0.4 to 1.1 s. A 60k request cold took 47 s. | [D] R4, owner repo |
| Worked example | Prompt 5620, common prefix 5599: the 4096 checkpoint gives 1524 recomputed tokens. With no checkpoint it gives 5620. | (doc) |

The formula of section 4.2 applies with `C = {up to 4 grid points, stable boundary, full prompt, learned branches, live L + G}`. The stable checkpoint covers a divergence inside the last 64 tokens (doc).

- PRX-ENG-029: The proxy must treat a gufo node as a llama-server compatible node for the reuse fields `timings.cache_n` and `timings.prompt_n`.

### 9.2 LM Studio

PROVEN facts: none from this project. The spike machine had no LM Studio model file ([S4](evidence/spikes/s4-smoke-README.md)).

All that we know comes from a vendor blog that R4 read. It is [U]. The mlx-engine v1.8.5 stores disk-backed KV checkpoints at 256-token boundaries for hybrid models. The vendor claims 82 percent less extra RAM and 2.2 times parallel chat throughput on an M3 Max with Qwen3.6-27B 4-bit. NOT TESTED here.

The proxy has no LM Studio rules except these.

- PRX-ENG-030: The proxy must treat an engine with no section in this file as an engine with unknown behaviour.
- PRX-ENG-031: The proxy must set the cap of a node with unknown behaviour to 1 until the calibration probe measures it. The overflow behaviour stays unknown until then.
- PRX-ENG-032: The proxy must not use a reuse signal from a node with unknown behaviour until the calibration probe finds the field.

### 9.3 Other facts for later

MTPLX (Apple, speculative decoding) runs a serial scheduler with `max_active_requests=1` ([D] R4 repository document). The Apple on-device shim has a 4096-token context, serial concurrency and needs a per-node max output patch (round 1, section 3.7). Both fit the rule of PRX-ENG-031 without change.

## 10. Hybrid models: Qwen 3.8 Flash-Next and Qwen 3.8 27B

All sizes below come from [R1](evidence/research/r1-models-20261007T022035Z.md). They are arithmetic from `config.json` unless marked [D]. The agent advised a check against llama-server log lines (KV buffer size, RS buffer size).

| Property | Flash-Next | 27B |
| --- | --- | --- |
| Type | MoE, 125B total, 6B active [D] | Dense, 27B [D] |
| Layers | 48 = 12 x (3 Gated DeltaNet plus 1 sparse attention) [D] | 64 = 16 x (3 Gated DeltaNet plus 1 gated attention) [D] |
| Layers with growing KV | 12 [D] | 16 [D] |
| KV per token, f16 | 24 KiB, plus about 0.75 KiB indexer [I] | 64 KiB [D in owner docs, I] |
| KV at 128k tokens | about 3 GiB [I] | about 8 GiB [I] |
| Recurrent state per sequence | about 110 to 115 MiB, constant [I] | about 150 MiB, constant [I] |
| Context | 262,144, extensible to 1M [D] | 262,144, extensible to about 1M [D] |
| MTP | 1 layer, 2 KiB per token [I] | 1 layer [D] |

The two models differ from a pure transformer in one way that matters here. KV rows can be cut at any token. The recurrent state cannot roll back. Reuse is valid only at a saved state ([R1](evidence/research/r1-models-20261007T022035Z.md), [I]).

The cost of a warm conversation on a node has three parts [I]. The first part is the KV. The second part is up to `ctxcp` checkpoints of 110 to 150 MiB each. The third part is one live state.

Unreconciled: an owner profile cites about 5 GB for 128k to 256k on a Strix Halo report, which is about 38 KiB per token. That is above the 24 KiB computed for Flash-Next. [U] R1.

S7 measured the sizes for the small model Qwen3.5-2B. A 6k conversation took about 129 MiB (91 MiB of state and two checkpoints of about 19 MiB). That matches the shape of the arithmetic above but not its scale.

Warm capacity arithmetic for Flash-Next and 27B stays an ASSUMPTION. The `--cache-ram` default of 8192 MiB holds about 54 states of 150 MiB or about 74 states of 110 MiB. One slot with 32 checkpoints of 150 MiB holds 4.7 GiB. R4 reports checkpoint sizes of 63 to 214 MiB for other models. So the default `--cache-ram` can hold fewer than one full checkpoint list for each slot at 4 slots.

The S7 sizes are NOT TRANSFERABLE. Spike B measures them on the owner machines.

Checkpoint sets by engine for the same turn, from the common model of s6 (`recompute(D) = N - max{c in C : c <= D}`, 0 if none):

| Engine | Set C | Granularity |
| --- | --- | --- |
| llama.cpp | `{L - 516, L - 4}`, user starts at least 8192 apart, live `L + G` | fine near the end, coarse before |
| vLLM | `{floor((P - 1) / B) * B}` per resident prompt, junctions | block size 528 to 2096 |
| SGLang | prefill chunk ends on a grid, decode multiples of 256, branch points, final state | 256 in the generated region |
| gufo | up to 4 grid points on 2048, stable boundary, full prompt, learned branches, live `L + G` | 2048 grid |
| mlx-lm | `{system end, L - 11, L + G}` | per message segment |
| mlx-vlm | final `L - 1` and one grid point on 2048, off by default | coarse |

Source: [s6 summary](evidence/spikes/s6-checkpoint-source-README.md) and the per-engine notes. The vLLM and SGLang rows are partly inferred in those notes.

Other facts that change affinity value (all from R1 to R4):

- Moving a conversation loses every checkpoint. A 60k-token prompt cost 47 s on gufo and about 230 s on llama.cpp on Strix Halo.
- Compaction rewrites history and always misses. pi compaction calls carry no session header.
- Speculative decoding with MTP and prefix caching can conflict. Reports exist for vLLM (#47861, #53912) and for the roadmap of SGLang. The owner measured MTP as a net loss (0.81 to 0.87 times) at 8 concurrent conversations on Strix Halo. NOT TESTED by this project.
- Spike 7 measured the hybrid cache behaviour of llama-server (section 4.5). The measured numbers in section 11 come from the 2B model and are NOT TRANSFERABLE for speed.

- PRX-ENG-033: The proxy must read the cache kind of a node (`dense`, `hybrid` or `unknown`) from the registry.
- PRX-ENG-034: The proxy must not treat a high common prefix as a high expected reuse on a node of kind `hybrid`.
- PRX-ENG-035: The proxy must send the bytes that become tokens unchanged to a node of kind `hybrid`. One changed token before a checkpoint loses all later checkpoints.
- PRX-ENG-036: The proxy must take the count of warm conversations of a node from the registry value `warm_capacity` or from the calibration probe. Changed 2026-10-07: the default equals the slot count. A larger value needs a declaration or a probe result (see [04](04-admission-control-and-queueing.md) section 4.5).

## 11. Cost table: tokens recomputed on llama.cpp

Source: S7, section 4.5. The model is Qwen3.5-2B on an Apple M2 under load. The token counts follow the formula of section 4.2 and are PROVEN for this model. They are an ASSUMPTION for Qwen 3.8 Flash-Next and 27B.

The times are PROVEN for this machine only and are NOT TRANSFERABLE. The earlier table of this file used an arithmetic example speed (172 tok/s) and checkpoints that no source gave. S7 replaced it.

`L` is the previous prompt. `N` is the new prompt. `D` is the common prefix. `c*` is the largest checkpoint at or below `D`.

| Case | `D` | `c*` | Recompute | Measured example | Label |
|---|---|---|---|---|---|
| Append only, state intact | `L + G` | `L + G` | `N - (L + G)`, plus 1 | 201 tokens for 200 new, about 0.9 s (hybrid and dense) | PROVEN, S7 |
| No change (exact retry) | `L` | `L - 4` | 4 | 4 tokens, 0.06 s at 6k | PROVEN, S7 |
| Template rewrites the last 2 tokens of the previous prompt | `L - 2` | `L - 4` | 4 plus new plus discarded tail | 332 tokens | PROVEN, S7 (synthetic history) |
| Edit 5 to 516 tokens before the end of `L` | `L - 516` to `L - 5` | `L - 516` | 516 | 516 tokens, 1.3 to 2 s | PROVEN, S7 |
| One token earlier than the checkpoint | `L - 517` | none or earlier | `N` | 6000 tokens, 15.0 s, against 516 tokens at `D = 5484` | PROVEN, S7 (3 of 3) |
| Edit earlier, token route | below `L - 516` | none | `N` | 6000 in 15 to 17 s, 12000 in 34 to 38 s | PROVEN, S7 |
| Edit earlier, chat route, first user start only | 194 to 4730 | 133 | `N - 133` | 5942 tokens, about 22 s (6075 history). 12148 tokens, about 47 s (12281 history) | PROVEN, S7 |
| Edit earlier, chat route, user start at 8404 | 9354 or 11692 | 8404 | `N - 8404` | 3877 tokens, about 16.5 s | PROVEN, S7 |
| Edit of the last user message | last user start | last user start | about 20 | 20 tokens, 0.15 to 0.31 s | PROVEN, S7 |
| Compaction, a changed system prompt, trimmed tool results | near the start | none or 133 | about `N` | not run | ASSUMPTION from the rule |
| Conversation moves to another node | 0 | none | `N` | not run. A 60k prompt cost 47 s on gufo and about 230 s on llama.cpp (R4) | ASSUMPTION for the hybrid rule, R4 for the 60k figures |
| Slot reused and entry lost from `--cache-ram` | 0 | none | `N` | `-np 1 --cache-ram 0`: about 6300 tokens and 23 s every turn | PROVEN, S7 |
| Dense model, edit at `D` | `D` | not needed | `N - D` | `cache_n` equals `D` in every case | PROVEN, S7 (qwen3:1.7b) |

Three readings matter for the design.

- A hybrid model on llama-server pays the whole prompt for an edit more than about 516 tokens before the end. A user-message checkpoint before the edit is the only exception. The proxy must treat such an edit as a cold prefill.
- The proxy must keep prompts append-only on hybrid nodes. That covers compaction, trimmed tool results, a changed system prompt and a changed tool list.
- A second conversation on the same slot stays warm only when `--cache-ram` is on (section 4.5).

The s6 policy read from source (checkpoints at `L - 516`, `L - 4`, user starts at least 8192 apart, and the last user start) matched every S7 measurement.

- PRX-ENG-037: The proxy must record, for each request to a node of kind `hybrid`, the tokens recomputed (`timings.prompt_n` on llama.cpp) and the prompt tokens.
- PRX-ENG-038: The proxy must label a turn as cold when `timings.prompt_n` far exceeds the expected new tokens. The expected new tokens are the prompt tokens of this turn minus the prompt and completion tokens of the previous turn of the same conversation. The allowance above the expected value is 516 tokens on a hybrid node (PROVEN, S7). On other nodes it is a PROPOSED value. Changed 2026-10-07: the earlier rule used 50 percent of the prompt, which misses a 12k edit that recomputes 3877 tokens. [08-observability-and-admin.md](08-observability-and-admin.md) owns the cold-turn event.

## 12. Cache profile and calibration probe

### 12.1 The cache profile

The cache profile is the part of the measured profile of a node that describes prompt cache behaviour. Fields (all PROPOSED):

| Field | Meaning |
| --- | --- |
| `reuse_field` | The response field that reports reuse, or `none`. |
| `warm_reuse_ratio` | Reused tokens over prompt tokens on a replay turn. |
| `divergence_cost` | Recomputed tokens for each of the probe cases in section 11. |
| `generated_text_kept` | Whether the replay turn reuses up to `L + G` (true) or only up to the prompt checkpoint (false). |
| `checkpoint_floor` | The smallest recomputed count seen on a replay turn. |
| `prefill_rate` | Prompt tokens per second on a cold prompt. |
| `cold_cost_s` | Seconds to prefill a prompt of 8,000 tokens. PROPOSED size. |
| `checkpoint_distances` | The checkpoint distances from the end of the prompt (step 11). S7 value for llama-server: 516 and 4. |
| `warm_capacity` | Conversations kept warm at one time (steps 12 and 13). |
| `state_size_mib` | The state of one conversation, or `unknown`. S7 value: about 129 for a 6k hybrid conversation on the 2B model. |
| `affinity_pays` | True, false or unknown. Derived by the rule in section 12.3. |

### 12.2 What the probe measures at join and on change

The probe runs when a node joins. It also runs when the engine version, the model, the context or the declared flags change. It runs on an idle node. It uses synthetic text that carries no user data. Each step names the finding that justifies it.

1. Identity. Read the engine version and `/props` where it exists. Source: PRX-ENG-004.
2. Context. Send a prompt larger than the declared context per slot. Record whether the node answers 400, cuts the prompt or accepts it. Source: S3 finding 3 and round 1.
3. Truncation. Send a prompt of known token length and read the prompt token count in `usage`. Source: PRX-ENG-018.
4. Concurrency. Send a decode-bound load (a short prompt, about 128 output tokens) at N = 1, 2, 4, 8. Choose the knee as the smallest N after which aggregate speed grows less than 1.15 times each time N doubles. Record speed per request. Do not use a prefill-bound load: its gains of 1.1 to 1.5 times lie inside the 2 times noise. Source: S3 consequence for story 59.
5. Prefill contention. Send one long prompt alone, then with a second long prompt. Record the ratio of the first time to first token. Source: S3 finding 7 (2.1 to 2.4 times on this machine).
6. Reuse field. Send a prompt twice and look for each known reuse field in the second response.
7. Replay turn. Send a prompt, let the node answer, then send the same history plus a new message. Record the reused tokens. This measures `generated_text_kept`.
8. Divergence turns. Repeat step 7 with an edit at 3, 100, 510, 520 and 5,000 tokens before the end. Repeat it with the reasoning of the last answer removed. Record the recomputed tokens. These are the cases of section 11.
9. Eviction depth. Send enough short distinct conversations to fill the declared slots plus one. Then replay the first. Record whether the first still reuses. This is a first measure of warm capacity.
10. Tool-call smoke, optional, informational only. See [07-registry-and-configuration.md](07-registry-and-configuration.md) section 9.
11. Checkpoint positions. Send a prompt of known length `N`. Then send variants that diverge at `D` values around `N - 4` and `N - 516`. Find the largest `D` that recomputes `N` and the smallest `D` that recomputes less. The result gives the checkpoint distances from the end (516 and 4 in S7). Use the token route where the engine has one, and the chat route otherwise.
12. Second conversation. Send conversation A, then conversation B, then a replay turn of A on a node with one slot. Record the recomputed tokens of A. A recompute near the new tokens means the node keeps A warm in host memory. A recompute near `N` means the node loses A when B uses the slot. This measures the `warm_capacity` and finds `--cache-ram` 0.
13. State size. Read the state size of one conversation where the node reports it (a log line or a metric). Where the node does not report it, record `unknown`. Use the size and the memory budget to bound `warm_capacity` (see section 4.5 of [04](04-admission-control-and-queueing.md)).

Steps 7 to 9 and 11 to 13 use the same fixed request patch as live traffic. The probe then sees the cache that live traffic sees. Steps 11 to 13 follow the method of spike 7.

- PRX-ENG-039: The proxy must run the calibration probe on a node before it routes live requests to that node. If it does not, the proxy must mark the node `uncalibrated` and use only declared values.
- PRX-ENG-040: The proxy must send probe traffic only to a node with no live request in flight.
- PRX-ENG-041: The proxy must send probe prompts that contain no text from live conversations.
- PRX-ENG-042: The proxy must store the probe result with the engine version, the model, the context, the patch and the declared flags that applied.
- PRX-ENG-043: The proxy must choose the concurrency knee from a decode-bound load and from aggregate speed.
- PRX-ENG-044: The proxy must stop a probe that runs longer than the probe budget and must mark the unfinished steps `not measured`. The default budget is 600 s. PROPOSED.
- PRX-ENG-057: The proxy must measure the checkpoint positions of a `hybrid` node in the probe (step 11). It must store them as distances from the end of the prompt.
- PRX-ENG-058: The proxy must measure in the calibration probe whether a second conversation stays warm after the slot serves another conversation (step 12).
- PRX-ENG-059: The proxy must record the state size of one conversation in the cache profile (step 13). The value is `unknown` when the node does not report it.
- PRX-ENG-060: The proxy must set `warm_capacity` in the cache profile from steps 12 and 13, with a maximum of the registry value.
- PRX-ENG-071: The proxy must run the replay turn of step 7 on every local node in the calibration probe. It must store the reused tokens in `warm_reuse_ratio`.
- PRX-ENG-072: The proxy must send two requests that share a prefix in the calibration probe. It must report a finding when the second request fails or when the node does not answer after it.
- PRX-ENG-075: The proxy must not compute the state size of a conversation from model arithmetic. It must read the size from an engine log, from a metric or from the measured profile.

### 12.3 How the proxy detects that affinity does not pay

Affinity pays on a node when the time saved by a warm turn is larger than the cost to keep a conversation on one node. The proxy decides in two stages.

Stage 1 runs at join, from the probe. The value `affinity_pays` is false in three cases. The first case is `reuse_field` equal to `none`. The second case is `warm_reuse_ratio` on step 7 below 0.5. The third case is `cold_cost_s` below the queue wait that affinity can add.

The threshold 0.5 is a PROPOSED value. The queue term needs a number from [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md).

Stage 2 runs in live traffic. The proxy keeps a rolling reuse ratio per node. The ratio covers the last 50 turns that affinity routed and that were not the first turn of a conversation. The window size is a PROPOSED value.

If the ratio falls below 0.5 for the whole window, the node gets plain least-loaded routing and no protected window. The proxy continues to measure and restores affinity when the ratio rises above 0.7. PROPOSED thresholds, with a gap so that a node does not switch back and forth.

The cause cannot be told apart from the response fields. A low ratio has four possible causes. They are a cache capacity limit, a template that rewrites history, a silent zero hit bug (vLLM #45238), and a harness that changes the prefix. The proxy only records the effect. [14-open-questions-and-risks.md](14-open-questions-and-risks.md) holds the open question of per-engine warm capacity.

- PRX-ENG-045: The proxy must set `affinity_pays` to false on a node whose probe finds no reuse field.
- PRX-ENG-046: The proxy must keep a rolling reuse ratio per node from the response fields of section 3.
- PRX-ENG-047: The proxy must switch a node to plain least-loaded routing. The condition is a rolling reuse ratio below the low threshold for a full window.
- PRX-ENG-048: The proxy must restore affinity on a node when the rolling ratio is above the high threshold.
- PRX-ENG-049: The proxy must report each switch, with the ratio and the window, in the event log and in the admin read.
- PRX-ENG-073: The proxy must report a node in the admin read. The condition is a rolling reuse ratio below its `warm_reuse_ratio` from the probe for a full window. PROPOSED.

## 13. Optimised specialisation: the boundary

The vision says that where tuning is possible, the proxy builds it in, so that endpoints run well. The proxy has no control of the engine process. So the work divides into three parts.

### 13.1 What the proxy tunes per request

The proxy tunes only the request body, with a fixed patch per node. The patch is the same on every turn of a conversation, because a changed prompt breaks the cache.

| Tuned item | Example | Engine |
| --- | --- | --- |
| Model name | rewrite `model` from the alias to the node model | all |
| Thinking switch | `chat_template_kwargs.enable_thinking` | llama-server |
| Thinking switch | `reasoning_effort` or `think` | Ollama |
| Output limit | clamp `max_tokens` or `max_completion_tokens` to a per-node value | all, Apple shim needs it |
| Sampling defaults | set `temperature` when the node declares one | all |

The proxy must not tune anything that changes the tokens of messages, tool definitions, tool order or the system prompt.

- PRX-ENG-050: The proxy must apply the same request patch of a node to every request that goes to that node.
- PRX-ENG-051: The proxy must not change the messages, the tool definitions, the tool order or the system prompt of a request.
- PRX-ENG-052: The proxy must reject at load a patch that touches `messages`, `tools`, `system` or `model`. The error is in [07-registry-and-configuration.md](07-registry-and-configuration.md).

### 13.2 What the proxy checks and reports

The proxy cannot read most engine flags. It reads what it can and it infers the rest from probe behaviour. The declared flag values in the registry are the owner's statement. A check compares the declared value with the observed behaviour and the arithmetic below. A failed check is a finding in the admin read. It never stops traffic.

| Setting | How the proxy checks it | Label |
| --- | --- | --- |
| `--jinja` (llama-server) | A request with tools returns 500 `tools param requires --jinja flag` when it is off. The probe sends a tool request. | PROVEN, S4 |
| `-np` and `total_slots` | Read `/props`. Compare with the declared slots. | PROVEN, S3 |
| Context per slot | `-c` divided by `-np` for an explicit `--parallel`. A test prompt above it must get a 400. | PROVEN, S3 |
| Unified KV | `kv_unified` true means each slot can use the whole `-c`. Visible in the engine log, not by a stable endpoint. | PROVEN in the log, not read by API |
| `--ctx-checkpoints` | Cannot be read. The probe infers it from step 8. Finding: `32 * slots * state size` against `--cache-ram`. | PROVEN default, inferred check |
| `--cache-ram` budget | Cannot be read. Finding when `slots * ctxcp * state_size` is above it, with the state size from the probe (step 13) or the model profile. | ASSUMPTION for large models, state size is [I] |
| `--checkpoint-min-step` | Cannot be read. The default is 8192 and a user-message checkpoint needs a gap of at least that size. Inferred from steps 8 and 11. A lower value (1024 in S7) gives user-message checkpoints at about 19 MiB each | PROVEN default and effect (S7). The inference: PROPOSED |
| `--cache-ram` 0 | Cannot be read. Step 12 finds it. Finding: every turn of a second conversation on the slot is cold, about 23 s at 6k in S7 | PROVEN, S7 |
| `--ctx-checkpoints` default 32 | Cannot be read. Eviction beyond 32 checkpoints is NOT TESTED | PROVEN default, NOT TESTED beyond |
| `--parallel` or `-np` | Read `/props total_slots`. With `-np 1` and `--cache-ram` above 0, two conversations stay warm (S7) | PROVEN, S3 and S7 |
| `OLLAMA_NUM_PARALLEL` | Cannot be read. Compare the declared value with the measured knee. | PROVEN, S3 |
| Ollama keep-alive | Cannot be read. Finding when the declared keep-alive is shorter than the protected window of [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md), because an unloaded model loses its cache. | PROPOSED |
| `--enable-prompt-tokens-details` (vLLM) | The probe looks for the field in a response. | PROVEN flag, source |
| `max_num_seqs`, `--sessions` | Declared only. The probe measures the knee. | PROPOSED |

- PRX-ENG-053: The proxy must compare each declared flag of a node with the observed behaviour at join. The proxy must report each difference as a finding in the admin read.
- PRX-ENG-054: The proxy must state in each finding the declared value, the observed value, and the setting the owner can change.
- PRX-ENG-055: The proxy must keep findings advisory. A finding must not stop or redirect traffic by itself.
- PRX-ENG-061: The proxy must report a finding when the probe shows `--cache-ram 0` on a llama-server node.
- PRX-ENG-067: The proxy must report a finding when the probe shows `--jinja` off on a llama-server node.
- PRX-ENG-068: The proxy must report a finding when `--checkpoint-min-step` is above 1024 on a hybrid llama-server node.
- PRX-ENG-069: The proxy must report a finding when `--ctx-checkpoints` needs more memory than the `--cache-ram` budget holds.
- PRX-ENG-070: The proxy must report a finding when `--parallel` differs from the declared slot count.

### 13.3 What the proxy never does

- PRX-ENG-056: The proxy must not start, stop, restart, load, unload or configure an engine process, an engine container or an engine environment variable.

- PRX-ENG-074: The proxy must read the signals and the cache fields of each engine through one adapter for that engine. The routing core must hold no engine name.

The same rule covers editing an engine configuration file, calling a model load or unload endpoint, and sending a signal to an engine. The proxy has no code path that does these. The owner or a separate tool applies the change that a finding recommends. This matches the v1 decision that the proxy does not start or manage engines.

## 14. Responses API by engine

Source: [R7](evidence/research/r7-responses-api-20261007T081115Z.md), read from source on 2026-10-07. Labels follow R7: D means documented (source or docs read), I means inferred, U means unverified. No engine ran against a live Codex conversation. Spikes C and D used a fake server. The pass-through decision is DEC-065 in [01](01-decisions.md).

| Engine and version | `/v1/responses` | State | Cached tokens in usage | Gaps against Codex |
|---|---|---|---|---|
| llama-server b11462 | Yes, since PR 18486 (D) | Stateless. `previous_response_id` gives 400 (D) | `input_tokens_details.cached_tokens` (D) | Issue 29159: a `reasoning` input item with `summary: null` gives 400 on every request. Drops `namespace` and `web_search` tools (issue 24295). A `custom_tool_call` item gives 400 (I). Ignores `prompt_cache_key`, `include` and `store`. No `sequence_number` |
| Ollama 0.40.0-rc6 | Yes, plus `/v1/responses/compact` (D) | Stateless. It accepts `previous_response_id` and silently ignores it (D) | Mapped from `PromptEvalCachedCount`. Real values U | Open issues: drops `developer` items (18305), custom tools (17673), namespace and tool search (15921, 18306), ordering (18798), empty reply to a tool follow-up (18419) |
| vLLM main, 2026-10-07 | Yes | Stateful only with `VLLM_ENABLE_RESPONSES_API_STORE=1`. The store is in memory in each API process (D) | `cached_tokens` set (D) | Ignores `prompt_cache_key`. Open Codex issues: 45273 (400 then 500 in multi-turn), 55659 (crash on `additional_tools`). Fix state U |
| SGLang main | Yes | `previous_response_id` needs `--enable-response-store` (D) | `cached_tokens` only with `enable_prompt_tokens_details` (D) | Puts reasoning in `encrypted_content` as a reversible encoding so that `store: false` harnesses can replay it. No Codex issue found (U) |
| mlx_lm.server a537041 | NO. It answers 404 (D, `server.py:1104`) | not applicable | not applicable | A node of this engine cannot serve Codex |
| mlx-vlm 4f4634b | Yes, plus `/responses/compact` (D) | Stateful when `store` is not false. Codex sends `store: false`, so the engine keeps nothing | Plumbing exists, U on `/v1/responses` | Own encrypted compaction capsule |
| LM Studio (0.3.29 and later) | Yes (D, docs) | Stateful: `previous_response_id` continues a server-side thread (D, docs) | U | Closed source. Codex has a built-in provider for it |
| gufo 33d1b20 | Yes (D, docs) | Stateless: `store` and `background` must be false (D) | U | Accepts `include`, `reasoning.summary` and `client_metadata`. Flattens `namespace`. Skips hosted tools. Closest to Codex-ready (D docs, U run) |
| LocalAI a3d5556 | Yes (D) | Stateful, with a time to live | `CachedTokens: 0` filler, real value U | Not tested with Codex |

Facts for the proxy:

- The first llama.cpp build with the endpoint is older than the brew build b11146. Brew 0.5.0 therefore has the endpoint (D from R7, tag not looked up).
- Codex always sends `store: false`. A stateful engine therefore keeps no state for Codex unless it ignores `store`. U for LocalAI and LM Studio.
- Engines that translate Responses to chat completions internally (llama-server, Ollama, probably LocalAI) show their known defects as translation defects. The proxy does not translate (DEC-065).

- PRX-ENG-062: The proxy must send a Responses request only to a node whose registry flag `responses` is true. The default of the flag is false.
- PRX-ENG-063: The proxy must report a finding for a node of engine mlx_lm with `responses` set to true, because the engine answers 404 on `/v1/responses`.
- PRX-ENG-064: The proxy must read the node flags `stateful_responses` and `ignores_previous_response_id` from the registry. Both default to false.
- PRX-ENG-065: The proxy must show the known gaps of each node with `responses` true in the admin read. The gaps come from the table of this section.
- PRX-ENG-066: The proxy must not state that an engine supports Codex until the real-session test of [13](13-test-fixtures-and-scenarios.md) passes for that engine and version.

## 15. Open items

- OPEN: the exact name of the cached-token field in Ollama 0.35.1 and 0.40.0 responses. A short probe settles it. S7 did not run Ollama.
- OPEN: overflow behaviour of vLLM, SGLang and gufo. ASSUMPTION only. The calibration probe step 2 measures it.
- OPEN: whether a typical agent history has user-message checkpoints at least 8192 tokens apart on llama.cpp. S7 saw one at 8404 in a synthetic 12k history. A recorded agent trace settles it.
- OPEN: warm conversations per node on each engine. S7 shows two for llama-server with `--cache-ram`. Eviction and the 8 GiB limit are NOT TESTED. Spike B and later spikes.
- OPEN: streaming usage and timing fields of llama-server for the cold-turn signal (NOT TESTED in S7).
- OPEN: effect of Ollama 0.40.0 on Mac (MLX runner) on every Ollama row here.
- OPEN: the behaviour of every engine in section 14 against a live Codex conversation (spike F, owner decision pending).
- OPEN: whether `cached_tokens` in SGLang counts the hit after Mamba truncation.

## Sources

- [R1 models](evidence/research/r1-models-20261007T022035Z.md), [R2 vLLM](evidence/research/r2-vllm-20261007T022033Z.md), [R3 SGLang and routing](evidence/research/r3-sglang-routing-20261007T022127Z.md), [R4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md)
- [R7 Responses API](evidence/research/r7-responses-api-20261007T081115Z.md)
- [S3 slots](evidence/spikes/s3-slots-README.md), [S4 smoke](evidence/spikes/s4-smoke-README.md), [S6 checkpoint source](evidence/spikes/s6-checkpoint-source-README.md), [S7 hybrid cache](evidence/spikes/s7-hybrid-cache/README.md)
- [llama.cpp notes](evidence/spikes/notes-llamacpp-20261007T054753Z.md), [errata](evidence/spikes/notes-llamacpp-errata-20261007T054827Z.md), [mlx notes](evidence/spikes/notes-mlx-20261007T054857Z.md), [gufo notes](evidence/spikes/notes-gufo-20261007T054857Z.md), [vLLM notes](evidence/spikes/notes-vllm-20261007T054827Z.md), [SGLang notes](evidence/spikes/notes-sglang-20261007T054827Z.md), [vLLM and SGLang errata](evidence/spikes/notes-vllm-sglang-errata-20261007T054857Z.md), [clones and commits](evidence/spikes/notes-clones-and-versions-20261007T054753Z.md)
- Round 1 engine facts: [spike decisions](../decisions/2026-10-spike-decisions.md) section 3.5
- Source commits: llama.cpp 78651c41 (b11460) and 7fe450e1 (b11146), vLLM 43b4aaea, SGLang f47d8956, gufo 33d1b208, mlx-lm a537041a, mlx-vlm 4f4634bb.
