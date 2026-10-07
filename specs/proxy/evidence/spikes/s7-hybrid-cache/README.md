# Spike 7: prefix-cache reuse, hybrid (Gated DeltaNet) vs dense, llama.cpp (REAL models, M2 16 GB)

Verdict: with default settings the cost is a CLIFF/STAIRCASE, not a sawtooth. Hybrid recomputes the whole prompt unless the edit lies after a checkpoint; dense recomputes only prompt_n = N - D (linear ramp).

## Setup (evidence)
- Hybrid model: unsloth/Qwen3.5-2B-GGUF Q4_K_M, 1,280,835,840 B (config.json: Qwen3_5ForConditionalGeneration, 24 layers, layer_types = 3 linear_attention : 1 full_attention). Chosen over 0.8B because its compute is closest to the dense control (qwen3:1.7b); 0.8B exists (unsloth/Qwen3.5-0.8B-GGUF) but was not run. Disk free before 49,391,484 KiB, after model+binary 48,066,708 KiB (~1.3 GB used; other agents later lowered it to 24 GB).
- Dense control: qwen3:1.7b Ollama blob sha256-3d0b79... via brew/new llama-server.
- New binary: ggml-org/llama.cpp tag b11459 (0.6.0-dev, commit f498f864f), llama-b11459-bin-macos-arm64.tar.gz sha256 04cd4ab4...e8c4e (matches GitHub digest). The "latest" release v0.6.0 ships no binaries (only nightly-tag.txt), so I used the newest tagged build with binaries (pre-release). Brew: 0.5.0 (b11146, commit 7fe450e19) NOT upgraded. Both load Qwen3.5 and behave identically on checkpoint tests (g).
- Defaults (from --help and startup log, same in both): --ctx-checkpoints 32 per slot, --checkpoint-min-step 8192, --cache-ram 8192 MiB, --cache-idle-slots on, --slot-prompt-similarity 0.10, -ub 512, kv_unified false with -np N and explicit -c. Flags used: --jinja -np {1,2} -c 20480*np -ngl 99 -lv 4 (lv 4 needed to log checkpoints).
- Harness: lib.py, sweep.py, scen.py, chatsweep.py, conc.py. Fresh server per sweep trial (no cross-trial cache contamination), 3 reps. Sweep prompts are raw token arrays on /completion (exact control of D); chat tests use /v1/chat/completions. Metrics: timings.cache_n, prompt_n, prompt_ms; chat also usage.prompt_tokens_details.cached_tokens (equals cache_n).
- Machine load: load average 3-6 at the start, spiking to 27-107 from other agents mid-run (see `uptime` in each trials.jsonl); prompt_ms is therefore noisy (about 700 tok/s hybrid, 350 tok/s at 12k), prompt_n/cache_n are deterministic (identical across all 3 reps). Real model only (no fakes).
- Runs: runs/<name>-<UTC timestamp>/ (cmd + version in *.meta.json, server logs, trials.jsonl / results json). Earlier aborted run runs/sweep-hyb-new-np1-20261007T055337Z kept (partial; I stopped it to shrink the 12k grid).

## (c) Divergence sweep, token route, defaults (median of 3; prompt_n deterministic)
N=6000 (hybrid | dense): D -> recomputed tokens, prompt_ms
| D | hybrid prompt_n | hybrid ms | dense prompt_n | dense ms |
|---|---|---|---|---|
| 100 | 6000 | 17013 | 5900 | 18666 |
| 1000 | 6000 | 14938 | 5000 | 17982 |
| 3000 | 6000 | 14237 | 3000 | 11979 |
| 4500 | 6000 | 15164 | 1500 | 6196 |
| 5000 | 6000 | 17075 | 1000 | 3240 |
| 5400 | 6000 | 17413 | 600 | 2707 |
| 5480 (=N-520) | 6000 | 15962 | 520 | 2442 |
| 5490 (=N-510) | 516 | 1658 | 510 | 2231 |
| 5900 | 516 | 1708 | 100 | 444 |
| 5997 (=N-3) | 4 | 62 | 3 | 72 |
| no edit | 4 | 66 | 1 | 38 |

N=12000
| D | hybrid prompt_n | hybrid ms | dense prompt_n | dense ms |
|---|---|---|---|---|
| 100 | 12000 | 34348 | 11900 | 47202 |
| 6000 | 12000 | 37396 | 6000 | 32716 |
| 8200 | 12000 | 38075 | 3800 | 18357 |
| 10500 | 12000 | 38507 | 1500 | 7874 |
| 11480 (=N-520) | 12000 | 36627 | 520 | 3306 |
| 11490 (=N-510) | 516 | 1673 | 510 | 3099 |
| 11997 | 4 | 58 | 3 | 65 |

Inferred checkpoint positions (from logs, "created context checkpoint ... n_tokens"): on the token route exactly two per prompt: N-516 and N-4 (5484/5996 for N=6000; 11484/11996 for 12000). No periodic checkpoint at 8192 in the token route. Rule: recompute = N - (largest checkpoint n_tokens <= D), else N. Hybrid time at full re-prefill: 15-17 s at 6k and 34-38 s at 12k, vs ~0.06 s (reuse at N-4) and ~1.5 s (reuse at N-516).

## (d) One token earlier (3 reps, all identical)
Hybrid N=6000: D=5483 -> cache 0 / recompute 6000 (15.0 s); D=5484 -> cache 5484 / recompute 516 (1.3 s); D=5995 -> 516; D=5996 -> 4. N=12000: D=11483 -> 12000 (30.6 s); D=11484 -> 516 (1.4 s). A 1-token shift at a checkpoint boundary costs the entire prompt (12x-24x). Dense: 517 vs 516, 5 vs 4: no cliff.

## Chat route (--jinja, enable_thinking=false), edit one word in message i, 3 reps
Hybrid, defaults. Checkpoints seen: 133 (first user message start), user-message-start checkpoints only if >=8192 after the previous one (8404 at 12k), last user message start, N-516, N-4 (last_user adds ~15 tokens before the end).
| N | edit location (D) | cache_n | prompt_n | ms |
|---|---|---|---|---|
| 6075 | 194 / 1552 / 3176 / 4730 | 133 | 5942 | ~22000 |
| 6075 | 5772 / 5936 (last assistant) | 5559 | 516 | 2051 |
| 6075 | 6066 (last user) | 6056 | 20 | 150 |
| 12281 | 342 / 3176 / 6215 | 133 | 12148 | ~47500 |
| 12281 | 9354 / 11692 | 8404 | 3877 | ~16500 |
| 12281 | 12171 | 11765 | 516 | 2148 |
| 12281 | 12272 | 12262 | 20 | 310 |
Dense (same histories, 6196 / 12232 tokens): cache_n = D exactly in every case; e.g. D=3180 -> prompt_n 3016 (15.3 s); D=9205 -> 3027 (22.8 s); D=12223 -> 10. Linear ramp.
Hybrid with -cms 1024 (12k, 1 rep): checkpoints at 133, 1328, 2513, 3702, 4880, 6056, 7224, 8404, 9594, 10778, 11765...; D=3176 -> cache 2513 (9768 recomputed), D=6215 -> 6056 (6225), D=9354 -> 8404 (3877), D=11692 -> 10778 (1503): a true sawtooth with ~1.2k spacing (spacing = user-message starts, not arbitrary tokens) at the price of ~19 MiB RAM per checkpoint per slot.

## (a) Pure append and (b) template effect
(a) 300 generated tokens then +200 new tokens, token route: hybrid cache 6299/12299, recompute 201 (about 0.9 s) in 3/3 reps; dense identical (201). Pure append is free on both.
(b) Hybrid (Qwen3.5 thinks; 300-token cap ended inside thinking, finish=length, no content, so I substituted a fixed answer text, label: synthetic): the history was ~9.4k / 19k tokens (my sizing overshot). Second prompt diverges at prompt1_len-2 (lcp 9422 of 9424): cache_n 9420 (checkpoint N-4), recompute 332 = 200 new + the discarded tail; 19028-token case likewise cache 19024, 332. The 300 generated tokens are lost, cost is small ONLY because Qwen3.5's template diverges within 4 tokens of the end of prompt 1. A divergence 5 to 516 tokens before the end costs 516; earlier costs the whole prompt (see sweep). Dense control (qwen3:1.7b) did not reproduce the divergence (its 55-token answer had no reasoning, lcp = prompt1_len), so (b) for dense is just an append (cache 9171, recompute 358); NOT a template-effect comparison.

## (e) Two interleaved conversations A,B,A,B (6k history, +100 gen, +200 per turn, 3 reps; identical every rep)
- -np 1 default, hybrid AND dense: A0/B0 cold (6000), then every later turn cache 6099/6399/6699, recompute 201: both stay warm, because --cache-ram (8 GiB) saves the idle slot state into the host prompt cache on slot swap ("saving prompt with length 6099, total state size 90.9 MiB", cache state 129 MiB for hybrid) and restores it. Swap costs ~30 ms to save; turn prompt_ms 0.4-1.0 s (load-noise).
- -np 2: slots by LCP similarity (f_sim 0.968), both warm, same numbers.
- -np 1 with --cache-ram 0, hybrid: every turn cold: 0/6300, 0/6600, 0/6900, ~23 s each. So warmth for >1 conversation per slot depends on cache-ram, not on the slot count.
- brew 0.5.0 -np 1: same as new (201 recompute).

## (g) brew 0.5.0 vs b11459
Same results: sweep (6k, 3 reps): D<=5480 -> 6000 recomputed, 5490/5900 -> 516, 5997 -> 4; same checkpoint positions. Brew's --help also has -cms 8192 and -cram 8192 defaults. No version effect observed between b11146 and b11459 (only 2 builds compared; older builds not tested; issue 22384 in s6 notes mentions an older 64-token minimum, not tested).

## Design consequences for a cache-affine proxy
- Cost model is not "prefix ratio". Hybrid: recompute = N - checkpoint(D), where checkpoints = {N_prev-516, N_prev-4} of the last prompt, user-message starts spaced >=8192 apart (default) and the last user message start. Treat any change earlier than ~516 tokens before the end of the previous prompt as full re-prefill (15 s at 6k, 35 s at 12k on this M2 under load). Dense degrades linearly.
- Affinity window: only "append only" (or edits within the last ~500 tokens, incl. a template that rewrites only the last assistant turn) are safe on hybrid models. Systems that rewrite history (compaction, tool-result trimming, thinking stripping 5+ tokens back, changed system prompt) get a cliff; the proxy should keep prompts append-only for hybrid nodes.
- Capacity: one warm conversation per -np slot, plus more in --cache-ram: hybrid 6k conversation = ~129 MiB (state 91 MiB + 2 checkpoints 19 MiB each); 8 GiB default holds dozens at this size but it is per llama-server process and token-limited (20480 tokens/slot shown in the log limits line). Per-node capacity should count slots + cache-ram budget; sizes scale with KV length for the full-attention layers. Not tested: eviction when the 8 GiB fills, bigger models.
- Detecting a cold turn from the response: timings.prompt_n (recomputed tokens) and timings.cache_n; in chat completions, usage.prompt_tokens_details.cached_tokens equals cache_n. Cold if prompt_n > (new tokens expected). Stream chunks carry timings only if the final/usage chunk does: NOT tested for streaming; the non-stream shape was verified.
- Flag to tune if long-history edits matter: -cms (checkpoint-min-step) lower, e.g. 1024, gives user-message-boundary checkpoints; -ctxcp bounds count (32 default). Not tested at -np 2 or under memory pressure.

## Not tested / caveats
Streaming usage fields; Qwen3-Next/Qwen3.8; the 0.8B model; checkpoint eviction (>32) and cache-ram exhaustion; tool-call (role TOOL) message boundaries; cross-version beyond 2 builds. Sweep trials restart the server each time, so they do not exercise host-cache interaction (that is covered by (e)). Timings are inflated by machine load. Source reading by the s6 agent (notes-llamacpp-*.md) predicted exactly: checkpoints {L-516, L-4}, user-start >=8192, c*(D) closed form; every observation above matched it.

## Housekeeping
Servers stopped, lock released (rmdir), nothing deleted. Model file models/Qwen3.5-2B-Q4_K_M.gguf (1.28 GB) and bin/ (42 MB) left in this dir for re-runs.
