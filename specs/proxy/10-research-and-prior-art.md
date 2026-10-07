# Research and prior art

Status: draft for the proxy baseline, written 2026-10-07. Owner: writer W-F. This file summarises the research notes and spikes, then decides whether to build or adopt.

The file has four parts. Part 1 explains the labels. Part 2 covers the six research notes. Part 3 covers the spikes. Part 4 is the build-versus-adopt analysis. The lessons that come out of this evidence are in [11-lessons-learned.md](11-lessons-learned.md).

## 1. How to read this file

The research agents wrote their notes on 2026-10-07 with web search and web fetch. Each note uses its own labels: documented, inferred and unverified. A small model summarised the fetched pages, so exact numbers need a re-check before a requirement relies on them.

This file maps the labels in this way.

| Source label | Label here | Meaning |
| --- | --- | --- |
| Documented, source read, or measured in a spike | PROVEN | A reference to the evidence file or URL follows the fact. |
| Inferred by an agent | ASSUMPTION | The agent computed or reasoned the fact. Nobody checked it. |
| Unverified, snippet only, blog only | ASSUMPTION | The source is weak. The text says so. |
| Not run | NOT TESTED | Nobody ran the thing. |
| A design choice | PROPOSED | The proxy spec chooses it. |

Research note r7 (how local engines support the Responses API and what Codex needs) is complete. Its label D (documented) maps to PROVEN. Its labels I (inferred) and U (unverified) map to ASSUMPTION. No engine ran with a live Codex conversation.

Spike 7 (hybrid cache measurement on this machine) is complete. It used llama-server b11459 and brew 0.5.0 on Qwen3.5-2B Q4_K_M and a dense control. Its token counts are PROVEN for that model. Its speeds are NOT TRANSFERABLE to other models and machines.

## 2. Research notes

### R1. Qwen 3.8 Flash-Next and Qwen 3.8 27B architecture

Evidence: [r1-models](evidence/research/r1-models-20261007T022035Z.md).

Question: how big is the per-conversation cache state of the two target models, and why is reuse of a prefix not like a plain transformer?

Method: the agent read the Hugging Face cards and the raw `config.json` files, the SGLang blog post and the owner's repo. The agent computed the byte sizes from the config values.

Key facts:
- Both models are Gated DeltaNet hybrids with three linear layers to one full-attention layer. PROVEN (r1, `config.json`).
- Flash-Next has 48 layers: 36 Gated DeltaNet and 12 sparse-attention layers. It is a mixture of experts with 125B total and 6B active parameters. PROVEN (r1).
- The 27B model is dense with 64 layers: 48 Gated DeltaNet and 16 full-attention layers. PROVEN (r1).
- The KV cache grows at 24 KiB per token on Flash-Next and 64 KiB per token on the 27B model at f16. ASSUMPTION (r1 arithmetic).
- The recurrent state is about 110 to 115 MiB on Flash-Next and about 150 MiB on the 27B model. It does not grow with the context. ASSUMPTION (r1 arithmetic).
- A Strix Halo report in the owner's profile file implies about 38 KiB per token for Flash-Next. The note does not reconcile this with 24 KiB. ASSUMPTION.
- The recurrent state cannot roll back. Reuse is valid only at a saved state snapshot. PROVEN (r1 and the llama.cpp issue text in R4).
- Neither model card says anything about prefix caching or concurrency. PROVEN (absence, r1).

Meaning for the proxy: the cost of a cache is the KV size plus each checkpoint plus one live state. A conversation of 64k tokens on the 27B model holds about 4 GiB of KV and about 150 MiB for each checkpoint. A request that lands on another node loses all of this and pays a full prefill. The proxy must measure sizes from the engine and must not trust the computed values. See [05-engine-behaviour.md](05-engine-behaviour.md).

### R2. vLLM prefix caching and hybrid models

Evidence: [r2-vllm](evidence/research/r2-vllm-20261007T022033Z.md).

Question: how does vLLM cache prefixes, what breaks on hybrid models, and what happens at a saturated engine?

Method: the agent read vLLM docs, release notes, issues and pull requests. The latest release seen was v0.31.0 on 2026-10-05.

Key facts:
- Dense models hash each KV block with its parent hash. The free list evicts by least recent use. PROVEN (vLLM design doc).
- Hybrid models need a large attention block so that one block holds one recurrent state. The block is 528 tokens for Qwen 3.5 and grows to 2096 tokens on the largest models. PROVEN (issues 40696 and 45238).
- In `align` mode the engine keeps only the recurrent state at the last block boundary of each prompt. PROVEN (issue 45238, checked by s6 source read).
- If that one state falls in tokens that are unique to the request, all reuse drops to zero without a signal. Example: 1600 shared and 600 unique tokens gave 52 of 64 block hits. The 1500 and 700 case gave none. PROVEN (issue 45238, open).
- Align mode costs 13 to 16 percent of output throughput even with zero hits on one tested model (Nemotron-3.5-Lightning). PROVEN (issue 60008, open on 2026-10-02).
- Prompt cache with multi-token prediction corrupted output on hybrids in v0.28.0. Nobody checked the merge status. ASSUMPTION (issues 47861 and 53912, snippets).
- Extra requests wait in the scheduler queue. KV memory exhaustion preempts requests that run, and the engine recomputes them. PROVEN (vLLM optimisation docs).
- The production-stack session router hashes a configurable header on a ring. The prefix-aware router assumes that seen prefixes stay cached. PROVEN (router docs, summary-level).
- llm-d measured 0.30 s mean time to first token with precise prefix routing and 13.3 s with approximate routing. Load-only routing gave 47 s. The setup was 8 pods on 16 H100 GPUs. PROVEN as a vendor benchmark.

Meaning for the proxy: vLLM offers no session pin. Affinity lives in the proxy. Hybrid hits are coarse and can fail without a signal. The proxy must measure the real hit from the response fields and from node metrics. It must not assume warmth. See lessons LES-001 and LES-002.

### R3. SGLang, routers and sticky-routing practice

Evidence: [r3-sglang-routing](evidence/research/r3-sglang-routing-20261007T022127Z.md).

Question: how do SGLang and the main routers keep a conversation warm, and what does general practice say about affinity windows and queues?

Method: the agent read SGLang docs and blogs, router docs, issues and papers.

Key facts:
- SGLang keeps recurrent states in a unified radix cache. Reuse is all or nothing at a saved state. PROVEN (LMSYS blog 2026-08-11).
- A session id in SGLang labels cache references. It is soft protection and not a memory pin. PROVEN (SGLang session cache docs).
- The SGLang gateway default policy is `cache_aware`. It builds a radix tree of request text per worker. The tree has no feedback from the worker. PROVEN (issue 7532 and gateway docs).
- The gateway has a known burst bug: running requests are invisible and a burst piles on one worker (29 to 3). PROVEN (smg issue 2804).
- An earlier bug counted load only after the response started, so fast decode and slow prefill collapsed onto one worker. PROVEN (sglang PR 27547).
- OpenRouter pins a conversation to a provider for 10 minutes of inactivity. The key is a hash of the first system message and the first non-system message. A `session_id` field or `x-session-id` header overrides it. PROVEN (OpenRouter docs).
- LiteLLM hard-coded its affinity time to live to 5 minutes. PROVEN (issue 28427).
- Hosted cache windows: Anthropic 5 minutes or 1 hour, refreshed on use. OpenAI 30 minutes on the newest models. PROVEN (vendor docs fetched 2026-10-07).
- Envoy and HAProxy bound the load on a hash ring with a factor between 1.25 and 2. PROVEN (docs). At five nodes the average load is small, so the bound is coarse. ASSUMPTION.
- No source gives a correct idle window for self-hosted affinity. PROVEN (absence).
- The only deadlock reference is an application-level case in pi-extensible-workflows issue 264. The inference-serving sources say nothing. PROVEN (absence).

Meaning for the proxy: use an exact table with a key and a last-seen time, as OpenRouter does. Count in-flight requests at admission, before dispatch. Treat the window length as a parameter to measure. See [03-affinity-and-keys.md](03-affinity-and-keys.md) and [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md).

### R4. Local engines: llama-server, Ollama, MLX, gufo

Evidence: [r4-local-engines](evidence/research/r4-local-engines-20261007T022141Z.md).

Question: what does each local engine do with prompt caches, queues and hybrid models?

Method: the agent read the llama-server README, issues, Ollama docs, MLX issues and the owner's gufo docs.

Key facts:
- llama-server defaults: 32 checkpoints per slot, 8192 MiB host cache, similarity 0.10 for slot choice. PROVEN (README, checked by s6 source).
- The recurrent state cannot trim partially. A prefix is reusable only where a checkpoint exists. PROVEN (issues 28425 and 20225).
- Issues report a cold prefill on every turn: 19794, 20225, 21831 and 22384. PROVEN (issue texts).
- Slot save and restore does not carry hybrid checkpoints. A 14,906-token restore took 0.12 s and then recomputed everything. PROVEN (issue 25913).
- Checkpoint sizes are 63 to 214 MiB each. This is a second-hand figure. ASSUMPTION (blog).
- Ollama runs 1 parallel request by default and queues up to 512, then answers 503. PROVEN (Ollama FAQ).
- MLX server caches checkpoints at segment ends. Hybrid caches are not trimmable. PROVEN (LocalAI issue 12451, then s6 source).
- gufo reuses an exact prefix only. It keeps up to 128 checkpoints for each conversation. PROVEN (gufo docs, s6 source).
- Owner measurements: a 60k-token request took 47 s on gufo and 230 s on llama.cpp on Strix Halo. A warm gufo conversation had 222 cache hits in 227 requests. PROVEN (owner repo docs).

Meaning for the proxy: a move between nodes is a cold prefill and costs minutes on slow hardware. Slot save and restore is not a move path. The proxy reads the real hit from `timings.cache_n` and `prompt_n`. See [05-engine-behaviour.md](05-engine-behaviour.md).

### R5. General gateways and layer-7 proxies against R1 to R10

Evidence: [r5-gateways](evidence/research/r5-gateways-20261007T055410Z.md).

Question: does a general gateway meet the requirement set, so that Legatus can adopt it?

Method: the agent cloned the repositories on 2026-10-07 and read source and docs. The agent scored each project against R1 to R10. Section 4 defines R1 to R10.

Key facts:
- No candidate does the affinity table (R4), the cap with a hold queue (R5) and byte-faithful streaming (R1) together. PROVEN (r5 verdict).
- HAProxy comes closest by mechanism. It is GPL-2 and its body hashing is fragile. PROVEN (r5, source read).
- Bifrost comes closest by features. It has no affinity and it re-serialises requests. PROVEN (r5).
- LiteLLM has affinity and hooks. Its cap answers 429 and does not wait. PROVEN (r5, source lines cited).
- GitHub marks TensorZero as archived. NadirClaw has a noncommercial licence since 0.22.0. PROVEN (r5).

Meaning for the proxy: see section 4.

### R6. Inference-aware routers and local balancers

Evidence: [r6-routers](evidence/research/r6-routers-20261007T055357Z.md).

Question: does a router built for LLM pools meet the requirement set?

Method: the agent cloned and read the repositories at HEAD on 2026-10-07.

Key facts:
- Olla is the closest project. It passes both protocols, keys sticky sessions by header and then by body hash, and expires idle keys. It has no admission cap, no hold queue and no load-aware spill. PROVEN (r6, `sticky.go`).
- Paddler has the best hold queue, but it works only with its own embedded llama.cpp agents. It has no affinity. PROVEN (r6).
- proxycache routes cache hits while the queue is short and migrates requests that wait after 120 s. It serves llama.cpp and OpenAI only and has no licence file. PROVEN (r6).
- llama-swap answers 429 at its cap on concurrent requests. PROVEN (r6, `concurrency.go`).
- The vLLM Rust router has an opt-in program scheduler that holds requests when a backend lacks capacity. It targets vLLM. PROVEN (r6, docs).
- Small new projects are young and often have one author. PROVEN (r6).

Meaning for the proxy: the novel parts are the cap, the hold queue and the load-aware spill. Section 4 uses this.

### R7. OpenAI Responses API, Codex 0.160.1 and local engines

Evidence: [r7](evidence/research/r7-responses-api-20261007T081115Z.md). Date 2026-10-07. Method: source reads of Codex (tag rust-v0.160.1) and of nine engines. The engines are llama.cpp, Ollama, vLLM, SGLang, mlx-lm, mlx-vlm, LocalAI and gufo. The agent also read issue lists and used the data of spikes C and D (fake server).

Question: which engines serve `/v1/responses`, and what must a proxy do to keep Codex working?

Key facts:
- Codex has no chat completions mode. `wire_api = "chat"` is a hard error, so a custom provider must serve `/v1/responses`. PROVEN (source).
- Codex on a custom provider sends HTTP with `store: false`, `stream: true`, `include: ["reasoning.encrypted_content"]`, `prompt_cache_key` equal to the session id and the full history each turn. It sends `previous_response_id` only on a websocket path that custom providers do not use. PROVEN (source and spike C).
- The SSE parser switches on the JSON `type`, takes tool calls and reasoning only from `response.output_item.done`, ignores unknown events and treats `usage` as optional. PROVEN (source).
- The idle timeout is 300 s, with 5 stream retries and 4 request retries. Codex never retries a 429. Codex retries a stream that closes before `response.completed`. PROVEN (source and spike D).
- llama-server b11462 and Ollama 0.40.0-rc6 serve the path as stateless translators. Both show open Codex gaps.
- vLLM and SGLang keep state only with a flag. mlx_lm.server has no `/v1/responses`.
- mlx-vlm, LM Studio, gufo and LocalAI serve the path.
- These are source and docs reads (PROVEN). They are NOT TESTED live.

Meaning for the proxy: pass the path through with no translation (DEC-065). Use `session-id`, `thread-id`, `prompt_cache_key` and the derived key in that order. Pass `x-codex-turn-state` both ways. Never answer 429. Record `response.id` for stateful engines.

Run real Codex conversations before a support claim (spike F, owner decision pending). See [06](06-protocols-and-harnesses.md) section 1.1 and [05](05-engine-behaviour.md) section 14.

### Where the research notes disagree or changed

| Topic | Note A | Note B | Status |
| --- | --- | --- | --- |
| Flash-Next KV per token | 24 KiB (R1 arithmetic) | About 38 KiB (owner profile, via R1) | Open. Measure from llama-server logs. |
| vLLM `all` mode | Pending or deprecated (R2) | Absent from `MambaCacheMode` at main (s6, source) | s6 wins for the main branch. |
| vLLM default for hybrids | Conflicting snippets (R2) | Prefix caching on by default, align forced (s6, `cache.py:141`) | s6 wins at commit 43b4aaea. |
| llama.cpp 64-token minimum | Stated in issue 22384 (R4) | Not found in code at b11460 (s6) | s6 wins for current code. |
| llama.cpp `-cms` default 8192 | Second-hand blog (R4) | Read from `common.h` (s6) | Now PROVEN. |
| SGLang checkpoint interval | Not found (R3) | Track interval 256 (s6, `exec_.py`) | Now PROVEN. |
| SGLang scheduler default | `fcfs` or `lpm` by source (R3) | Not re-read | Open. Depends on version. |
| Ollama v0.40.0 date | 2026-09-25 (R6, GitHub) | 2026-10-06 (R4, release site) | Open. Check before use. |
| `tokens_cached` in llama.cpp | Used as hit count (R4 hint) | N plus G, not the reused count (s6) | s6 wins. Use `timings.cache_n`. |
| llama.cpp checkpoint positions | Source reading: `L - 516`, `L - 4`, user starts at least 8192 apart (s6) | Measured: exactly the same positions, plus the first user-message start on the chat route (S7) | Agree. S7 PROVEN for Qwen3.5-2B. |
| Prefill speed in S7 | The S7 note says about 700 tok/s for the hybrid model | The S7 tables give 6000 tokens in 15 to 17 s, which is about 350 to 400 tok/s | Open. Use the tables. Speeds are NOT TRANSFERABLE. |

## 3. Spikes

All spikes ran on one Apple M2 with 16 GB, unless stated. Absolute speeds drift by 2 times between identical runs, because other processes shared the machine. Trust the ratios and shapes.

### Round 1 spikes (router prototype, Messages path, engines, Apple model, probe, DeepSeek Harness)

Evidence: [spike decisions](../decisions/2026-10-spike-decisions.md) and [scope reset](../decisions/2026-10-scope-reset.md). The original logs are not in this folder.

Question: can a thin Rust router front the real harnesses and engines?

Method: a Rust prototype (axum and reqwest) ran against scripted fakes and against Qwen3 1.7B on engines.

Key facts:
- The prototype added 0.06 ms to the first byte and 0.05 ms for each SSE chunk. It used 3 to 6 MB resident. Streams stayed byte-identical. PROVEN (round 1, fakes).
- A first-byte timeout of 3 s let pi recover in 3.4 s. Without it pi waited 300 s. PROVEN (round 1). v1 drops timeouts in the proxy, so a hung node holds the harness for its own timeout.
- Claude Code speaks Messages only. It sends `x-claude-code-session-id` and `x-claude-code-agent-id`, about 11 beta headers and mid-conversation `system` messages. PROVEN (round 1).
- A Messages 529 `overloaded_error` makes pi and Claude Code retry. A mid-stream `api_error` makes Claude Code fall back to a non-streaming request. PROVEN (round 1).
- Engine matrix: Ollama 0.35.1 truncates silently, llama-server 0.5.0 answers 400, mlx_lm 0.32.0 has no overflow check. PROVEN (round 1).
- Earlier text said pins must persist across restarts. The new proxy keeps soft state. PROVEN (round 1) and superseded by a decision.

Meaning for the proxy: the proxy keeps the streaming path byte-exact and thin. It keeps one node call site for the later retry work. See [09-restart-and-failure.md](09-restart-and-failure.md).

### Spike 1 and 1b. pi 1.0.3 lease header and its gaps

Evidence: [s1](evidence/spikes/s1-pi-lease-README.md) and [s1b](evidence/spikes/s1b-pi-lease-gaps-README.md).

Question: can a pi extension add a per-session header, and what do compaction, children and signals do to it?

Method: pi 1.0.3 ran against a scripted fake with an isolated configuration. Each run kept its own timestamped folder. No real model ran.

Key facts:
- The hook `before_provider_headers` can add a header to every request. This covers retries and the compaction call. PROVEN (s1b rows 1a and 1b).
- pi sends `x-session-affinity` only with `compat.sendSessionAffinityHeaders: true` and `sessionAffinityFormat: "openai-nosession"`. On the Messages path it sends none by default. PROVEN (s1b rows 1c and 2a').
- The compaction call carries no `x-session-affinity`. PROVEN (s1b row 1d).
- Compaction after a context overflow retries once with the summary. PROVEN (s1b row 2b).
- SIGINT does not release a lease. SIGKILL leaves it active. PROVEN (s1b rows 1h and 2c).
- The request `model` field stays at the role name. A router rewrite by lease covers compaction calls without harness code. PROVEN (s1b rows 4a and 4c).
- Workflow children run in separate processes. pi-subagents refuses two parallel foreground children. PROVEN (s1b rows 3c and 3h).

Meaning for the proxy: the proxy cannot assume a session header on every request. The scope reset parked the lease parts ([16-parked-capability-manager.md](16-parked-capability-manager.md)). The header and compaction facts stay.

### Spike 2. Rust router under tokio paused time

Evidence: [s2](evidence/spikes/s2-paused-time-README.md).

Question: can the real proxy run on virtual time for tests?

Method: 27 tests with an in-memory transport on tokio 1.53.2, hyper 1.12.0 and axum 0.7.9.

Key facts:
- Virtual time works with an in-memory transport. Five virtual hours ran in 0.2 s. PROVEN (s2 F3 and F4).
- Real sockets break paused time. 290 of 300 loopback requests hit a 1 s timeout. PROVEN (s2 F2).
- `tokio::time::advance` is one jump. A driver `sleep` steps through the timers. PROVEN (s2 finding 1).
- Timer resolution is 1 ms and deadlines round up. Same-instant order is first in, first out. PROVEN (s2 findings 2).
- `std::time::Instant` and `std::thread::sleep` are not virtual. PROVEN (s2 F6).
- Scaled mode over real sockets showed lateness of 0 to 11 ms. PROVEN (s2 F8).

Meaning for the proxy: the proxy needs a transport seam, tokio instants only and a wall-clock trait. See [13-test-fixtures-and-scenarios.md](13-test-fixtures-and-scenarios.md).

### Spike 3. Slots against real concurrency

Evidence: [s3](evidence/spikes/s3-slots-README.md).

Question: does the declared slot count match the useful concurrency of each engine?

Method: N of 1, 2, 4 and 8 parallel requests, three repetitions, on Ollama 0.35.1, llama-server 0.5.0 and mlx_lm 0.32.0 with Qwen3 1.7B.

Key facts:
- Ollama defaults to 1 slot. Queued requests receive no headers until service starts. `/api/ps` has no busy field. PROVEN (s3 findings 1 and 2).
- llama-server `/props total_slots` equals the running slots. `/metrics` shows `requests_processing` and `requests_deferred` exactly. Queued requests receive headers early. PROVEN (s3 finding 3).
- With `-c 16384 -np 4` each slot has 4096 tokens. A 6850-token prompt received a 400 error. PROVEN (s3 finding 3).
- mlx_lm runs 8 requests at once but gains nothing. Aggregate speed stays near 55 tokens per second while per-request speed divides by N. PROVEN (s3 finding 4).
- The best gain is 1.6 to 1.8 times at 4 slots on decode-bound traffic. It is about 1.3 times on prefill-bound traffic. PROVEN (s3 finding 5).
- Prefill is one shared compute pool. A second long prompt delays the first by up to 2.1 to 2.4 times. PROVEN (s3 finding 7).
- A prefill-bound probe picks the wrong knee. PROVEN (s3 design consequences).
- llama-server closes idle connections after 5 s. A reused connection failed once. PROVEN (s3 finding 9).

Meaning for the proxy: the cap per node comes from the engine facts and a measured knee. See [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md).

### Spike 4. Tool-call smoke suite

Evidence: [s4](evidence/spikes/s4-smoke-README.md).

Question: can a model that fits this machine pass a tool-call smoke suite?

Method: 20 trials for each case on Qwen3 1.7B and 4B, on Ollama and llama-server, with thinking on and off.

Key facts:
- Thinking is the largest effect. The 1.7B common-5 score rose from 0.70 to 0.96 on Ollama at temperature 0. PROVEN (s4).
- The same GGUF scored 0.70 on Ollama and 0.82 on llama-server with thinking off. The engine template matters. PROVEN (s4).
- The 4B model scored 0.94 and 0.95. The 4B library build cannot turn thinking off. PROVEN (s4).
- Thinking text leaked into `content` with status 200 for the 4B model with `reasoning_effort: none`. PROVEN (s4).
- With `--no-jinja` llama-server answers 500 for tool requests. PROVEN (s4).
- pi 1.0.3 completed a one-tool task on 5 of 5 trials on llama-server. PROVEN (s4, small sample).

Meaning for the proxy: this spike measures models, not the proxy. It shows that thinking switches and templates differ by engine and that stub engines need recorded captures. See [05-engine-behaviour.md](05-engine-behaviour.md).

### Spike 5. pi 1.0.3 against refused, 503, mid-stream drop and slow first byte

Evidence: [s5](evidence/spikes/s5-pi-restart-README.md).

Question: how long does pi tolerate a proxy that is not ready?

Method: a scripted fake with late listen, status codes, resets, mid-stream drops and slow first bytes. The agent also read the retry code.

Key facts:
- pi retries 3 times at 2, 4 and 8 s. The window is 14.1 s from the first failure. PROVEN (s5 rows 1 and 2).
- pi decides a retry by a regular expression on the error text, not by status. PROVEN (s5 mechanism).
- pi ignores `Retry-After`, even at 120 s. PROVEN (s5 row 2b).
- pi does not retry 400, 401, 404, 409, 422 or 529. It retries 429, 500, 502, 503 and 504. PROVEN (s5 status table). Spike 5 ran the Anthropic path only for 503 and 400.
- A slow first byte of 5, 20 and 60 s and a hold of 120 s all succeeded with no retry. A true hang ended at 300.6 s. PROVEN (s5 row 4).
- pi retries a mid-stream drop and discards the partial text. PROVEN (s5 row 3b).
- A held connection is the best way to start. It gave a ready-wait of about 300 s instead of 14 s. PROVEN (s5 verdict).

Meaning for the proxy: the proxy listens at once and holds requests until it is ready. The hold limit plus the time until the node head stays at or below 290 s. See [09-restart-and-failure.md](09-restart-and-failure.md). Spike D later measured the other harnesses.

### Spike 6. Checkpoint policy read from source

Evidence: [s6](evidence/spikes/s6-checkpoint-source-README.md), [clones and versions](evidence/spikes/notes-clones-and-versions-20261007T054753Z.md), [llama.cpp](evidence/spikes/notes-llamacpp-20261007T054753Z.md) with [errata](evidence/spikes/notes-llamacpp-errata-20261007T054827Z.md), [vLLM](evidence/spikes/notes-vllm-20261007T054827Z.md), [SGLang](evidence/spikes/notes-sglang-20261007T054827Z.md), [vLLM and SGLang errata](evidence/spikes/notes-vllm-sglang-errata-20261007T054857Z.md), [MLX](evidence/spikes/notes-mlx-20261007T054857Z.md) and [gufo](evidence/spikes/notes-gufo-20261007T054857Z.md).

Question: where does each engine save recurrent state, and what does a divergent request cost?

Method: shallow clones on 2026-10-07 and a read of the checkpoint code, with file and line references. No engine ran. The common model is: recompute(D) = N minus the largest checkpoint position that is at most D. N is the new prompt length. D is the common prefix with the stored sequence.

Key facts:
- llama.cpp keeps checkpoints at L minus 516 and L minus 4, at user-message starts at least 8192 apart, and the live state. It takes none after generation. PROVEN (s6, b11460).
- A divergent request removes all later checkpoints of the slot. PROVEN (s6, llama.cpp step 4).
- vLLM keeps one state per prompt at floor((P minus 1) / B) times B, plus junction states. It keeps none for generated tokens. PROVEN (s6, commit 43b4aaea).
- SGLang tracks states on a grid and every 256 decode tokens by default. PROVEN (s6, source).
- gufo keeps up to 4 grid checkpoints on 2048-token steps, a stable boundary, the full prompt and learned branches. PROVEN (s6, source and docs).
- mlx-lm keeps the system end, the user-segment end and the finished sequence. It keeps 10 entries by default. PROVEN (s6, source).
- Response fields: llama.cpp `timings.cache_n`. vLLM `cached_tokens` only with `--enable-prompt-tokens-details`. PROVEN (s6).
- The step from 18000 to 17999 tokens is a staircase, not a sawtooth. PROVEN (s6).

Meaning for the proxy: the cost of a changed prompt depends on where the change falls and on the engine. The proxy reports the cost formula and the measured reuse. It does not change the checkpoint policy of any engine. See [05-engine-behaviour.md](05-engine-behaviour.md).

### Spike C. Key stability in real harness traffic

Evidence: [spike C README](evidence/spikes/sC-key-stability/README.md), [analysis](evidence/spikes/sC-key-stability/analysis-20261007T065841Z.txt), [per-run key tables](evidence/spikes/sC-key-stability/analysis-20261007T065922Z.txt), [keyv1.py](evidence/spikes/sC-key-stability/keyv1.py), [19 test vectors](evidence/spikes/sC-key-stability/test-vectors-keyv1-20261007T065515Z.json), [versions](evidence/spikes/sC-key-stability/VERSIONS-20261007T065702Z.txt).

Question: does the key stay the same on every turn of one conversation, and does it differ between conversations?

Method: real binaries of pi 1.0.3, Claude Code 2.1.291, opencode 1.18.35, DeepSeek Harness 0.2.0-rc.2, Codex 0.160.1 and four SDKs. They ran against a scripted fake server that recorded raw bodies and headers. No real model ran.

Key facts:
- The key from the first system text and the first non-system message was byte-stable on every turn in all harnesses. PROVEN (spike C).
- It breaks at compaction, by design. It does not separate two conversations with the same first prompt. It collides on Codex, because the first input item is an environment block. PROVEN.
- The first 512 bytes were constant across all conversations in four harnesses. The Olla rule is unusable. PROVEN.
- The working directory sits at character 8663 in opencode and 17198 in Codex. PROVEN.
- Headers: `x-session-affinity` (pi with the flag, opencode), `x-session-id` (opencode), `x-claude-code-session-id` with agent ids, `session-id`, `thread-id` and `prompt_cache_key` (Codex). PROVEN.
- A Claude Code child keeps the session id and adds a distinct agent id. PROVEN.
- A retry after a 503 keeps a byte-identical body and header in every harness. PROVEN.
- Not tested: Open WebUI, real models, Claude Code interactive subagents, Codex compaction and subagents.

Meaning for the proxy: header first, body hash as the fallback, with the canonicalisation of `keyv1.py` (DEC-063). See [03](03-affinity-and-keys.md).

### Spike D. Hold tolerance of harnesses and SDKs

Evidence: [spike D README](evidence/spikes/sD-hold-tolerance/README.md), [status matrix](evidence/spikes/sD-hold-tolerance/analysis-err-20261007T071847Z.txt), [grid](evidence/spikes/sD-hold-tolerance/matrix-stage2-20261007T071847Z.txt), and the stage analyses in the same folder.

Question: does a proxy that holds a request for 250 s with no byte survive each harness, and which status ends the hold best?

Method: the same real binaries and SDKs as spike C, with Node v24.15.0 and curl 8.7.1, against a scripted slow fake server. The grid held requests for 15 s to 600 s. Separate runs tested statuses, `Retry-After` and errors after a 200. The stage 1 results above 700 s are void because the fake crashed.

Key facts:
- A hold of 250 s completed at the first attempt in every harness. PROVEN.
- Give-up times with no byte: curl and Codex never (to 600 s). pi and DeepSeek Harness 299 s. opencode 300 s. Node SDKs and `fetch` 301 s. Claude Code 360 s. Python SDKs 600 s. PROVEN.
- The binding limit is the sum of `hold_limit` and the time until the node head. The sum is at most 290 s. PROVEN as a derivation from the table.
- Every harness retries 503, 502, 504, 500 and 529. Codex fails at once on 429. Claude Code fails at once on `Retry-After` of 90 s or more. PROVEN.
- An early head with keep-alive does not help DeepSeek Harness and needs a real SSE event for Codex. Rejected for v1. PROVEN.
- Retry patience when all attempts fail: SDKs 3 attempts, pi 4, DeepSeek Harness 7, opencode 9, Codex 30, Claude Code 11. PROVEN.

Meaning for the proxy: hold up to 250 s with no byte, then answer 503 (DEC-064). See [04](04-admission-control-and-queueing.md), section 8.

### Spike 7. Hybrid cache measurement on this machine

Evidence: [s7](evidence/spikes/s7-hybrid-cache/README.md). Date 2026-10-07. Apple M2 16 GB, under load from other agents.

Question: does affinity pay on each engine and model, and how many warm conversations fit?

Method: real models, no fakes. llama-server b11459 (commit f498f864f) and brew 0.5.0 (b11146). Hybrid model Qwen3.5-2B Q4_K_M. Dense control qwen3:1.7b. A fresh engine for each sweep trial, three repetitions. Token route sweeps on `/completion` and chat route edits on `/v1/chat/completions` with `--jinja`. Two interleaved conversations at `-np 1` and `-np 2`.

Key facts, all PROVEN for this model and identical in three of three repetitions:
- On the token route the hybrid model has exactly two checkpoints for each prompt, at `N - 516` and `N - 4`. The recompute is `N` minus the largest checkpoint at or below `D`, or `N` if none exists. At N = 6000, D = 5490 recomputes 516 tokens and D = 5480 recomputes 6000.
- A one-token step is a cliff. D = 5483 recomputes 6000 tokens in 15.0 s. D = 5484 recomputes 516 tokens in 1.3 s. The dense control degrades linearly.
- On the chat route the checkpoints are the first user-message start, the last user-message start, `N - 516` and `N - 4`. Other user-message starts receive one only at least 8192 tokens after the previous checkpoint. An edit at D = 194 to 4730 in a 6075-token history recomputes 5942 tokens (about 22 s). An edit of the last user message recomputes 20 tokens.
- `--checkpoint-min-step 1024` gives checkpoints at user-message boundaries for about 19 MiB each for each slot.
- A pure append recomputes 201 tokens for 200 new tokens on both models. A template divergence 2 tokens before the end recomputes 332 tokens.
- Two interleaved conversations both stay warm at `-np 1` and `-np 2`, because `--cache-ram` (default 8192 MiB) saves an idle slot in host memory. A 6k hybrid conversation takes about 129 MiB and a swap takes about 30 ms. With `-np 1 --cache-ram 0` every turn is cold (about 23 s).
- Both builds gave the same checkpoint positions. The s6 source read matched every measurement.

Not tested: streaming usage fields, Qwen3-Next architecture models, eviction beyond 32 checkpoints, a full 8 GiB cache, more than two builds. The 2B model is small. The rules transfer to Flash-Next and 27B as ASSUMPTION. Speeds and sizes are NOT TRANSFERABLE. Spike B (a script for the owner machines, DEC-060) measures them.

Meaning for the proxy: affinity pays on append-only traffic and gives a cold prefill when the history is rewritten. The proxy keeps prompts append-only on hybrid nodes. The warm capacity of a node is not its slot count (DEC-066). See [05](05-engine-behaviour.md) sections 4.5 and 11 and [04](04-admission-control-and-queueing.md) section 4.5.

## 4. Build versus adopt

### 4.1 The requirement set R1 to R10

The six research briefs scored projects against R1 to R10. The proxy brief does not list them. This file reconstructs them from the notes. They are not PRX requirement IDs. They are a frame for scores.

| ID | Frame requirement |
| --- | --- |
| R1 | Serve OpenAI chat and Anthropic Messages. Stream byte for byte. |
| R2 | Work with local engines (llama-server, Ollama, vLLM, SGLang, MLX). |
| R3 | Map a model alias to a pool of nodes. |
| R4 | Keep affinity: key from a header or a body hash, exact table, idle expiry, spill. |
| R5 | Cap running requests per node and hold the rest with a time limit. |
| R6 | Read engine health and load signals. |
| R7 | Log each request, show metrics, log cached tokens, offer a dashboard. |
| R8 | One small binary, permissive licence, runs on macOS. |
| R9 | Patch the request per node (for example thinking switches). |
| R10 | Do not force retries or failover. The operator can turn them off. |

### 4.2 Candidates, versions, dates and licences

Dates come from the research notes (2026-10-07). This table does not repeat star counts.

| Project | Language | Licence | Version or last activity | Role |
| --- | --- | --- | --- | --- |
| Olla | Go | Apache-2.0 | v0.0.29 on 2026-08-10, push 2026-09-27 | Closest router |
| Bifrost | Go | Apache-2.0 | commit 2026-10-07 | Gateway |
| LiteLLM proxy | Python | MIT | v1.104.0 on 2026-10-03 | Gateway |
| HAProxy | C | GPL-2 | 3.4.0, push 2026-10-06 | Proxy |
| nginx OSS | C | BSD-2 | 2026-09-30 | Proxy |
| Envoy | C++ | Apache-2.0 | 2026-10-07 | Proxy |
| Envoy AI Gateway | Go | Apache-2.0 | v1.2.0 notes | Gateway |
| SGLang gateway (SMG) | Rust | Apache-2.0 | sglang v0.5.21 on 2026-10-02 | Router |
| vLLM router | Rust | Apache-2.0 | push 2026-10-02, no release | Router |
| production-stack | Python | Apache-2.0 | chart 0.1.13 on 2026-09-29 | Router |
| llm-d | Go | Apache-2.0 | v0.10.0 on 2026-09-29 | Kubernetes scheduler |
| Paddler | Rust | Apache-2.0 | v4.1.0 on 2026-07-19 | Balancer with agents |
| llama-swap | Go | MIT | v262 on 2026-10-03 | Process swapper |
| proxycache | Go | None seen | 2026-09 | llama.cpp proxy |
| Kong OSS | Lua | Apache-2.0 | 2026-10-02 | Gateway |
| APISIX | Lua | Apache-2.0 | 2026-10-04 | Gateway |
| Higress | Go | Apache-2.0 | 2026-10-05 | Gateway |
| Portkey | TypeScript | MIT | v1.15.2 on 2026-05-25 | Gateway, stale |
| TensorZero | Rust | Apache-2.0 | Archived, push 2026-06-11 | Dropped |
| NadirClaw | Python | PolyForm Noncommercial | 0.23.1 | Fails licence |

The notes also name Traefik, vLLM Semantic Router, LocalAI, GPUStack, exo, Ollama and several small projects. None of them changes the result.

### 4.3 Requirement matrix

Y is yes, P is partial, N is no, U is unverified, a dash is not scored. The rows merge the R5 and R6 matrices. Evidence: [r5](evidence/research/r5-gateways-20261007T055410Z.md) and [r6](evidence/research/r6-routers-20261007T055357Z.md).

| Project | R1 | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Olla | Y | Y | P | P | N | P | P | Y | P | Y |
| Bifrost | P | Y | Y | N | P | U | Y | P | Y | Y |
| LiteLLM | P | Y | Y | P | N | P | Y | N | Y | Y |
| HAProxy | Y | Y | P | P | Y | P | P | P | P | Y |
| nginx OSS | Y | Y | P | P | N | P | P | Y | P | Y |
| Envoy | Y | Y | P | P | N | P | Y | N | P | Y |
| Envoy AI Gateway | Y | Y | Y | P | N | P | Y | N | P | Y |
| APISIX | P | Y | Y | P | U | Y | Y | N | Y | Y |
| Higress | P | Y | Y | P | N | P | Y | N | P | Y |
| Kong OSS | P | P | P | N | N | U | P | N | Y | Y |
| Portkey | U | Y | P | N | N | U | P | P | P | Y |
| SGLang gateway | P | P | P | P | P | N | P | N | P | - |
| vLLM router | P | N | P | P | P | N | P | P | P | - |
| llm-d | N | N | P | P | P | N | P | N | P | - |
| Paddler | N | N | P | N | Y | P | Y | Y | N | - |
| llama-swap | Y | P | N | N | N | N | P | Y | P | - |
| Ollama | Y | N | N | N | P | P | N | Y | N | - |
| proxycache | N | N | P | Y | Y | Y | P | Y | P | - |
| llm-router (AutreMachine) | N | P | Y | N | Y | N | Y | P | P | - |
| NadirClaw | N | P | N | N | N | N | P | N | N | Y |

Notes on the matrix:
- The R8 mark for HAProxy is P because of the GPL-2 licence. PROVEN (r5).
- The R8 mark for proxycache is Y for size, but it has no licence file. The proxy cannot reuse unlicensed code. PROVEN (r6).
- Most marks for R1 are ASSUMPTION. Nobody audited byte-faithfulness of SSE for Olla, Bifrost or the LiteLLM Messages path. NOT TESTED.
- The marks come from two agents who used the same letter scale but not the same method. Treat a P as "read the source before you rely on it".

### 4.4 Analysis

The matrix shows three facts.

1. No project has both R4 and R5. HAProxy has R5 and a header key. Olla has the key order and has no cap. proxycache has both ideas but serves llama.cpp and OpenAI only and has no licence. PROVEN (r5, r6).
2. The projects that fit best by mechanism fail on a hard constraint. HAProxy is GPL-2 and limits a body key to its buffer size. The default is 16384 bytes. LiteLLM answers 429 at its cap and uses hundreds of MB. The router projects assume one engine. PROVEN (r5, r6).
3. Nobody ships the parts that carry the vision. These parts are the exact table with a fallback key, the per-node cap with a hold queue, and the engine cache fields as feedback. R5 lists them as gaps in every adoption path. PROVEN (r5 "Gaps after adoption").

The two research notes disagree on the second best choice.

| Source | Ranked recommendation | Effort estimate |
| --- | --- | --- |
| R5 | Build a thin Rust proxy. HAProxy is a reference spike only. | Build: small. Prototype added 0.06 ms and used 3 to 6 MB. |
| R6 | Spike Olla for two days. Extend it if it passes. Otherwise build thin. | Olla extension: 1 to 2 weeks of Go. Build: 3 to 5 weeks. |

Both effort estimates are ASSUMPTION. Nobody measured them. The Olla spike is NOT TESTED.

### 4.5 Conclusion

PROPOSED: build a thin Rust proxy. Do not adopt a project. The reasons are these.

- The decision log sets the language (Rust, one static binary). Olla is Go. An Olla fork breaks that decision. See [01-decisions.md](01-decisions.md).
- The novel parts are the whole product. They are the key table, the cap with a hold queue, the cache feedback and the per-node patches. Adoption does not remove this work in any path. PROVEN (r5).
- The needed protocol work is small. The proxy passes bytes. It needs no translation between OpenAI and Messages. This removes the main saving of Olla. PROPOSED.
- Olla is version 0.0.x with one dominant author. PROVEN (r6).
- The prototype already met the footprint target. PROVEN (round 1, fakes).

The cost of this choice is real. The proxy gives up 11 engine profiles, a dashboard and Messages translation. The proxy must rebuild the part of the profiles that it needs. The 3 to 5 week estimate is ASSUMPTION. The owner can lower the risk with the Olla spike in the benchmark plan ([13-test-fixtures-and-scenarios.md](13-test-fixtures-and-scenarios.md)). Olla is also a baseline to beat.

The open question about this choice is in [14-open-questions-and-risks.md](14-open-questions-and-risks.md).

### 4.6 Designs the proxy borrows

| Design | Source | What the proxy takes |
| --- | --- | --- |
| Conversation key | OpenRouter | Hash of the first system or developer message and the first non-system message. An explicit session value overrides it. Errors do not refresh the idle timer. |
| Cap and queue timeout | HAProxy | Per-server `maxconn` and `timeout queue`. The option `hash-preserve-affinity maxconn` skips full servers. |
| Hold queue | Paddler | A buffered request manager with a time limit and a maximum count of held requests. |
| Move rule | proxycache | Route a cache hit while the queue is short. Move a request that waits after a set time. Defaults seen: queue limit 2, backend queue 5, move after 120 s. |
| Key order | Olla | Session header, then body hash, then auth header, then client address. A sliding idle time of 600 s and a cap of 10000 entries. |
| Sticky response headers | Olla | Response headers that show a sticky hit, miss or repin. |
| Spill thresholds | SGLang gateway | Absolute and relative load thresholds before a conversation leaves its node. |
| Manual routing key | SGLang gateway | A key table that remaps only on an unhealthy node. |
| Hold on no capacity | vLLM router | A request pool that holds requests and resumes them in an order that protects the cache. |
| Slot pin idea | llamacpp-affinity-proxy | Inject `id_slot` from a header. One backend only. |
| Pin time to live | LiteLLM | A key-scoped pin cache. The time to live must follow the cache window and not stay constant. |

Each design is a source of ideas. The proxy copies no code. The details belong to the files that own them: [03-affinity-and-keys.md](03-affinity-and-keys.md) and [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md).

### 4.7 Earlier prior art in the baseline

The baseline [prior-art](../baseline/prior-art.md) covers coordination projects (Bernstein, MCP Agent Mail, agent-comm, Claude Code agent teams). They belong to the parked capability manager ([16-parked-capability-manager.md](16-parked-capability-manager.md)). The same file rejects routing inside an inference server. It names llama-server router mode and the vLLM Semantic Router. The rejection stands for the proxy.

## Sources

Research notes and spikes (local copies):
- [r7](evidence/research/r7-responses-api-20261007T081115Z.md), [spike 7](evidence/spikes/s7-hybrid-cache/README.md).
- [r1-models](evidence/research/r1-models-20261007T022035Z.md), [r2-vllm](evidence/research/r2-vllm-20261007T022033Z.md), [r3-sglang-routing](evidence/research/r3-sglang-routing-20261007T022127Z.md), [r4-local-engines](evidence/research/r4-local-engines-20261007T022141Z.md), [r5-gateways](evidence/research/r5-gateways-20261007T055410Z.md), [r6-routers](evidence/research/r6-routers-20261007T055357Z.md)
- [spike C](evidence/spikes/sC-key-stability/README.md), [spike D](evidence/spikes/sD-hold-tolerance/README.md), [s1](evidence/spikes/s1-pi-lease-README.md), [s1b](evidence/spikes/s1b-pi-lease-gaps-README.md), [s2](evidence/spikes/s2-paused-time-README.md), [s3](evidence/spikes/s3-slots-README.md), [s4](evidence/spikes/s4-smoke-README.md), [s5](evidence/spikes/s5-pi-restart-README.md), [s6](evidence/spikes/s6-checkpoint-source-README.md)
- [Spike decisions](../decisions/2026-10-spike-decisions.md), [scope reset](../decisions/2026-10-scope-reset.md), [baseline prior art](../baseline/prior-art.md)

External URLs (read by the research agents on 2026-10-07):
- https://docs.vllm.ai/en/latest/design/hybrid_kv_cache_manager/ and https://github.com/vllm-project/vllm/issues/45238
- https://openrouter.ai/docs/guides/best-practices/prompt-caching
- https://github.com/thushan/olla, https://github.com/distantmagic/paddler, https://github.com/utopiafallen/proxycache
- https://github.com/BerriAI/litellm, https://github.com/maximhq/bifrost, https://github.com/haproxy/haproxy
- https://github.com/sgl-project/sglang/tree/main/sgl-model-gateway, https://github.com/vllm-project/router, https://llm-d.ai/blog/kvcache-wins-you-can-see
- https://github.com/mostlygeek/llama-swap, https://github.com/JochenLinnemann/llamacpp-affinity-proxy
