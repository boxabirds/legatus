# Lessons learned

Status: draft for the proxy baseline, written 2026-10-07. Owner: writer W-F. This file lists painful facts that other projects, other engines and our own spikes taught. Each lesson has an ID, a problem, a source, a consequence for the design and the requirement area that it feeds.

## How to use this file

A lesson is not a requirement. A lesson explains why a requirement exists. The field "Feeds" names the requirement IDs that the lesson supports.

Each feed shows the real requirement ID. Where a lesson has no consequence for the proxy, the feed says "no proxy consequence".

Labels follow [10-research-and-prior-art.md](10-research-and-prior-art.md). PROVEN means read from a source or measured. ASSUMPTION means inferred or second-hand. NOT TESTED means nobody ran it. Each source line ends with the label and the evidence file.

Lessons from the parked capability manager (leases, guard, worktrees, dual clocks) are not here. They are in [16-parked-capability-manager.md](16-parked-capability-manager.md) and in the [archive](../archive/).

## Feed summary by requirement area

| Area | Lessons |
| --- | --- |
| PRX-SCOPE | LES-007, LES-036, LES-066 |
| PRX-PROTO | LES-074, LES-075, LES-066, LES-068, LES-069, LES-070, LES-019, LES-025, LES-026, LES-036, LES-037, LES-041, LES-044, LES-045, LES-046, LES-047 |
| PRX-KEY | LES-065, LES-066, LES-067, LES-012, LES-033, LES-039, LES-044, LES-046, LES-047 |
| PRX-AFF | LES-071, LES-072, LES-001, LES-002, LES-005, LES-007, LES-012, LES-013, LES-029, LES-031, LES-034, LES-035, LES-043, LES-050, LES-051, LES-052, LES-054, LES-056 |
| PRX-ADM | LES-072, LES-068, LES-069, LES-070, LES-005, LES-010, LES-020, LES-021, LES-023, LES-024, LES-028, LES-030, LES-032, LES-034, LES-040, LES-042, LES-048, LES-049, LES-050, LES-056, LES-057 |
| PRX-ENG, Responses | LES-074, LES-075 |
| PRX-ENG, hybrid cache | LES-071, LES-072, LES-073 |
| PRX-ENG | LES-001, LES-002, LES-003, LES-004, LES-005, LES-006, LES-008, LES-009, LES-010, LES-011, LES-014, LES-016, LES-017, LES-018, LES-019, LES-020, LES-021, LES-022, LES-023, LES-024, LES-026, LES-027, LES-038, LES-063, LES-064 |
| PRX-REG | LES-072, LES-002, LES-003, LES-004, LES-008, LES-010, LES-014, LES-018, LES-021, LES-022, LES-027, LES-031, LES-038, LES-051, LES-063 |
| PRX-OBS | LES-071, LES-073, LES-001, LES-003, LES-006, LES-011, LES-013, LES-016, LES-029, LES-047, LES-048, LES-055 |
| PRX-REST | LES-040, LES-041, LES-042, LES-043, LES-045, LES-053, LES-054, LES-055, LES-057 |
| PRX-SEC | LES-036, LES-048 |
| PRX-TEST | LES-065, LES-066, LES-004, LES-008, LES-009, LES-015, LES-022, LES-027, LES-028, LES-058, LES-059, LES-060, LES-061, LES-062, LES-064 |
| PRX-PERF | LES-025, LES-039, LES-057, LES-060 |

## A. Hybrid models and prompt caches

### LES-001 Silent zero cache hits on vLLM hybrids
- Problem: In align mode vLLM keeps one recurrent state for each prompt. If that state falls in tokens that are unique to the request, the engine drops all reuse, even for matching attention blocks. The status stays 200 and no metric shows it.
- Source: vLLM issue 45238, open, filed 2026-06-11 (https://github.com/vllm-project/vllm/issues/45238). Related: issue 51250 (zero hits on Qwen 3.5 35B-A3B) and issue 40696 (prompts under 528 tokens give about zero hits). PROVEN ([r2](evidence/research/r2-vllm-20261007T022033Z.md), [s6 vLLM notes](evidence/spikes/notes-vllm-20261007T054827Z.md)).
- Consequence: A warm node is not a proven hit. The proxy measures reuse from every response. A node with poor measured reuse receives plain least-loaded routing and no protected window.
- Feeds: PRX-AFF-025, PRX-AFF-026 (poor reuse fallback), PRX-OBS-023, PRX-OBS-026 (cold-turn detection), PRX-ENG-024, PRX-ENG-025.

### LES-002 The align mode costs throughput on vLLM
- Problem: Align mode splits prefills at block boundaries and runs eager kernels. It cost 13 to 16 percent of output throughput even with zero hits, on Nemotron-3.5-Lightning. Nobody measured Qwen 3.8 models.
- Source: vLLM issue 60008, open on 2026-10-02 (https://github.com/vllm-project/vllm/issues/60008). PROVEN for the tested model ([r2](evidence/research/r2-vllm-20261007T022033Z.md)). The effect on Qwen 3.8 is NOT TESTED. Spike 7 did not run vLLM.
- Consequence: Affinity has a price on some engines. The proxy must not assume that more caching is better. The registry must record whether prefix caching is on for each node.
- Feeds: PRX-ENG-047, PRX-AFF-025, PRX-REG-045.

### LES-003 vLLM hides the cached token count by default
- Problem: The field `prompt_tokens_details.cached_tokens` exists in vLLM but the flag `--enable-prompt-tokens-details` defaults to off. Without it the proxy sees no hit data.
- Source: `launchers/cli_args.py:148` at commit 43b4aaea. PROVEN ([s6 errata](evidence/spikes/notes-vllm-sglang-errata-20261007T054857Z.md)).
- Consequence: The registry must declare the flag for each vLLM node. The proxy must report "no signal" and must not report "zero hits" when the field is missing.
- Feeds: PRX-ENG-024, PRX-REG-047, PRX-OBS-023.

### LES-004 llama.cpp re-reads the whole prompt every turn on hybrids
- Problem: Several issues report a cold prefill on each turn for Qwen hybrids. Partial fixes did not close them. The cause in issue 22384 was a checkpoint search that never matched for recurrent models.
- Source: llama.cpp issues 19794 (closed, not planned), 20225 (partial fixes only), 21831 (closed, not planned) and 22384 (closed). Example: 12,146 tokens took 11 s, then 31 tokens took 115 ms after a fork fix. See https://github.com/ggml-org/llama.cpp/issues/22384. PROVEN as reports ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)). [s6](evidence/spikes/notes-llamacpp-20261007T054753Z.md) reads the fix state in current upstream code.
- Consequence: Engine version matters for cache reuse. The registry records the engine version. The calibration probe must test a second turn on each node and record the reuse.
- Feeds: PRX-ENG-001, PRX-ENG-003, PRX-REG-024, PRX-ENG-071.

### LES-005 A diverging request destroys later checkpoints
- Problem: In llama.cpp a request that diverges at position D removes every checkpoint of the slot after D. The checkpoints are a staircase. A change in generated text loses the generated tokens plus 4. A failed checkpoint also stays silent (second-hand report: status 200, one rejected request removed 15 checkpoints).
- Source: s6 llama.cpp reading, steps 3 and 4, with errata (`server-context.cpp` lines 3610 to 3628 at b11460). PROVEN ([s6](evidence/spikes/notes-llamacpp-20261007T054753Z.md), [errata](evidence/spikes/notes-llamacpp-errata-20261007T054827Z.md)). The silent failure report is ASSUMPTION ([r4](evidence/research/r4-local-engines-20261007T022141Z.md), blog).
- Consequence: A side request on the same slot can ruin the warm state of the main conversation. The proxy must give each conversation its own key and must not send two conversations to one slot when it can avoid it.
- Feeds: PRX-AFF-032, PRX-ADM-015, PRX-ADM-019, PRX-ENG-035.

### LES-006 llama.cpp `tokens_cached` is not the reused count
- Problem: The field `tokens_cached` equals the prompt plus the generated tokens. It does not show what the request reused.
- Source: s6 llama.cpp reading (`server-task.cpp:349-356`). PROVEN ([s6](evidence/spikes/notes-llamacpp-20261007T054753Z.md)). The fields to use are `timings.cache_n` and `timings.prompt_n`.
- Consequence: The proxy reads `timings.cache_n` and `prompt_n` for llama.cpp and gufo. The spec forbids `tokens_cached`.
- Feeds: PRX-OBS-021, PRX-ENG-008, PRX-ENG-009.

### LES-007 Slot save and restore does not carry hybrid checkpoints
- Problem: The restore of a 14,906-token prefix took 0.12 s and then recomputed everything.
- Source: llama.cpp issue 25913 (https://github.com/ggml-org/llama.cpp/issues/25913), build 10068. PROVEN as a report ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)).
- Consequence: A move between nodes is always a cold prefill. The proxy offers no transfer of cache state. Strict stickiness is the only protection.
- Feeds: PRX-AFF-012, PRX-SCOPE-008.

### LES-008 Partial-prefix reuse on Flash-Next crashes llama.cpp
- Problem: Without speculative decoding the value `n_rs_seq` is 0 and `seq_rm` returns false. The engine does not check this. The second request that shares a prefix crashes or leaks.
- Source: llama.cpp issue 28425, open on 2026-09-05 (https://github.com/ggml-org/llama.cpp/issues/28425). PROVEN as a report ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)).
- Consequence: The proxy cannot assume that a shared prefix is safe on a new model. The calibration probe must send two requests with a shared prefix at join time. The registry holds the result.
- Feeds: PRX-ENG-039, PRX-ENG-042, PRX-REG-031, PRX-ENG-072.

### LES-009 ROCm carries recurrent state across requests
- Problem: On ROCm a reused slot kept the Gated DeltaNet state of the earlier request.
- Source: llama.cpp issue 29092 (https://github.com/ggml-org/llama.cpp/issues/29092), as named in [r1](evidence/research/r1-models-20261007T022035Z.md). PROVEN that the issue exists. The details are NOT TESTED here.
- Consequence: The proxy cannot detect wrong output. Real-engine tests must include a divergence check on ROCm nodes. The registry records the engine and the machine.
- Feeds: PRX-TEST-081, no proxy consequence (the registry already records the engine and the machine).

### LES-010 The host cache can be smaller than the checkpoints
- Problem: The llama-server host cache defaults to 8192 MiB. Checkpoints are 63 to 214 MiB each and the default allows 32 for each slot. mlx-lm keeps 10 entries with no byte limit. Nobody knows how many warm conversations fit on a node.
- Source: s6 source reading for the defaults (`common.h:635-638`, `server.py:1772`). PROVEN. The size range comes from a blog and is ASSUMPTION ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)).
- Update 2026-10-07: [spike 7](evidence/spikes/s7-hybrid-cache/README.md) measured two warm conversations on one llama-server slot with the default `--cache-ram`. A 6k hybrid conversation took about 129 MiB on Qwen3.5-2B. PROVEN for that model. See LES-072.
- Consequence: Warm capacity beyond two conversations is an open measurement (spike B). The protected window must not promise more warm conversations than the node holds.
- Feeds: PRX-ENG-060, PRX-ENG-069, PRX-ADM-049, PRX-ADM-050, PRX-REG-042, PRX-REG-043.

### LES-011 Every engine checkpoints in its own way
- Problem: The positions that survive differ by engine. The llama-server keeps L minus 516, L minus 4 and user-message starts. vLLM keeps one state per prompt on a block grid. The grid is 528 to 2096 tokens. SGLang tracks a grid and every 256 decode tokens. gufo keeps up to 4 grid points and learned branches. The mlx-lm engine keeps segment ends.
- Source: [s6 README](evidence/spikes/s6-checkpoint-source-README.md) and the per-engine notes. PROVEN (source read, no run).
- Consequence: One cost formula covers all engines: recompute(D) = N minus the largest checkpoint position at most D. The proxy reports it. It does not tune a checkpoint policy.
- Feeds: PRX-ENG-037, PRX-ENG-057, PRX-OBS-026.

### LES-012 A side request evicts the main conversation on a single slot
- Problem: Hybrid combinations run with one slot. Any other conversation on that slot replaces the main prompt cache. Titles, compaction calls and subagents are such conversations.
- Source: owner note `awesome-local-ai/docs/research/20260917-nadirclaw-vs-semantic-router.md` (lines 361 to 374). ASSUMPTION (code-level reasoning, not measured), as quoted in [r6](evidence/research/r6-routers-20261007T055357Z.md).
- Consequence: Side requests are different conversations. The proxy keeps them off the main conversation seat when another seat is free.
- Feeds: PRX-KEY-022, PRX-AFF-018.

### LES-013 A move costs a full prefill, and the cost differs by a factor of five
- Problem: The same 60k-token request took 47 s on gufo and 230 s on llama.cpp on Strix Halo. A compaction re-read took 6 to 8 minutes on llama.cpp.
- Source: owner docs `20260928-gufo-long-session-investigation.md` and `20260927-strix-halo-llamacpp-findings.md`, read by [r4](evidence/research/r4-local-engines-20261007T022141Z.md). PROVEN (owner measurements).
- Consequence: A conversation moves only when its node is unavailable. The proxy logs each move with its estimated cost.
- Feeds: PRX-AFF-012, PRX-AFF-032, PRX-OBS-058.

### LES-014 Multi-token prediction and prefix caching conflict
- Problem: vLLM 0.28.0 corrupted output with the prompt cache and multi-token prediction on hybrids. SGLang benchmarks run spec decode with `--disable-radix-cache`. The owner measured MTP as a net loss (0.81 to 0.87 times) at 8 concurrent conversations on Strix Halo.
- Source: vLLM issues 47861 and 53912 (merge status NOT TESTED), SGLang PyTorch blog, owner repo. PROVEN that the reports exist ([r2](evidence/research/r2-vllm-20261007T022033Z.md), [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md), [r1](evidence/research/r1-models-20261007T022035Z.md)).
- Consequence: The registry records speculative decoding for each node. Affinity value on such a node is NOT TESTED and is an open item.
- Feeds: PRX-REG-046, PRX-ENG-073.

### LES-015 Cached and fresh outputs are not bit-identical
- Problem: bf16 truncation makes a warm run differ slightly from a cold run. LMCache says the same.
- Source: vLLM pull request 26807 and the LMCache docs. PROVEN as a report ([r2](evidence/research/r2-vllm-20261007T022033Z.md)).
- Consequence: Tests must not compare output text to prove a cache hit. Tests check the hit fields.
- Feeds: PRX-TEST-082.

### LES-016 The cached count can stay at zero while caching is on
- Problem: A report shows `cached_tokens` 0 with `cache_prompt` true while a multimodal projector was loaded.
- Source: Bonsai-demo issue 147, cited in the owner note on concurrent conversations. The cause is NOT TESTED ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)).
- Consequence: Reuse measurement needs a baseline for each node. The proxy compares a node with its own history.
- Feeds: PRX-ENG-073, PRX-ENG-046.

## B. Local engine behaviour

### LES-017 `/slots` wakes a sleeping llama-server
- Problem: A request to `/slots` loads the model again when `--sleep` is on. The requests `/metrics`, `/health` and `/props` do not.
- Source: round 1 engine log, cited in [s3](evidence/spikes/s3-slots-README.md) (finding 3). PROVEN in round 1. Not repeated in spike 3.
- Consequence: The proxy polls `/metrics` and `/props` only. It never calls `/slots` on a node with `--sleep`.
- Feeds: PRX-ENG-006.

### LES-018 The `-np` flag divides the context
- Problem: With `-c 16384 -np 4` each slot has 4096 tokens. A 6850-token prompt received a 400 error `exceed_context_size_error`. Default slots with a unified KV let each slot use the whole context.
- Source: [s3](evidence/spikes/s3-slots-README.md) finding 3. PROVEN (llama-server 0.5.0).
- Consequence: The effective context is per slot. The registry must hold the per-slot value. The proxy compares the prompt estimate with that value.
- Feeds: PRX-ENG-007, PRX-REG-027.

### LES-019 Ollama truncates silently
- Problem: The default context of 4096 tokens cut a long prompt to about 2050 tokens. The status was 200.
- Source: round 1 engine matrix in the [spike decisions](../decisions/2026-10-spike-decisions.md) (section 3.5). PROVEN (Ollama 0.35.1).
- Consequence: The proxy checks the prompt token estimate against the loaded context of an Ollama node. It answers with a context error instead of sending the request.
- Feeds: PRX-ENG-015, PRX-ENG-016, PRX-PROTO-034.

### LES-020 Ollama runs one request at a time and queues without headers
- Problem: Ollama defaults to one parallel request. A queued request receives no headers until service starts. One queued request waited 44 s for its first byte. The default queue holds 512 requests and then answers 503.
- Source: [s3](evidence/spikes/s3-slots-README.md) findings 1 and 2, and the scope reset record (item 9). PROVEN (Ollama 0.35.1). The queue limit comes from the Ollama FAQ ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)).
- Consequence: The proxy holds requests in its own queue. It does not rely on the engine queue. A harness timer fires before service starts on an engine queue.
- Feeds: PRX-ADM-020, PRX-ENG-012, PRX-ENG-017.

### LES-021 Ollama and mlx_lm give no load signal
- Problem: `/api/ps` has no busy or queue field. The value `OLLAMA_NUM_PARALLEL` has no discovery. mlx_lm has no `/props`, `/metrics` or `/slots`.
- Source: [s3](evidence/spikes/s3-slots-README.md) findings 1 and 4. PROVEN.
- Consequence: The proxy counts its own in-flight requests for these engines. The registry must declare the slot count of an Ollama node.
- Feeds: PRX-ENG-013, PRX-ENG-021, PRX-REG-026, PRX-ADM-005, PRX-ADM-006.

### LES-022 Engine versions drift and invalidate results
- Problem: Ollama updated itself during a test. The brew package of llama.cpp was one release behind. Ollama v0.40.0 now runs models on MLX by default on Apple Silicon.
- Source: decision D-5 in the [spike decisions](../decisions/2026-10-spike-decisions.md) and [s4](evidence/spikes/s4-smoke-README.md) versions table. PROVEN. The v0.40.0 behaviour is from release notes ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)).
- Consequence: The registry records the engine version of each node. Tests use frozen engine builds. A version change marks the measured profile as stale.
- Feeds: PRX-REG-032, PRX-ENG-001, PRX-ENG-002, PRX-TEST-063.

### LES-023 mlx_lm batches requests but does not raise the cap
- Problem: mlx_lm ran 8 requests at once. The total speed stayed near 55 tokens per second and each request slowed by N. The engine does not honour a cancelled prefill. The engine dropped the connection on an invalid request. The engine wedged once under heavy load.
- Source: [s3](evidence/spikes/s3-slots-README.md) finding 4 and the [spike decisions](../decisions/2026-10-spike-decisions.md) (sections 3.5 and 6). PROVEN (mlx_lm 0.32.0, Qwen3 1.7B).
- Consequence: The default cap for mlx_lm is 1. The proxy cannot cancel work on the engine.
- Feeds: PRX-ADM-006, PRX-ENG-020, PRX-ENG-022.

### LES-024 Slots do not give throughput, and prefill is one shared pool
- Problem: The best gain was 1.6 to 1.8 times at 4 slots on decode-bound traffic. A second long prompt delayed the first by 2.1 to 2.4 times. A prefill-bound probe picks the wrong knee.
- Source: [s3](evidence/spikes/s3-slots-README.md) findings 5 to 8. PROVEN on one M2 with a 1.7B model. Larger models and other hardware are NOT TESTED.
- Consequence: The cap is the measured efficient concurrency and not the slot count. The calibration uses a decode-bound probe. Queue wait is the price of low time to first token.
- Feeds: PRX-ADM-002, PRX-ENG-043.

### LES-025 llama-server closes idle connections after 5 s
- Problem: A client that reused a connection received `fetch failed` at 5 ms.
- Source: [s3](evidence/spikes/s3-slots-README.md) finding 9. PROVEN (one occurrence).
- Consequence: The proxy must not reuse a node connection older than the engine keep-alive time. Without retries, a stale connection becomes a visible failure.
- Feeds: PRX-ENG-010, PRX-TEST-083.

### LES-026 Engines report errors in different shapes
- Problem: llama-server 0.5.0 answers 500 for malformed JSON. pi retries a 500. mlx_lm drops the connection on invalid requests. llama-server answers 400 for context overflow. The names of the thinking fields differ: `reasoning` on Ollama and mlx_lm, `reasoning_content` on llama-server.
- Source: [spike decisions](../decisions/2026-10-spike-decisions.md) section 3.5. PROVEN (versions in that file).
- Consequence: The proxy passes engine errors through unchanged and logs the engine and the shape. A fixture records each shape.
- Feeds: PRX-PROTO-009, PRX-ENG-011.

### LES-027 Thinking switches differ by engine and can fail without a signal
- Problem: The score of the same GGUF changed from 0.70 to 0.96 with thinking. Ollama and llama-server use different switches. Thinking text leaked into `content` with status 200. The `qwen3:4b` library build cannot turn thinking off. A custom tool-less template did not take effect on Ollama.
- Source: [s4](evidence/spikes/s4-smoke-README.md) "Which settings matter". PROVEN (small models, one machine).
- Consequence: The per-node patch never changes and is the same on every turn. The registry holds the switch for each node. Tests judge content and not only status.
- Feeds: PRX-ENG-050, PRX-REG-013, PRX-TEST-084.

## C. Routers and gateways

### LES-028 A burst piles on one worker
- Problem: The SGLang gateway does not see running requests and uses stale throughput. A burst sent 29 requests to one worker and 3 to another.
- Source: smg issue 2804 (https://github.com/smg-project/smg/issues/2804). PROVEN as a report ([r3](evidence/research/r3-sglang-routing-20261007T022127Z.md)).
- Consequence: The proxy counts a request as in flight from the moment it admits it. The count is exact for the proxy. Burst tests are a required scenario.
- Feeds: PRX-ADM-008, PRX-TEST-020.

### LES-029 Router trees do not see evictions
- Problem: The SGLang gateway, the production-stack prefix router and the llm-d approximate scorer guess what a node caches. They receive no feedback from the engine. llm-d measured 13.3 s mean time to first token with the approximate scorer. The precise scorer gave 0.30 s.
- Source: SGLang issue 7532, production-stack router docs and the llm-d blog (https://llm-d.ai/blog/kvcache-wins-you-can-see). PROVEN (the llm-d numbers are a vendor benchmark on H100 GPUs) ([r2](evidence/research/r2-vllm-20261007T022033Z.md), [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md)).
- Consequence: The table is a guess too. The proxy checks the guess against the cache fields of each response and ends the protected window of a node that does not reuse.
- Feeds: PRX-AFF-025, PRX-OBS-021.

### LES-030 Count the load before dispatch
- Problem: An SGLang router counted load only after the response started. Fast decode and slow prefill then collapsed onto one worker.
- Source: sglang pull request 27547 (https://github.com/sgl-project/sglang/pull/27547). PROVEN ([r3](evidence/research/r3-sglang-routing-20261007T022127Z.md)).
- Consequence: The in-flight counter rises at admission and falls on completion, error or harness disconnect.
- Feeds: PRX-ADM-008.

### LES-031 A fixed affinity time breaks long caches
- Problem: LiteLLM hard-coded the affinity time to 5 minutes. This breaks a 1 hour cache. The fix matches the time to the cache control time.
- Source: LiteLLM issue 28427 and pull request 40776 (https://github.com/BerriAI/litellm/issues/28427). PROVEN ([r3](evidence/research/r3-sglang-routing-20261007T022127Z.md)).
- Consequence: The idle time of a table entry is a parameter for each route and each node. It has no hidden constant.
- Feeds: PRX-AFF-008, PRX-REG-048.

### LES-032 A cap that answers 429 hands the problem to the harness
- Problem: The LiteLLM concurrency cap raises a 429 error and does not wait. The llama-swap program answers 429 at its limit and does not queue. pi retries a 429 for only 14 s.
- Source: LiteLLM `client_initalization_utils.py:16-46` and llama-swap `internal/server/concurrency.go`. PROVEN (source read, [r5](evidence/research/r5-gateways-20261007T055410Z.md), [r6](evidence/research/r6-routers-20261007T055357Z.md)). The pi window is from [s5](evidence/spikes/s5-pi-restart-README.md).
- Consequence: The proxy holds a request when the node is full. HAProxy `maxconn` with `timeout queue` is the model.
- Feeds: PRX-ADM-007, PRX-ADM-020.

### LES-033 Olla keys collide
- Problem: The Olla prefix hash covers the first 512 bytes of the `messages` array. On the OpenAI path the system prompt fills those bytes, so different conversations from one harness share a key. The key cannot tell a title request from the main conversation.
- Source: Olla `internal/adapter/balancer/sticky.go` (https://github.com/thushan/olla). PROVEN (source read, [r6](evidence/research/r6-routers-20261007T055357Z.md)). The collision is an ASSUMPTION from the code, NOT TESTED.
- Consequence: The key hashes the first system message and the first non-system message, in full. A test fixture must hold two conversations with one system prompt.
- Feeds: PRX-KEY-011, PRX-KEY-045.

### LES-034 A sticky table with no cap overloads one node
- Problem: Olla returns the pinned endpoint whenever it is routable. It has no per-node cap and no spill. All pinned conversations then queue inside one engine.
- Source: Olla `sticky.go` lines 90 to 118. PROVEN ([r6](evidence/research/r6-routers-20261007T055357Z.md)).
- Consequence: Strictness needs a cap and a hold queue next to it. New conversations choose a node by free seats. Existing conversations stay.
- Feeds: PRX-AFF-013, PRX-ADM-007, PRX-ADM-023.

### LES-035 Hash rings are coarse at five nodes
- Problem: At five nodes with four slots the average load is a small integer. A bound of 1.5 times the average is then 3 or so. Ring placement of about 12 conversations is lumpy.
- Source: [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md) section 3, with Envoy and HAProxy docs. ASSUMPTION (reasoning, NOT TESTED).
- Consequence: The proxy keeps an exact table and not a hash ring.
- Feeds: PRX-AFF-001.

### LES-036 NadirClaw drops fields and sends traffic to the cloud
- Problem: NadirClaw drops `chat_template_kwargs`, `top_k` and `presence_penalty`. It forces any request with a tool to the cloud tier. It applies one API base to every upstream call, so a cloud key can reach the local engine. It has a noncommercial licence since 0.22.0.
- Source: owner notes `20260917-nadirclaw-hybrid-rouder.md` and `20260917-nadirclaw-vs-semantic-router.md`, read by [r5](evidence/research/r5-gateways-20261007T055410Z.md). PROVEN (measured by the owner).
- Consequence: The proxy forwards every field it does not patch. It routes by the `model` alias and not by prompt content. It keeps one key for each hosted node and sends it only to that node.
- Feeds: PRX-PROTO-007, PRX-SEC-008, PRX-REG-004.

### LES-037 A strict response schema rejects engine extras
- Problem: The vLLM Semantic Router rejects llama-server `timings` and MTPLX extra fields.
- Source: owner note `20260917-nadirclaw-vs-semantic-router.md`, read by [r5](evidence/research/r5-gateways-20261007T055410Z.md). PROVEN (measured by the owner).
- Consequence: The proxy passes unknown response fields. It reads the cache fields from a copy and does not rewrite the response.
- Feeds: PRX-PROTO-009, PRX-PROTO-013.

### LES-038 Engine-shaped routers do not fit other engines
- Problem: The SGLang gateway probably does not apply `cache_aware` to llama-server. Paddler works only with its own agents. The vLLM router scrapes vLLM metrics.
- Source: [r6](evidence/research/r6-routers-20261007T055357Z.md). ASSUMPTION (code reading, NOT TESTED).
- Consequence: The proxy uses one adapter for each engine for signals and cache fields. The routing core stays engine-neutral.
- Feeds: PRX-ENG-008, PRX-ENG-028, PRX-ENG-074.

### LES-039 Body keys need the body, and bodies are large
- Problem: HAProxy limits a body key to its buffer (16384 bytes by default). JSON key order is the choice of the harness. A long agent conversation can be hundreds of kilobytes.
- Source: HAProxy `doc/configuration.txt` (tune.bufsize), read by [r5](evidence/research/r5-gateways-20261007T055410Z.md). PROVEN for the HAProxy limit. The size of agent bodies is ASSUMPTION.
- Consequence: The proxy parses the body to find `messages` in any key order. It sets a documented body size limit and a fixed cost. Hash the first two messages only.
- Feeds: PRX-KEY-013, PRX-KEY-031, PRX-PROTO-065.

## D. Harness behaviour

### LES-040 pi retries for only 14 seconds and ignores Retry-After
- Problem: pi retries 3 times at 2, 4 and 8 s. The window is 14.1 s from the first failure. It ignores `Retry-After`, even with 120 s.
- Source: [s5](evidence/spikes/s5-pi-restart-README.md) rows 1, 2 and 2b. PROVEN (pi 1.0.3, fakes).
- Consequence: A refusal that must give the harness more time than 14 s cannot use a status that pi retries. The hold limit error needs a status choice (see OPEN in [14-open-questions-and-risks.md](14-open-questions-and-risks.md)).
- Feeds: PRX-ADM-028, PRX-REST-006.

### LES-041 pi decides a retry from the error text
- Problem: pi matches a regular expression on the text. It retries 429, 500, 502, 503 and 504. It does not retry 400, 401, 404, 409, 422 and 529. A 409 or 400 body that holds words like "503", "timeout" or "overloaded" causes a retry.
- Source: [s5](evidence/spikes/s5-pi-restart-README.md) status table. PROVEN for pi 1.0.3. The Anthropic path was run only for 503 and 400.
- Consequence: Error bodies of the proxy carry no retry words unless the proxy wants a retry. A fixture checks this for each error.
- Feeds: PRX-PROTO-032, PRX-REST-038.

### LES-042 A held request beats a refused request
- Problem: A refused connection and a 503 both end after 14 s. Held requests of 5, 20, 60 and 120 s all succeeded with no retry. A true hang ended at 300.6 s.
- Source: [s5](evidence/spikes/s5-pi-restart-README.md) rows 1, 2 and 4. PROVEN for pi. Other harnesses are NOT TESTED.
- Consequence: The proxy holds requests during start and during a full pool. The hold limit stays below 300 s.
- Feeds: PRX-ADM-027, PRX-ADM-043, PRX-REST-002.

### LES-043 A mid-stream drop restarts the request and drops the partial text
- Problem: pi retries a stream that ends early. The partial text is not in the retry request.
- Source: [s5](evidence/spikes/s5-pi-restart-README.md) row 3b. PROVEN.
- Consequence: The retry is a new request and costs a prefill on the same or another node. The proxy sees it as a repeat turn with the same key.
- Feeds: PRX-REST-039, PRX-REST-041, PRX-KEY-050.

### LES-044 Compaction calls carry no session header
- Problem: pi sends `x-session-affinity` only with two compat flags. On the Messages path it sends none by default. The compaction and summary calls never carry it.
- Source: [s1b](evidence/spikes/s1b-pi-lease-gaps-README.md) rows 1c, 1d and 2a'. PROVEN (pi 1.0.3).
- Consequence: The key falls back to the body hash. A compaction call has a different history, so it is a different conversation. The harness matrix lists the flags.
- Feeds: PRX-KEY-051, PRX-PROTO-036.

### LES-045 Claude Code falls back to non-streaming after a mid-stream api_error
- Problem: A mid-stream `api_error` makes Claude Code repeat the request without streaming. An `overloaded_error` makes pi and Claude Code retry.
- Source: round 1, [spike decisions](../decisions/2026-10-spike-decisions.md) section 3.2. PROVEN.
- Consequence: The error shape on a failed stream matters. The proxy passes the engine error through. It does not invent a mid-stream error.
- Feeds: PRX-PROTO-025, PRX-PROTO-026, PRX-REST-051.

### LES-046 Claude Code sends mid-conversation system messages
- Problem: Claude Code sends `system` messages in the middle of a conversation, about 11 beta headers and a prompt of about 15,000 tokens. Prefill of that prompt dominated latency (452 s for 3 turns). The effect on the model is NOT TESTED.
- Source: round 1, [spike decisions](../decisions/2026-10-spike-decisions.md) sections 3.6 and 6. PROVEN for the headers and timing.
- Consequence: Key derivation uses the first system message and the first non-system message only. Later system messages never change the key. Claude Code sends its own session header, so it rarely needs the body key.
- Feeds: PRX-KEY-013, PRX-TEST-085.

### LES-047 A change to earlier prompt text breaks the cache
- Problem: Any edit before the cached point means a miss. Compaction rewrites history and always misses. A chat template that strips reasoning from an earlier assistant turn loses the generated tokens plus 4 on llama.cpp. Hosted caches need an identical prefix.
- Source: s6 closed forms ([llama.cpp](evidence/spikes/notes-llamacpp-20261007T054753Z.md), [mlx](evidence/spikes/notes-mlx-20261007T054857Z.md)), [r4](evidence/research/r4-local-engines-20261007T022141Z.md) section 6 and the Anthropic cache docs in [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md). PROVEN for the engine formulas. No file records a harness that edited its own prompt, so the harness part is ASSUMPTION.
- Consequence: The proxy passes the prompt byte for byte. Per-node patches never change and are the same on every turn. The proxy logs cold turns for each harness so an edit becomes visible.
- Feeds: PRX-PROTO-006, PRX-PROTO-012, PRX-KEY-034, PRX-OBS-032.

### LES-048 Open WebUI fans out up to 20 sub-agents on one model
- Problem: Sub-agents use the parent model and run up to 20 in parallel. A serial node queues them. Open WebUI forwards user and chat headers when enabled. These headers hold personal data.
- Source: [docs/horizon/openwebui.md](../../docs/horizon/openwebui.md), read from documentation only. NOT TESTED.
- Consequence: The hold queue serves held requests first come, first served, so 20 children wait in order of arrival. The proxy removes personal headers before a hosted node and never logs them.
- Feeds: PRX-ADM-020, PRX-SEC-006, PRX-SEC-010, PRX-OBS-013.

## E. Fairness, windows and cache lifetime

### LES-049 A parent that waits for children can deadlock the pool
- Problem: A parent that holds a permit while it waits for children can block the children. The only reference is an application case where nested children take no slot of their own. pi-subagents refuses two direct foreground calls in one turn. Workflow children run as separate processes.
- Source: pi-extensible-workflows issue 264 (https://github.com/vekexasia/pi-extensible-workflows/issues/264), [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md), [s1b](evidence/spikes/s1b-pi-lease-gaps-README.md) row 3h. PROVEN that the case exists. Its fit to the proxy is ASSUMPTION, NOT TESTED.
- Consequence: A protected window must not block a new conversation forever. The hold limit is the guard. A parent request normally ends before its children start, so the parent holds no running slot while it waits. This is ASSUMPTION.
- Feeds: PRX-ADM-027.

### LES-050 Reserved idle seats are dead warm capacity
- Problem: A slot reserved for an idle conversation serves nobody. Too many concurrent agents thrash the prompt cache. Paused agents lose their slots by LRU eviction while others run.
- Source: CONCUR (arXiv 2601.22705) and AgentKV (arXiv 2609.14872), read as snippets in [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md) section 4. ASSUMPTION (queueing reasoning, papers not read in full).
- Consequence: The protected window is short and a parameter (PROPOSED 180 s). The proxy ends the protection of a node that does not reuse (LES-001).
- Feeds: PRX-ADM-010, PRX-ADM-011, PRX-AFF-025.

### LES-051 Hosted caches expire in 5 to 30 minutes
- Problem: OpenRouter pins for 10 minutes of inactivity. Anthropic keeps a cache for 5 minutes or 1 hour, refreshed on each hit. OpenAI keeps 30 minutes on the newest models, and traffic above 15 requests a minute for one key can overflow to other machines.
- Source: vendor docs read on 2026-10-07, in [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md) section 3. PROVEN (second-hand blog claims about changed defaults are ASSUMPTION).
- Consequence: Hosted nodes receive plain passthrough. The proxy does not own a hosted cache window. The default idle time of the local table is 600 s. The owner approved it on 2026-10-07 with these values as anchors. The label PROPOSED stays until a measurement.
- Feeds: PRX-AFF-008, PRX-REG-049.

### LES-052 Errors must not refresh an affinity entry
- Problem: OpenRouter lets only a successful request refresh the idle timer and falls back on an error.
- Source: OpenRouter prompt caching docs, [r5](evidence/research/r5-gateways-20261007T055410Z.md). PROVEN.
- Consequence: A failed request does not refresh the last-seen time of its key.
- Feeds: PRX-AFF-005, PRX-AFF-006.

## F. Restart and failure

### LES-053 A hung node holds the harness for its full timeout
- Problem: With a 3 s first-byte timeout in the router pi recovered in 3.4 s. Without it pi waited 300 s, and its retry went to the same node.
- Source: round 1, [scope reset](../decisions/2026-10-scope-reset.md) "Spike evidence". PROVEN (fakes). v1 has no timeouts in the proxy, so this cost stays.
- Consequence: The spec states what the harness sees. The one node call site is the seam for the later timeout work. The proxy records each outcome.
- Feeds: PRX-REST-046, PRX-REST-047.

### LES-054 Earlier designs persisted pins, and the new design does not
- Problem: Round 1 found that pins must persist across restarts, or conversations move nodes. The new design keeps a soft table with no journal. After a restart the table is empty.
- Source: round 1, [spike decisions](../decisions/2026-10-spike-decisions.md) section 3.2, against the decisions in [01-decisions.md](01-decisions.md). PROVEN for round 1. The cost of the new choice is NOT TESTED.
- Consequence: A conversation can land on a cold node after a restart. This is an accepted risk. The cold-turn log makes the cost visible. Tracked in [14-open-questions-and-risks.md](14-open-questions-and-risks.md).
- Feeds: PRX-AFF-002, PRX-REST-015.

### LES-055 Failure classes need engine knowledge (parked)
- Problem: Status codes alone do not tell a request fault from a node fault. A 400 `exceed_context_size_error` is a request fault. A 500 for malformed JSON is a request fault. A dropped connection can be either.
- Source: [router resilience design](../archive/router-resilience-design.md), archived. PROVEN for the three shapes on the spike versions.
- Consequence: v1 does not classify failures for retries. The recorded outcome keeps the engine and the shape, so v2 can classify. Failover only before the first byte stays a v2 rule.
- Feeds: PRX-REST-047, PRX-REST-049, PRX-OBS-002.

### LES-056 Harness cleanup is not reliable
- Problem: SIGINT to pi did not run the release hook. SIGKILL left the lease active until its timer ran out.
- Source: [s1b](evidence/spikes/s1b-pi-lease-gaps-README.md) rows 1h and 2c. PROVEN (pi 1.0.3).
- Consequence: The proxy never depends on a harness to say that a conversation ended. Table entries end by idle time. In-flight counts end on harness disconnect.
- Feeds: PRX-AFF-007, PRX-ADM-008.

### LES-057 Held requests use sockets and the backlog is finite
- Problem: A held request keeps a connection open. On macOS a bound socket that does not accept completes connect and send in 7 ms. The listen backlog is finite. Nobody tested the held count at scale.
- Source: [s5](evidence/spikes/s5-pi-restart-README.md) "Router start sequence". PROVEN for one socket. The held count at scale is NOT TESTED.
- Consequence: The proxy accepts the connection and holds the request in its own queue. The queue has a maximum size and a documented behaviour when full.
- Feeds: PRX-ADM-026, PRX-PERF-006, PRX-REST-009.

## G. Lessons from our own spikes

### LES-058 Overwritten logs lost evidence
- Problem: Spike 1 logs were overwritten and their evidence was lost. Spike 3 engine logs and one spike 4 file were overwritten before the rule arrived.
- Source: the round 2 brief ([spike-round2-BRIEF](evidence/spikes/spike-round2-BRIEF.md)), [s3](evidence/spikes/s3-slots-README.md) honesty note and [s4](evidence/spikes/s4-smoke-README.md). PROVEN.
- Consequence: Every test run and every capture writes a new timestamped file. A script refuses to truncate an existing one.
- Feeds: PRX-TEST-086.

### LES-059 Paused time breaks with real sockets
- Problem: Under paused time, 290 of 300 loopback requests hit a 1 s timeout, because the clock advanced while the real server worked.
- Source: [s2](evidence/spikes/s2-paused-time-README.md) F2. PROVEN (tokio 1.53.2).
- Consequence: Virtual-time tests use an in-memory transport and never a real socket. Real-socket tests use scaled time.
- Feeds: PRX-TEST-013.

### LES-060 The virtual clock has traps
- Problem: `tokio::time::advance` is one jump, so a lease touched every 600 s expired wrongly. Timer resolution is 1 ms and deadlines round up. `std::time::Instant` and `std::thread::sleep` are not virtual. An armed gate holds every task that reaches it.
- Source: [s2](evidence/spikes/s2-paused-time-README.md) findings 1 to 5. PROVEN.
- Consequence: The code uses tokio instants only and a driver `sleep` loop. A test does not aim to check order below one millisecond. The code needs a transport seam and a wall-clock trait.
- Feeds: PRX-TEST-011, PRX-TEST-001, PRX-TEST-003, PRX-TEST-087.

### LES-061 A small model gives a flapping verdict
- Problem: With 20 trials the threshold 0.95 passes a model of true rate 0.95 in only 54 percent of runs. The temperature changed scores by up to 8 points.
- Source: [s4](evidence/spikes/s4-smoke-README.md) "PROPOSED smoke parameters". PROVEN (binomial calculation).
- Consequence: Real-engine tests report an interval and a borderline state. They do not report a bare pass or fail.
- Feeds: PRX-REG-036.

### LES-062 Small models prove plumbing, not quality
- Problem: Qwen3 1.7B failed the chain case in 20 of 20 trials with thinking off. All round 1 and round 2 runs used one M2 with a model of at most 4B.
- Source: [s4](evidence/spikes/s4-smoke-README.md) and the [spike decisions](../decisions/2026-10-spike-decisions.md) preface. PROVEN.
- Consequence: Claims about CUDA, Vulkan and Strix Halo, and about large hybrid models, stay NOT TESTED until spike B and later work. Spike 7 also used a small model (Qwen3.5-2B), so its speeds are NOT TRANSFERABLE. Stub engines copy measured shapes only.
- Feeds: PRX-TEST-068.

### LES-063 Computed sizes disagree with reports
- Problem: The KV size of Flash-Next is 24 KiB per token by arithmetic and about 38 KiB per token in a Strix Halo report.
- Source: [r1](evidence/research/r1-models-20261007T022035Z.md). ASSUMPTION (unreconciled).
- Consequence: The proxy reads sizes from the engine logs or the measured profile. It never computes them.
- Feeds: PRX-ENG-059, PRX-ENG-075.

### LES-064 Second-hand notes are wrong often enough to matter
- Problem: Spike 6 changed the research notes on five points. The points are the llama.cpp 64-token minimum, the `-cms` default, the vLLM `all` mode, the SGLang checkpoint interval and `tokens_cached`. A small model wrote the summaries of the pages that the agents read.
- Source: [s6](evidence/spikes/s6-checkpoint-source-README.md) against [r2](evidence/research/r2-vllm-20261007T022033Z.md) and [r4](evidence/research/r4-local-engines-20261007T022141Z.md). PROVEN.
- Consequence: Engine facts in the spec cite source lines and versions. Recorded real-engine captures check the stub engines.
- Feeds: PRX-TEST-015, no proxy consequence (the rule on cited source lines binds the authors of this specification).

### LES-065 Olla's 512-byte key is constant
- Problem: Olla hashes the first 512 bytes of `messages`. In spike C the first 512 bytes were identical across all conversations of pi, Claude Code, DeepSeek Harness and opencode. The system prompt fills them. In pi on the Messages path the value changed from turn 2 because `cache_control` moves. In a short SDK conversation it changed on every turn.
- Source: [spike C](evidence/spikes/sC-key-stability/README.md), stability table. PROVEN. Olla rule: [r6](evidence/research/r6-routers-20261007T055357Z.md).
- Consequence: The proxy does not offer a first-512-bytes key. The key uses the system text and the first user text with separate limits (DEC-063).
- Feeds: PRX-KEY-045, PRX-KEY-019.

### LES-066 Codex starts with an environment block
- Problem: The first `input` item of Codex 0.160.1 is `<environment_context>`. The old rule gave one key to every Codex conversation in one directory. The real prompt is item 2. The Codex system text is 19.6 KB and its working directory sits at character 17198.
- Source: [spike C](evidence/spikes/sC-key-stability/README.md), distinctness section and vectors V14 to V16. PROVEN.
- Consequence: The key skips wrapper-only messages and joins them to the next message. The system text limit is at least 32768 bytes.
- Feeds: PRX-KEY-038, PRX-KEY-039, PRX-KEY-031, PRX-TEST-071.

### LES-067 pi and opencode already send `x-session-affinity`
- Problem: opencode 1.18.35 sends `x-session-affinity` and `x-session-id` with the same `ses_` value. pi sends `x-session-affinity` with the compat flag. The header name is therefore a de facto habit and not a pi detail.
- Source: [spike C](evidence/spikes/sC-key-stability/README.md), stability table. PROVEN.
- Consequence: Header rank 1 and rank 2 cover pi and opencode with no change in either harness. `session-id` covers Codex.
- Feeds: PRX-KEY-002, PRX-KEY-047.

### LES-068 The 600 s timeout of the Node SDKs is overridden to 300 s
- Problem: openai-node 7.30.0 and anthropic-node 0.131.0 set a timeout of 600 s. Undici gives up at 300 s, so the SDK request fails at 301 s and retries twice. Python SDKs keep 600 s.
- Source: [spike D](evidence/spikes/sD-hold-tolerance/README.md), table 1. PROVEN.
- Consequence: The hold limit plus the time until the node head stays at or below 290 s. A longer hold fails for every Node harness.
- Feeds: PRX-ADM-027, PRX-ADM-043.

### LES-069 `API_TIMEOUT_MS` cannot raise the wait of Claude Code
- Problem: Claude Code 2.1.291 gives up at 360 s with no byte. `API_TIMEOUT_MS=100000` shortened the wait to 100 s. `API_TIMEOUT_MS=1800000` did not raise it. The stream timeout settings had no visible effect. The cause is not understood.
- Source: [spike D](evidence/spikes/sD-hold-tolerance/README.md), Claude Code settings. PROVEN as observed. Cause NOT TESTED.
- Consequence: An operator cannot extend the hold for Claude Code with a setting. The hold limit must fit the lowest harness.
- Feeds: PRX-ADM-043, PRX-PROTO-053.

### LES-070 pi and opencode decide a retry from the error text
- Problem: The OpenAI error body of spike D had the type `server_error`. pi retried it on every status, also 400 and 409. So did opencode. On the Messages path pi retried 408 and 429 and did not retry 400 and 409.
- Source: [spike D](evidence/spikes/sD-hold-tolerance/README.md), table 3 and the status matrix. Same mechanism as [s5](evidence/spikes/s5-pi-restart-README.md). PROVEN.
- Consequence: A refusal that must not be retried carries none of the words `server_error`, `overloaded`, `timeout`, `rate limit`, "503" and `terminated`. This holds on 400 and 409 as well.
- Feeds: PRX-PROTO-032, PRX-PROTO-051, PRX-ADM-047.

### LES-071 A one-token step can cost the whole prompt on a hybrid model
- Problem: On llama-server with a Qwen3.5-2B hybrid, a prompt of 6000 tokens with a common prefix of 5483 recomputed 6000 tokens in 15.0 s. A prefix of 5484 recomputed 516 tokens in 1.3 s. The result was identical in three of three repetitions. The dense control showed no cliff.
- Source: [s7](evidence/spikes/s7-hybrid-cache/README.md), llama-server b11459 and brew 0.5.0. PROVEN for this model. ASSUMPTION for Flash-Next and 27B.
- Consequence: The cost of an edit is a staircase, not a ratio. The proxy treats an edit more than about 516 tokens before the end as a cold prefill. It keeps prompts append-only on hybrid nodes. The token share is a poor measure of cost.
- Feeds: PRX-ENG-034, PRX-ENG-038, PRX-AFF-032, PRX-AFF-033, PRX-OBS-026.

### LES-072 The warm capacity of llama-server is the slots plus the host cache
- Problem: With `-np 1`, two interleaved conversations both stayed warm (201 tokens recomputed a turn) because `--cache-ram` saved the idle slot state in host memory. With `-np 1 --cache-ram 0` every turn was cold (about 23 s at 6k). A seat for each slot over-restricts such a node.
- Source: [s7](evidence/spikes/s7-hybrid-cache/README.md). PROVEN for two conversations and one model. Eviction beyond two conversations and past 8 GiB is NOT TESTED.
- Consequence: The registry and the probe set `warm_capacity`. The default is the slot count. A larger value needs a declaration or a probe result.
- Feeds: PRX-ADM-009, PRX-ADM-049, PRX-ADM-050, PRX-ENG-036, PRX-ENG-058, PRX-REG-042.

### LES-073 History rewrites make the cache fail on hybrid models
- Problem: An edit at 194 tokens in a 6075-token conversation history recomputed 5942 tokens (about 22 s). An edit at 342 tokens in a 12281-token history recomputed 12148 tokens (about 47 s). A pure append recomputed 201 tokens.
- Source: [s7](evidence/spikes/s7-hybrid-cache/README.md). PROVEN for this model.
- Consequence: Compaction, trimmed tool results and a changed system prompt cost a full prefill on a hybrid node. The proxy cannot prevent a rewrite by the harness. It must not cause one, and it must show the cold turn.
- Feeds: PRX-ENG-035, PRX-ENG-038, PRX-OBS-026.

### LES-074 Ollama silently ignores `previous_response_id`
- Problem: Ollama 0.40.0-rc6 accepts `previous_response_id` and does not use it. The harness loses the context with no error. llama-server answers 400 instead.
- Source: [r7](evidence/research/r7-responses-api-20261007T081115Z.md) (source read, `responses.go:1336`). PROVEN as source reading. NOT TESTED live.
- Consequence: The registry flag `ignores_previous_response_id` makes the proxy refuse such a request. A stateful engine needs `response.id` routing.
- Feeds: PRX-PROTO-061, PRX-PROTO-062, PRX-ENG-064.

### LES-075 Engines that translate Responses show translation defects
- Problem: llama-server and Ollama convert Responses to chat completions inside the engine. Their open Codex issues are all translation defects. A `reasoning` item with `summary: null` gives 400 (llama.cpp issue 29159). Ollama drops non-function tools and `developer` items (Ollama issue 18305). Custom tool calls fail.
- Source: [r7](evidence/research/r7-responses-api-20261007T081115Z.md). PROVEN as issue and source reads. NOT TESTED live.
- Consequence: The proxy does not translate (DEC-065). It sends Codex only to nodes with the `responses` flag and reports the known gaps. A real-session test comes before a support claim.
- Feeds: PRX-PROTO-054, PRX-ENG-062, PRX-ENG-065, PRX-ENG-066.

## Sources

- Evidence files: [spike 7](evidence/spikes/s7-hybrid-cache/README.md), [r7](evidence/research/r7-responses-api-20261007T081115Z.md), [spike C](evidence/spikes/sC-key-stability/README.md), [spike D](evidence/spikes/sD-hold-tolerance/README.md), [r1](evidence/research/r1-models-20261007T022035Z.md), [r2](evidence/research/r2-vllm-20261007T022033Z.md), [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md), [r4](evidence/research/r4-local-engines-20261007T022141Z.md), [r5](evidence/research/r5-gateways-20261007T055410Z.md), [r6](evidence/research/r6-routers-20261007T055357Z.md), [s1b](evidence/spikes/s1b-pi-lease-gaps-README.md), [s2](evidence/spikes/s2-paused-time-README.md), [s3](evidence/spikes/s3-slots-README.md), [s4](evidence/spikes/s4-smoke-README.md), [s5](evidence/spikes/s5-pi-restart-README.md), [s6](evidence/spikes/s6-checkpoint-source-README.md)
- Records: [spike decisions](../decisions/2026-10-spike-decisions.md), [scope reset](../decisions/2026-10-scope-reset.md), [router resilience design](../archive/router-resilience-design.md)
- Issue and pull request pages named in each lesson, read by the research agents on 2026-10-07.
