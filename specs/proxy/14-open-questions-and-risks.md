# Open questions and risks

Status: draft for review, 2026-10-07. Part of the Legatus proxy baseline. This file lists what the spec does not know and what can go wrong. Each item names the measurement or decision that settles it.

## Reading guide

An OPEN item is a question that has no answer yet. A RISK item is a way the proxy or its tests can fail. Each item has a stable ID. Other files cite the IDs.

The owner approved or decided many items on 2026-10-07. A row that the owner closed keeps its ID and says CLOSED with the reason. "Spike D" is the harness test of the hold-limit status, the hold tolerance and the retries. It ran on 2026-10-07 on fake servers. "Spike C" is the key stability test of the same date.

Labels: PROVEN (measured or read from source, with a reference), PROPOSED (a design choice), ASSUMPTION (believed, not checked), NOT TESTED. Spike 7 (S7) measured hybrid cache behaviour on this machine on 2026-10-07 with llama-server b11459 and brew 0.5.0 on Qwen3.5-2B Q4_K_M and a dense control. Its numbers are PROVEN for that model and NOT TRANSFERABLE in speed to larger models.

The evidence labels R1 to R7, S1B to S7, RND1 and OWUI appear in [the label table of the edge cases](12-edge-cases.md#evidence-labels).

Edge cases that depend on an item cite the ID in [edge cases](12-edge-cases.md). Scenarios that settle an item are in [test fixtures and scenarios](13-test-fixtures-and-scenarios.md).

## Open questions

| ID | Question | What is known | Settled by | Owner decision |
|---|---|---|---|---|
| OPEN-001 | What is the default protected window? | PROPOSED 180 s. Hosted caches use 5 to 30 min idle windows (R3). No source gives a window for self-hosted engines | A replay of recorded agent traces with several windows. Measure the warm share and the idle slot time. Spike 7 measured warm swaps and no idle time limit | Choose the default and the range |
| OPEN-002 | What is the default hold limit? | PROPOSED 250 s. A hold of 250 s completed at the first attempt in every harness tested (spike D). The sum of the hold limit and the time until the node head must stay at or below 290 s | The time to head of real engines (OPEN-032) | Choose the default and whether the registry sets it for each route |
| OPEN-003 | CLOSED 2026-10-07: the status at the hold limit is 503 with no `Retry-After` or 30 s or less (DEC-064). | PROVEN (spike D): every harness retries 503, 502, 504, 500 and 529. Codex fails at once on 429. Claude Code fails at once on `Retry-After` of 90 s or more | FIX-133, FIX-307, FIX-313 | The owner approves DEC-064 |
| OPEN-004 | Which paths does the passthrough of other OpenAI-style paths cover, and does the proxy answer `/v1/models`? The owner approved the passthrough (DEC-004). | Embeddings, audio and images exist in Open WebUI. Which paths and bodies it sends is not checked (OWUI). The proxy picks a node by the `model` field | Capture FIX-048 and read the paths. Test FIX-153 and FIX-154 | Approve the path list. Decide the model list answer. The passthrough itself is decided |
| OPEN-005 | CLOSED 2026-10-07: v1 supports the OpenAI Responses API. The owner reversed DEC-057 and decided that Codex is supported (DEC-062). | Codex 0.160.1 ran against a fake server (spikes C and D). Engine support is in [05](05-engine-behaviour.md) section 14 from r7 (OPEN-031). DEC-065 (pass-through only) is pending the owner | FIX-318, FIX-319 | Decided |
| OPEN-006 | Which harnesses tolerate a held request, and for how long? | PROVEN on fakes (spike D): a hold of 250 s completes in pi, Claude Code, opencode, DeepSeek Harness, Codex and four SDKs. Give-up times: 299 s to never. Open WebUI is NOT TESTED (OPEN-033) | FIX-306 and FIX-316 | None for the tested harnesses |
| OPEN-007 | Is the key stable across turns for each harness? | PROVEN (spike C) for pi, Claude Code, opencode, DeepSeek Harness, Codex and the SDKs. The key breaks at compaction and collides for same-prompt conversations with no header. Open WebUI is NOT TESTED | FIX-305, FIX-108, and an Open WebUI capture (OPEN-033) | List the key map for each harness |
| OPEN-008 | Does affinity pay on each engine and model? | PROVEN for llama-server b11459 and 0.5.0 on Qwen3.5-2B (S7): an append-only turn recomputes 201 tokens for 200 new. An edit more than 516 tokens before the end recomputes the whole prompt. So affinity pays on append-only traffic and gives a cliff on rewritten history. The dense control degrades linearly. NOT TESTED: vLLM, SGLang, gufo, Flash-Next, 27B. vLLM issue 45238 gives a silent zero hit (R2) | The benchmark FIX-292 to FIX-294 on real engines, and spike B on the owner machines | Accept per-engine affinity settings from the result |
| OPEN-009 | How many warm conversations fit on one node for each engine? | llama.cpp: 32 checkpoints for each slot, host cache 8 GiB, checkpoints of 63 to 214 MiB (S6, R4). gufo: 128 entries (S6). mlx-lm: 10 entries (S6). vLLM and SGLang depend on memory. Measured (S7, llama-server, 2B model): two conversations stay warm on one slot with `--cache-ram` 8192. Each 6k conversation takes about 129 MiB. With `--cache-ram` 0 every turn is cold. Eviction beyond two conversations is NOT TESTED | A warm capacity probe: open conversations one by one on a real node and measure the turn at which reuse falls (OPEN-043) | Set the window and the cap rules from the result |
| OPEN-010 | Do MTP and prefix caching work together on hybrid models? | Corruption issues exist in vLLM (R2). SGLang disables the radix cache in its spec-decode test (R3). A measured MTP loss on 8 conversations was 0.81 to 0.87 times (R1). NOT TESTED here | A paired run: same turns with MTP on and off, compare cache fields and output text | Decide whether the registry may declare MTP with caching |
| OPEN-011 | How does Anthropic prompt caching (`cache_control` markers) interact with the proxy? | The proxy passes bytes unchanged. Anthropic caches by prefix with a 5 min or 1 h life (R3). Local engines ignore the markers. A hosted node has its own cache, so affinity can matter or not | Test FIX-105 and FIX-222. Measure hosted cache reads with and without a stable key | Decide whether keys ignore the markers and whether hosted routes use affinity |
| OPEN-012 | CLOSED 2026-10-07 for the default: the table expiry is 600 s of idle time (DEC-049). The label PROPOSED stays. | OpenRouter uses 10 min of inactivity (R3). Olla uses 600 s (R6). The window is separate and is PROPOSED at 180 s | Trace replay with several expiries | Decided. A measurement can change the value |
| OPEN-013 | Does `x-session-affinity` go to hosted nodes? | The brief strips credentials and `x-legatus-*` only. Hosted APIs have their own cache keys, for example `prompt_cache_key` (R3) | Test FIX-220. Read the API docs for each hosted route | Decide the header rule for hosted routes |
| OPEN-014 | What bounds the hold queue? | PROPOSED: `max_held`, default 64 (file 04). S5 did not test the kernel backlog at scale | FIX-132 and FIX-202 | Choose the bound and the overflow status |
| OPEN-015 | Where does the admin API listen? | The brief says versioned, token required, dashboard in another process. The port and bind address are not fixed | Review of [observability and admin](08-observability-and-admin.md) | Decide port, bind address and token source |
| OPEN-016 | What happens when the log writer is slower than traffic? | The brief says keep all events and pause on low disk. A stall rule is not defined | FIX-038 and FIX-245 | Choose: bounded memory and counted drops, or a request wait |
| OPEN-017 | Does the key include the credential label? | Three users can send the same short first message. OpenRouter keys by account (R3) | FIX-107 | Decide the scope of the key |
| OPEN-018 | What does "ready" mean at start? | Pi tolerates a hold (S5). Calibration at start can take long. PROPOSED: listening, registry loaded, no health wait | FIX-200 | Decide the ready rule |
| OPEN-019 | What is the poor-reuse threshold and the measurement window? | The brief says a node with poor reuse gets least-load routing. The brief sets no value. Hybrid reuse on llama-server is a cliff and a staircase, not a sawtooth (S7). The token share is a poor guide to cost (see [03](03-affinity-and-keys.md) section 9.6) | FIX-115 and FIX-183 and a trace replay | Choose the threshold and the hysteresis |
| OPEN-020 | Does v1 terminate TLS on the model port? Harness credentials: CLOSED 2026-10-07. The proxy requires a client token whenever a hosted node exists (DEC-059). | Three users on a LAN is the target. Local-only setups stay open | Review of [registry and configuration](07-registry-and-configuration.md). FIX-301 to FIX-303 | Decide TLS |
| OPEN-021 | How accurate must the token estimate be for the Ollama guard? | Ollama truncates above about half its context (S3). The proxy has no tokenizer by default. NOT TESTED | FIX-194 on captures, with an error bound for each model family | Choose the margin and the false refusal rate |
| OPEN-022 | Do the cache fields mean the same on every engine? | `tokens_cached` on llama.cpp is not the reused count (S6). vLLM needs a flag (S6). Hosted fields differ | FIX-188 and the engine captures | List the field for each engine in [engine behaviour](05-engine-behaviour.md) |
| OPEN-023 | CLOSED 2026-10-07: the hold-limit error carries no `Retry-After` or a value of 30 s or less. | PROVEN (spike D): the Python SDKs, anthropic-node and Codex honour it. Claude Code fails at once at 90 s or more. pi, DeepSeek Harness and opencode ignore it | FIX-313 | None |
| OPEN-024 | Can a long-held request with a table entry move to a free node? | The brief says strict stickiness. SGLang gateway has a spill gate (R3). A move costs a full prefill, 47 to 230 s at 60k tokens (R4) | FIX-126 with the cost model, then real engines | Keep strict, or add a rule after a set wait |
| OPEN-025 | Does a second concurrent request of one conversation take a second slot? | llama-server picks a slot by prefix similarity (S6). The second request can split the cache | FIX-128 on a real node | Decide the count rule |
| OPEN-026 | How does the proxy label a harness in the log? | The brief asks for a harness label. The source is not defined. User-agent is one choice | Captures FIX-046 to FIX-051 | Decide the source |
| OPEN-027 | Which versions and pass numbers does the benchmark use? | PRX-TEST-063 needs frozen versions. The owner rule: baselines first. Beat every baseline on cold-turn rate and queue wait. Stay within 2 times of the best on added latency and memory. The numbers follow the benchmark (DEC-051) | Owner input and a dry run | Approve the baseline versions. Set the pass numbers after the baselines ran |
| OPEN-028 | CLOSED 2026-10-07: a hosted node has no running cap in v1. The limits of the provider apply (DEC-050). The protected window does not apply to a hosted node. | The brief treats hosted nodes as plain passthrough. A rate limit can still bind | FIX-224 is REMOVED | Decided |
| OPEN-029 | Where do captures live and who reviews the redaction? | PRX-TEST-056 forbids unredacted captures. The spec defines no store | Owner input | Decide the store and the reviewer |
| OPEN-030 | Is the hold limit set for each route? | Give-up times differ from 299 s to never (spike D). One value of 250 s fits all tested harnesses. A higher value for Claude Code, Codex or Python SDK routes is NOT TESTED | FIX-316 | Decide |
| OPEN-031 | Which local engines serve the Responses API, and with which fields for the key and the cache? | Read from source in r7 (D, I and U labels): see [05](05-engine-behaviour.md) section 14. llama-server and Ollama each have a Codex gap that is open. mlx_lm.server has no `/v1/responses`. No engine ran with a live Codex conversation | Spike F (OPEN-040) | Accept the engine list |
| OPEN-032 | What is the time until the response head of each real engine under load? | llama-server sends headers early, Ollama late and mlx_lm at once (S3). The sum with the hold limit must stay at or below 290 s (spike D). Real values are NOT TESTED | A run of each engine with a cold prefill and a full queue | Set the hold limit for each alias |
| OPEN-033 | How does Open WebUI behave on key, hold, retry and the refusal message? | Docs only (OWUI). NOT TESTED | An Open WebUI run with forward headers and a hold of 250 s | None |
| OPEN-034 | How do Claude Code interactive subagents behave, with many children and long runs? | A child keeps the session id and has its own agent id (spike C). Interactive subagents are NOT TESTED | A Claude Code interactive capture | None |
| OPEN-035 | Does a hold above 600 s work in any harness? | Stage 2 of spike D went to 600 s. Tests at 700 s ran in stage 1 and are void for the late part | A rerun of the 700 s cases with the fixed fake | None |
| OPEN-036 | What happens with many simultaneous holds, for example 64 requests at 250 s? | Spike D did not test it. NOT TESTED | A load test with the held-queue bound | None |
| OPEN-037 | The stage 1 results of spike D past 700 s are void. Which cases need a repeat? | The fake server crashed at about 700 s on a non-stream early head. Give-up times below 700 s are valid. Stage 2 repeated every case | A rerun of the stage 1 cases above 700 s, if needed | None |
| OPEN-038 | How do Codex compaction, subagents, retries and long loops affect the key? | Spike C ran plain, two-session and resume cases only. NOT TESTED | A Codex capture (FIX-328) | None |
| OPEN-039 | Why do the stream timeout settings of Claude Code have no effect? | `API_TIMEOUT_MS` shortens the wait and cannot raise it. The cause is not understood. NOT TESTED further | A source read or a trace of Claude Code | None |
| OPEN-040 | Does real Codex 0.160.1 work against each engine with tool loops and compaction? | R7 read the source only. llama-server fails on a `reasoning` item with `summary: null` (issue 29159) and on `custom_tool_call`. Ollama drops `developer` items (issue 18305). vLLM has Codex issues that are open. gufo documents a Codex subset. Spikes C and D used a fake server | Spike F: real Codex conversations on llama-server, Ollama, vLLM and gufo, with apply_patch, reasoning items and compaction (FIX-331) | Owner decision pending: run spike F before any Codex support claim |
| OPEN-041 | After `previous_response_id` on an engine that ignores it: refuse (400) or warn and pass? After a stream failure on the Responses path: abrupt close or `response.failed` with a code? | R7 recommends refuse or warn, and `response.failed` with `error.code`. The spec proposes refuse and abrupt close (PRX-PROTO-062, PRX-PROTO-047). Codex retries a dropped stream 5 times (spike D) | FIX-318, FIX-319 and spike F | Choose both |
| OPEN-042 | Does a Codex subagent send its own `session-id` or only its own `thread-id`? | The default key order is `session-id`, `thread-id`, `prompt_cache_key`. If the subagent shares the `session-id`, parent and children share a key. NOT TESTED | A Codex subagent capture (FIX-328) | Choose the order if the result needs it |
| OPEN-043 | How many conversations stay warm on one llama-server node when the host cache fills? | S7 tested two conversations. Eviction beyond 32 checkpoints and past 8 GiB is NOT TESTED | A warm capacity probe with many conversations, and spike B | Set the default of `warm_capacity` rules |
| OPEN-044 | Does a streamed llama-server response carry `timings` and `cached_tokens` for the cold-turn signal? | S7 verified the non-stream shape only. Source reading says the terminal chunk carries `timings` ([05](05-engine-behaviour.md) section 4) | A streamed capture (FIX-332) | None |
| OPEN-045 | Does the S7 rule hold for Flash-Next, 27B and other builds? | S7 used Qwen3.5-2B and two builds (b11146, b11459). Rules transfer as ASSUMPTION. Speeds and sizes are NOT TRANSFERABLE. Models with the Qwen3-Next architecture are NOT TESTED | Spike B on the Strix Halo and the M5 Max (DEC-060) | Run the script |

## Risks

| ID | Risk | Evidence | Settled by or mitigated by |
|---|---|---|---|
| RISK-001 | Affinity gives no gain on some engine or model, so the main idea is wrong there | Spike 7 measured a gain on append-only traffic and a cold prefill on rewritten history (llama-server, 2B model). vLLM align mode hits only at block boundaries (S6) | FIX-292 to FIX-294. Per-node least-load mode is the mitigation |
| RISK-002 | vLLM returns zero cache hits with status 200 and no metric | PROVEN: issue 45238 (R2) | FIX-180. The proxy measures reuse from the reply |
| RISK-003 | llama.cpp re-reads the whole prompt on hybrid models, and one open case can crash on a shared prefix | PROVEN as reports: issues 19794, 20225, 22384 and 28425 (R4) | Real-engine capture FIX-041. Pin the engine build |
| RISK-004 | The 8 GiB host cache of llama-server is smaller than 32 checkpoints for 4 slots | S7 measured 129 MiB for a 6k conversation on a 2B hybrid (PROVEN). Sizes for Flash-Next and 27B are second-hand or computed (R4, R1) and NOT TRANSFERABLE. `--cache-ram` default from source (S6) | FIX-182 and the warm capacity probe of OPEN-009 |
| RISK-005 | A hold of 249 s plus 60 s of prefill passes the harness timeout, and the harness cancels a request that the proxy served late | PROVEN: pi cuts at 299 s to 300.6 s (S5, spike D). The sum is arithmetic | FIX-271. Set the hold limit below the timeout by the longest expected service time |
| RISK-006 | A harness other than pi fails on a held request | PROVEN not to fail at 250 s in six harnesses and four SDKs (spike D). Open WebUI is NOT TESTED | OPEN-033 |
| RISK-007 | A retrying harness multiplies the wait: 4 requests of 250 s each is 1000 s | PROVEN: attempts are 3, 4, 7, 9, 30 and 11 (spike D). The product is arithmetic. Codex gives 7500 s | FIX-136 and FIX-316 |
| RISK-008 | Strict stickiness leaves nodes idle while requests with a table entry wait | PROPOSED rule. Spill designs exist (R3, R6) | FIX-126 and OPEN-024 |
| RISK-009 | Keys change between turns because a harness edits the system prompt or the first message, and affinity fails | PROVEN not to happen in six harnesses (spike C). Open WebUI is NOT TESTED | OPEN-007 and the key churn metric |
| RISK-010 | Many conversations start with the same message and share one key, so one node gets all the load | PROVEN: Olla has the same collision (R6) | FIX-101. The cap still holds. Add the credential label if it matters (OPEN-017) |
| RISK-011 | A side request evicts the main cache on a one-slot hybrid node | PROVEN as a claim in the owner notes (R6) | FIX-122 and FIX-103 |
| RISK-012 | The Ollama guard refuses good prompts or lets truncated prompts pass | NOT TESTED | OPEN-021 and FIX-194 |
| RISK-013 | The cap defaults are wrong. The prefill-bound gains of 1.1 to 1.5 times were inside the 2 times noise of spike s3 | PROVEN: S3 notes | Re-measure the knee with a decode-bound probe and report both speeds |
| RISK-014 | The window default wastes warm capacity or loses warm caches | PROPOSED value | OPEN-001 |
| RISK-015 | After a restart the engines run old requests while the proxy counts zero, and the table is gone | PROVEN for the table loss (BRIEF). The count gap is a design gap | FIX-209 and FIX-203 |
| RISK-016 | A stub differs from its engine and the tests pass for the wrong reason | ASSUMPTION | Engine captures FIX-040 to FIX-045. Compare each release |
| RISK-017 | Virtual mode cannot find socket bugs | PROVEN: real sockets break paused time (S2 F2) | Scaled and real scenarios marked S and R |
| RISK-018 | Scaled tests race, so a green run proves little about order | PROVEN: 25 to 15 split (S2 F8) | Use bands. Do not claim exact order in scaled mode |
| RISK-019 | A baseline has a poor configuration and the comparison is unfair | ASSUMPTION | PRX-TEST-060 to PRX-TEST-062 and a review of each baseline by someone who knows it |
| RISK-020 | Engine releases change behaviour. Ollama 0.40.0 runs MLX by default on Apple Silicon, and the frozen copy is 0.35.1 | PROVEN: R4 and spike notes | Pin versions. Re-run captures on each release (FIX-191) |
| RISK-021 | The admin token or the dashboard leaks a session value | PROPOSED rules | FIX-240, FIX-241 and FIX-247 |
| RISK-022 | The log guard pauses logging and leaves gaps that hide a fault | PROPOSED | FIX-245 and the marker records |
| RISK-023 | MTP with prefix caching corrupts output | PROVEN as issues: vLLM 47861 and 53912 (R2). Here NOT TESTED | OPEN-010 |
| RISK-024 | Three users share one hosted key, so one user can use the whole rate limit | PROPOSED: key custody in the proxy. Budget tracking is out of v1 | FIX-223 and a per-user limit in a later version |
| RISK-025 | A compaction storm gives many full prefills at once and many hold-limit errors | PROVEN for cost: 230 s at 60k tokens on llama.cpp (R4) | FIX-300 and the hold limit choice |
| RISK-026 | A harness uses an API path that v1 does not serve | NOT TESTED | OPEN-004 and OPEN-005 |
| RISK-027 | The proxy behaves differently on a multi-thread runtime than on the one-thread paused runtime | NOT TESTED: S2 | Scaled mode runs multi-thread. Compare results |
| RISK-028 | The measured profile of a node is old or taken on a busy node | NOT TESTED | Calibration on change. Show the age in the admin read |
| RISK-029 | An existing router, such as Olla, with a small patch already meets the vision, so the build is not worth it | R6 recommends a two-day spike of Olla against real harness traffic | The baseline run FIX-292 to FIX-295 and the Olla spike |
| RISK-030 | All timing evidence is from one M2 with 16 GB. Linux, CUDA and Strix Halo are not measured | PROVEN: S3 notes | Real-engine captures on each target machine |
| RISK-031 | pi 1.0.4 or a later release changes the header flags or the retry rules | PROVEN: 1.0.4 exists, not tested (S5) | Pin 1.0.3. Re-run FIX-260 on each release |
| RISK-032 | Redacted captures lose detail that matters, such as text-dependent behaviour | ASSUMPTION | Keep a small hand-reviewed set of real captures, held privately |
| RISK-033 | A large number of held connections exceeds the kernel backlog or memory | NOT TESTED: S5 | FIX-132 and FIX-202. `max_held` bounds the queue (PROPOSED 64) |
| RISK-034 | Two conversations with the same first prompt and no header share one key (DeepSeek Harness, SDK clients) | PROVEN as a known limit (spike C). A session header separates them | FIX-310 and the key churn metric |
| RISK-035 | A newer Claude Code version changes the billing block, for example with a `cch=` field, and breaks the drop rule | Spike C saw no `cch=` in 2.1.291. A newer version is NOT TESTED | FIX-309 on each Claude Code version |
| RISK-036 | The derived key changes at compaction and the conversation looks new | PROVEN by design (spike C). The old cache is useless anyway. A header key survives | FIX-317 |
| RISK-037 | The Responses fields `previous_response_id` and `store` tie a conversation to the node that holds the state | Codex 0.160.1 sends `store: false` and no `previous_response_id` on a local provider (R7, source and fake). Stateful engines need `response.id` routing (PRX-PROTO-061). Ollama ignores the id with no error (R7). NOT TESTED with a live engine | FIX-318, FIX-330 and spike F |
| RISK-038 | The sum of the hold limit and the time until the node head passes 299 s for an engine that sends no early head | PROVEN limit (spike D). Ollama sends the head late (S3) | FIX-323 and OPEN-032 |
| RISK-039 | A harness rewrites history (compaction, trimmed tool results, a changed system prompt) and a hybrid node pays a cold prefill each time | PROVEN for Qwen3.5-2B on llama-server: 6000 tokens in 15 s at 6k and 12000 in 34 to 38 s at 12k (S7). ASSUMPTION for Flash-Next and 27B | FIX-333. The proxy keeps prompts append-only and reports cold turns |
| RISK-040 | A llama-server node runs with `--cache-ram 0` and every turn of a second conversation is cold | PROVEN: about 23 s a turn at 6k (S7) | The probe step 12 and the finding of PRX-ENG-061 |
| RISK-041 | A Codex request reaches an engine with an open Responses gap and fails on every request | R7: llama-server issue 29159, Ollama issue 18305, vLLM issue 45273. NOT TESTED live | Spike F and the `responses` flag default of false |
| RISK-042 | A stateful engine loses a response id after a proxy restart or a move, and the follow-up fails | R7, inferred. Codex does not send the id on a custom provider | PRX-PROTO-061 and FIX-330 |

## Measurements to run, in order

The order follows the number of items each measurement settles.

| Step | Measurement | Settles |
|---|---|---|
| 1 | Spike 7: DONE 2026-10-07 for llama-server on Qwen3.5-2B. Not run for other engines | OPEN-008, OPEN-009, OPEN-019, RISK-001, RISK-004 are partly settled. OPEN-043 to OPEN-045 remain |
| 2 | Spike D (done on fakes, 2026-10-07): hold matrix, status matrix and abrupt close. Open: real engine time to head | OPEN-002, OPEN-003, OPEN-006, OPEN-023, OPEN-030, OPEN-032, RISK-005 to RISK-007 |
| 3 | Harness captures with key analysis. Spike C ran for six harnesses. Open: Open WebUI, Codex cases, Claude Code subagents | OPEN-007, OPEN-026, OPEN-033, OPEN-034, OPEN-038, RISK-009 |
| 4 | Engine captures and the stub oracle test | RISK-016, OPEN-022 |
| 5 | Two-day Olla spike on real traffic | RISK-029 |
| 6 | Baseline benchmark on stubs and then on real engines | The claims in [test fixtures](13-test-fixtures-and-scenarios.md) |
| 7 | Replay of recorded traces with several windows and expiries | OPEN-001, value of DEC-049 |
| 8 | Spike B: a script that the owner runs on the Strix Halo and on the M5 Max. Spike 7 reported. The owner or the coordinator writes the script next. The owner runs it | OPEN-043, OPEN-045. Its questions: checkpoint positions, warm swaps, state size and eviction on the large models |
| 9 | Spike F: real Codex 0.160.1 conversations against llama-server, Ollama, vLLM and gufo. Owner decision pending | OPEN-040, OPEN-041, OPEN-042, RISK-041 |

## Owner decisions pending

The table lists what the owner must decide. The column "Blocks" names the file that cannot reach its final text until the decision.

| Decision | Options | Proposed | Blocks |
|---|---|---|---|
| Protected window default | 60 s, 180 s, 600 s | 180 s | [admission](04-admission-control-and-queueing.md) |
| Hold limit default | 120 s, 250 s, per route | 250 s | admission, [restart](09-restart-and-failure.md) |
| Status at the hold limit | 503, 504, 529. The proxy refuses 429 | 503 with no `Retry-After` or 30 s or less (DEC-064, PROVEN in spike D). The owner approves | [protocols](06-protocols-and-harnesses.md), admission |
| Other OpenAI paths | Decided 2026-10-07: pass through by path | Pass through by path | protocols |
| Responses API | Decided 2026-10-07: in v1 for Codex (DEC-062) | Pass-through only, no translation, per-node `responses` flag (DEC-065, owner decision pending). Engine support from r7 | protocols |
| Warm capacity rule | Seats equal slots, or `warm_capacity` | `warm_capacity` with default slots (DEC-066, owner decision pending) | [admission](04-admission-control-and-queueing.md) |
| Defaults pending approval | DEC-063 key rule, DEC-064 hold-limit status with `Retry-After` 30 s, DEC-067 two successful requests | Applied as written, pending owner approval | [affinity and keys](03-affinity-and-keys.md), admission |
| Spike F | Run, or defer | Run before any Codex support claim (owner decision pending) | protocols, [engines](05-engine-behaviour.md) |
| Credential label in the key | Yes or no | No | [affinity and keys](03-affinity-and-keys.md) |
| Affinity header to hosted nodes | Forward or strip | Strip | [registry](07-registry-and-configuration.md) |
| Hold queue bound | A number | 64 (PROPOSED) | admission |
| Log overflow rule | Count drops, or wait | Count drops | [observability](08-observability-and-admin.md) |
| Benchmark pass numbers and baseline versions | Numbers | Rule decided 2026-10-07. Numbers follow the benchmark | [test fixtures](13-test-fixtures-and-scenarios.md) |
| Capture store and reviewer | A path and a person | None chosen | test fixtures |

## Parked for version 2

The owner dropped these rules from v1 on 2026-10-07 (DEC-061). The trigger for each is "users complain of unfair waits".

| Parked item | What it did | Where it was |
|---|---|---|
| Memory of the arrival time of a refused request | Kept the arrival time for 30 s, so that a retry kept its place in the queue | Old PRX-ADM-031 and the parameter `hold_memory` |
| Per-user ordering | Served the user with the fewest held seats first | Old PRX-ADM-032 and PRX-ADM-034 |
| Priority after half of the hold limit | Served a request that waited more than half of the hold limit first | Old PRX-ADM-033 |

v1 serves held requests first come, first served. The probation window (30 s) and the key classes STRONG, DERIVED and WEAK with sibling spill stay in v1.

## Sources

- [Spike C: key stability](evidence/spikes/sC-key-stability/README.md)
- [Spike D: hold tolerance](evidence/spikes/sD-hold-tolerance/README.md)
- [Spike s5: pi retry and restart](evidence/spikes/s5-pi-restart-README.md)
- [Spike s3: slots and concurrency](evidence/spikes/s3-slots-README.md)
- [Spike s2: paused time](evidence/spikes/s2-paused-time-README.md)
- [Spike s1b: header mechanism](evidence/spikes/s1b-pi-lease-gaps-README.md)
- [Spike s6: checkpoint policy](evidence/spikes/s6-checkpoint-source-README.md)
- [Models research](evidence/research/r1-models-20261007T022035Z.md), [vLLM research](evidence/research/r2-vllm-20261007T022033Z.md), [SGLang and routing research](evidence/research/r3-sglang-routing-20261007T022127Z.md), [local engines research](evidence/research/r4-local-engines-20261007T022141Z.md), [routers research](evidence/research/r6-routers-20261007T055357Z.md)
- [Spike round 1 decisions](../decisions/2026-10-spike-decisions.md)
- [Open WebUI notes](../../docs/horizon/openwebui.md)
