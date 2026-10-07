# Spike 4: tool-call smoke suite on this 16 GB M2 (finding about THIS hardware and THESE models, not about Legatus)

Gate question: can any local model that fits this machine pass the smoke suite, so story 8 does not leave the first slice with `NoEffectiveCandidate`?
**Verdict: GO-WITH-CHANGES.** At the specced `SMOKE_MIN_PASS` 0.95 and `SMOKE_MIN_VALID_JSON` 1.0 nothing I tested passes reliably. At a PROPOSED 0.90 plus changes below, qwen3:4b passes with margin on Ollama; the 1.7B is borderline and flaps.

All results are real models (no fakes). Run dirs are timestamped and never overwritten (`results/run-<UTC>-<tag>/*.jsonl` plus `*.cmd.json`; `logs/run-<UTC>-<tag>/` with command, env, versions, server logs). Pre-rule files were copied to `logs/pre-rule-20261006T223952Z/`. Two honesty notes: the first `logs/pilot.out` (a LOCK TIMEOUT line) was overwritten before the rule arrived; `pkill -f` was used once on my own waiter shell (no other process matched).

## Versions (installed vs latest, checked 2026-10-07)
| item | used | latest |
|---|---|---|
| Ollama (frozen binary, :11435, ctx 16384 for runs C+, default for q17-off run) | 0.35.1 | v0.40.0 |
| llama-server (brew, :18101, `-c 16384 -np 1`) | 0.5.0 b11146 | llama.cpp v0.6.0 |
| pi | 1.0.3 | 1.0.4 |
| mlx_lm | not run (only a 1.7B 4-bit exists; out of time) | |
| harness | `smoke.py`, python 3.11 via `uv run --no-project`, stdlib only | |

Models (both Q4_K_M GGUF, same blob served by both engines): `qwen3:1.7b` digest 8f68893c685c (blob sha256-3d0b79...); `qwen3:4b` digest 359d7dd4bcda (blob sha256-3e4cb141...), pulled by me: 2.5 GB (blob 2497280480 B), `~/.ollama/models` 1.3 GB -> 3.6 GB; `df` avail 43138288 KB before (22:31Z) -> 39418688 KB after pull -> 44 Gi at end (other agents also write/delete, so only the model-dir delta is attributable). Within the 3.5 GB cap, spike total about 2.5 GB.
**qwen3:4b in the Ollama library is a thinking-only build** (thinking cannot be turned off: `reasoning_effort=none`, `think:false` on /api/chat, `enable_thinking=false` on llama-server all still produce the reasoning; with `none` it leaks into `content`). `logs/probe-think-*/`. I should have pulled an instruct-2507 build; the brief allowed one pull so I kept it.
Not found: the LM Studio stablelm-2-zephyr GGUF (no `.gguf` anywhere, LM Studio dir has no models); no MLX model besides Qwen3-1.7B-4bit.

## Suite (smoke.py)
Streaming OpenAI chat, tools never executed, content-judged (exact args; strings case-insensitive, large_arg verbatim), N=20 trials per case (10 variants cycled, so temp 0 is not 20 copies), max_tokens 3000. Cases: single, choice (1 of 3 tools), typing (integer, enum, boolean, nested object plus array), chain (call tool A, read canned result, call tool B with the returned integer id), parallel (2 calls in one turn), no_tool (tools offered, must answer in text), large_arg (900-char verbatim), json_prompt, json_schema (`response_format`). "Right" = story 2 sense; "valid" = args parse AND match schema. Default temperature: Ollama applies the Modelfile (0.6, top_p .95, top_k 20); llama-server applies 0.8, top_k 40, min_p .05 (`logs/run-20261007T021510Z-nojinja17/env.txt`). Thinking: Ollama `reasoning_effort: none|medium`; llama-server `chat_template_kwargs.enable_thinking`.

## Results: pass counts (of 20 per case)
| config | single | choice | typing | chain | parallel | no_tool | large_arg | json_prompt | json_schema | valid args |
|---|---|---|---|---|---|---|---|---|---|---|
| Ollama 1.7B think OFF t=0 | 20 | 16 | 18 | **0** | 20 | 16 | 12 | 20 | 20 | 134/134 |
| Ollama 1.7B think OFF default | 20 | 16 | 18 | **0** | 20 | 18 | 12 | 20 | 20 | 134/134 |
| Ollama 1.7B think ON t=0 | 20 | 20 | 20 | 16 | 20 | 20 | **4** | 20 | 20 | 160/160 |
| Ollama 1.7B think ON default | 20 | 20 | 18 | 15 | 20 | 15 | **6** | 20 | 20 | 156/160 |
| llama-server 1.7B OFF t=0 | 20 | 18 | 20 | 8 | 20 | 16 | 8 | 20 | 20 | 154/154 |
| llama-server 1.7B OFF default | 20 | 18 | 20 | 7 | 20 | 18 | 9 | 20 | 20 | 154/154 |
| llama-server 1.7B ON default | 20 | 20 | 20 | 14 | 20 | 17 | 15 | 20 | 20 | 160/160 |
| Ollama 4B (always thinking) t=0 | 20 | 20 | 20 | 20 | not run | **14** | not run | not run | not run | 102/102 |
| Ollama 4B (always thinking) default | 20 | 20 | 20 | 20 | not run | **15** | not run | not run | not run | 102/102 |
| llama-server 4B | 16/16 single only (stopped: 16-35 s per call) | | | | | | | | | |

Common-5 (single, choice, typing, chain, no_tool; 100 trials; `logs/common5-*.md`): Ollama 1.7B OFF 0.70/0.72; ON 0.96 (t=0) / 0.88 (default); llama-server 1.7B OFF 0.82/0.83, ON 0.91; **4B 0.94 / 0.95** (95% CI 0.88-0.98). Full tables `logs/summary-all-*.md`.
Failure shapes: 1.7B think-off in chain hallucinates `user_id 123` instead of calling the lookup (0/20, Ollama) or calls both tools at once; 4 of 4 invalid-argument calls (Ollama 1.7B ON default) were schema failures in chain (`{"user_id":{"$lookup_result":1}}`), never unparseable JSON; large_arg copy errors (extra words, one wrapped object) hit the 1.7B hardest; 4B's weak spot is no_tool: it answers "cannot be answered with the functions provided" or calls `calculate` for "capital of France". JSON output was 40/40 for the 1.7B in every config.

## Which settings matter
1. **Thinking: the largest effect** (1.7B common-5 0.70 -> 0.96 on Ollama t=0), at 4x latency (2.3 s -> 8 s per trial; 4B about 37 s per trial, chain 60-110 s at about 11 tok/s).
2. **Engine**: same GGUF, 1.7B OFF: Ollama 0.70 vs llama-server 0.82; ON: 0.88 vs 0.91. Different templates/parsers, so identity must include the engine.
3. **Temperature**: t=0 vs default differs by up to 8 points (1.7B ON Ollama 0.96 vs 0.88), inside noise at n=100 (Wilson +-7), so a verdict near the threshold flaps between runs.
4. **`--jinja`**: in llama-server 0.5.0 it is on by default; `--no-jinja` gives HTTP 500 `tools param requires --jinja flag` for 60/60 calls (explicit failure, run `...nojinja17v2`). A tool-less template (`--chat-template chatml`) gave 0/40 tool calls and `<think>` in content (`...chatml17`): fails correctly. Ollama 0.35.1 ignored a custom tool-less TEMPLATE for qwen3 and kept calling tools (`...notools`), so that negative control is invalid, not a pass of anything.
5. Thinking text can leak into `content` with a 200 status (4B with `none`), only a content judge sees it.

## pi 1.0.3 `-p`, one tool (`write`), "create hello.txt containing hello", 5 trials each (`logs/pi/run-*`)
Ollama 1.7B default thinking 5/5 (16-22 s); Ollama 1.7B `--thinking off` 4/5 (trial 2 stopped with no tool call, file missing; thinking blocks still present, pi's default compat did not disable thinking on Ollama); Ollama 4B 5/5 (45-64 s); llama-server 1.7B default 5/5 (6-9 s); llama-server 1.7B `--thinking off` 5/5 (2-3 s, no thinking blocks via `thinkingFormat: qwen-chat-template`). pi's system prompt is light (202-886 input tokens). This matches raw `single` 20/20 and the one miss matches a raw "no tool call" shape: single-call plumbing works on all; it says nothing about chains. Handful of trials only.

## PROPOSED smoke parameters (evidence above; binomial in `logs/binomial-*.txt`)
- **SMOKE_MIN_PASS (tool roles) PROPOSED 0.90**, valid-args floor kept at 1.0 but defined as schema-valid tool calls, plus a PROPOSED per-behaviour floor 0.70 and **at least 20 calls per behaviour** (SMOKE_CALLS 50 is about 8 per behaviour: for a model whose true rate is 0.95, a 0.95 threshold passes only 54% of runs, 0.90 passes 96%; at true 0.88 a 0.90 threshold passes 44%, i.e. the verdict is a coin flip).
- Smallest model that clears it: by point estimate the 1.7B with thinking ON on llama-server (0.91) or Ollama t=0 (0.96), but it fails at Ollama default temp (0.88 and 4 invalid-arg calls) so it is NOT stable. **qwen3:4b on Ollama clears 0.90 at both temps (0.94, 0.95) with 100% valid args and no behaviour below 0.70**, but parallel, large_arg and JSON are NOT TESTED for it (large_arg was the 1.7B's worst case), and it costs about 30 minutes of smoke time per 50 calls here. Neither clears the specced 0.95 reliably.
- **Chat-only roles (no tool calling)**: a separate chat suite (no_tool answers + JSON) with PROPOSED `SMOKE_MIN_PASS_CHAT` 0.90: the 1.7B scored 0.92-1.00 in all 7 measured configs (`logs/chatonly-*.md`; qwen3:4b no_tool alone 0.70-0.75, JSON not run).
- **First slice**: the 1.7B can serve the plumbing (single-call and JSON work, 20/20; pi single-tool 5/5 on llama-server and Ollama-default), not real work (chain, large_arg, no_tool unreliable). Use it for chat-only roles at 0.90; any tool role needs the 4B (or a better non-thinking instruct build, untested).

## Story 8 behaviour when no node passes for a tool_calling role
Per story 8 as written: `effectiveCandidates` throws `NoEffectiveCandidate{role, excluded:[{node,state,failingCalls,untestedCapabilities}]}`, generation writes nothing, the router keeps its previous table; on a first generation there is no previous table, so nothing starts. The user would see (content per story 8; wording PROPOSED): `error no_effective_candidate: role "coder" requires tool_calling; excluded: mac-m2 [failed, pass 0.72 < SMOKE_MIN_PASS 0.95, failing calls chain#0..#19, ...]. No outputs written.` This is the first-slice trap with the specced parameters.

## Recommended changes
- **Story 2**: pair identity (and the verdict key) must include thinking mode, engine chat-template/renderer setting (`--jinja`, template override), ctx and the sampling parameters actually applied (record the engine default values, since "no temperature declared -> refuse" blocks the default case); raise calls per behaviour to 20 or report the Wilson interval and mark verdicts within it "borderline" rather than pass/fail; add a dependent two-step chain behaviour (calls tool B using tool A's result) because "follow-up consumes result" does not expose the 1.7B's 0/20; large_arg and no_tool with offered tools as separate floors; per-behaviour floors; record thinking leaking into `content`; timeouts must allow thinking models (4B chain trial up to 110 s).
- **Story 8**: add a per-role requirement (`tool_calling` vs chat-only) and exclude capability-scoped (only roles requiring the failed capability), consistent with story 60; add an explicit, recorded owner waiver or "plumbing" tier so a first slice does not deadlock; on no-candidate print the best state per node and the threshold gap.
- **Story 60**: unchanged in design; note a silent-success failure exists (thinking in content, tool template ignored) so judges must stay content-based.
- **First-slice pass condition**: replace "every pair passes at 0.95" with: one tool-role pair (qwen3:4b Ollama, thinking, parameters above, rerun for the 3 untested behaviours) and one chat-only pair (1.7B) pass; or an explicit waiver for plumbing-only runs.

## Not tested
MLX; qwen3:4b parallel/large_arg/JSON and llama-server 4B beyond 16 single trials; non-thinking 4B instruct build; Ollama 0.40/llama.cpp 0.6; other hardware; models above 4B; pi with the 4B off-thinking; a stablelm negative control (not on disk); `reasoning_effort` low/medium differences; larger N than 20 per case. Left behind: Ollama model `s4-notools-qwen3-1.7b` (tiny manifest) and `qwen3:4b` in `~/.ollama/models`.
