# Spike 5: pi 1.0.3 vs router restart (refused / 503 / mid-stream / slow first byte)

Versions: pi 1.0.3 (npm latest is 1.0.4; target pinned to 1.0.3 by the project), node v24.15.0, openai SDK 7.19.0, @anthropic-ai/sdk 0.129.0 (bundled with pi). Everything ran against FAKES (fake2.mjs, no real model, no lock). Retry logic also read in source: `pi-ai/dist/utils/retry.js`, `pi-coding-agent/dist/core/agent-session.js` (_prepareRetry), `docs/settings.md`.

Method: `run.sh <name> <oai|anth> '<SCEN json>' <timeout>` starts `fake2.mjs` (scripted: late listen, status for T s counted from the first request, slow first byte, reset, mid-stream drop, hang, hold-in-accept) and runs `pi --mode json --no-session -p "say pong" </dev/null` under `timeout`; timestamps are seconds since launch. Logs: `logs/<name>.sum` (retry timeline), `.pi.txt`, `.fake.jsonl`. Multi-prompt: `rpc-driver.mjs` (pi `--mode rpc` with a session dir, one long-lived process, three prompts, fake killed/restarted between them).

## Mechanism (verified in source and runs)
pi retries at agent level (the provider-level SDK retry is 0). Default `retry.maxRetries=3`, `baseDelayMs=2000`, exponential: gaps 2, 4, 8 s, so 4 attempts in total, last attempt 14.1 s after the first failure. Whether an error is retried is decided by a REGEX on the error text, not by status: matches 429, 500, 502, 503, 504, 520, 524, "connection error/refused", "terminated", "ended without", "stream ended before message_stop", "timed out", "overloaded", "rate limit", "service unavailable", etc. `Retry-After` is IGNORED (provider maxRetries 0). No jitter. Exit code of `-p` on exhaustion with plain text mode: prints the error text (e.g. `Connection error.`) and exits 1; in `--mode json` the agent_end carries stopReason=error.

## Results (all measured)
| # | Case | pi behaviour | Evidence |
|---|---|---|---|
| 1 | Nothing listening (ECONNREFUSED), fake starts at T | Error text `Connection error.`; retried. Attempts at 0, 2.0, 6.0, 14.0 s. Recovered for T=1 (2 s), 5 (6 s), 10 (14 s), 13 (14.6 s); gave up for T=15 and T=20 after 14.3 s with `Connection error.` (exit 1 in text mode, 15.8 s wall) | refuse1/5/10/13/15/20 |
| 2 | 503 for T s then 200 (JSON body naming phase, no Retry-After) | Retried, same 0/2/6/14 s timeline; error text `503: {"message":"router not ready",...}` (body is shown). OK for T=1,5,10,13,14; FAIL for T=14.5, 16, 20 (measured from the first 503) | s503j_T* |
| 2b | 503 plain body / with Retry-After 1, 5, 30, 120 | Identical timeline: Retry-After ignored, even 120 | s503plain, s503ra* |
| 2c | Other statuses (503 body for 5 s then 200) | See status table below | st*_T5 |
| 3 | Connection reset at accept (once / always) | `Connection error.`, retried; once: OK at 2.6 s; always: 4 attempts then fail at 14.9 s | reset_once/always |
| 3b | Mid-stream: partial text then socket destroyed / clean close without terminator | error `terminated` / `Stream ended without finish_reason` (Anthropic: `Anthropic stream ended before message_stop`); RETRIED; partial text DISCARDED: the retry request carries `[system,user]` only, the partial is not in the next request context (fake checked `hasPartial=false`, rpcC). Retries exhausted -> final message is an error message with the partial text kept in the failed message | mid_*, midclose_*, rpcC |
| 4 | Slow first byte 5 / 20 / 60 s | NO retry, waits, succeeds (OK at 5.5, 20.5, 60.5 s). Hold of 120 s in accept also succeeds at 120.1 s (hold120). A true hang: pi gives up at 300.6 s with `Request timed out.` and retries once (2 s later) | slow*, hold120, hang_default |
| 5 | Same session after the endpoint returns | YES. RPC mode, one pi process (pid unchanged), session file persisted. rpcA: router killed between prompts 1 and 2, back after 5 s: prompt 2 succeeded on retry #2, prompt 3 normal, history intact (failed attempts are omitted from the request: roles `system,user,assistant,user`). rpcB: router back after 25 s (retries exhausted at 14.6 s, prompt 2 ended in error), prompt 3 sent after the router was up worked in the same process; the failed prompt 2 stays in the context as an unanswered user message (roles `system,user,assistant,user,user`) | rpcA/B/C |
| 6 | Anthropic Messages path (`anthropic-messages`) | Same: refused T=5 OK at 6.3 s, T=20 fails at 14.9 s; 503 T=5 OK, T=16 fails after 4 attempts; mid-stream and clean-close retried and partial discarded; slow 20 s OK. Error text `503 {"error":{...},"phase":"replaying_journal"}` | a_* |

Status table (retry decided by regex on `<status> <body>` text; `-` = NOT retried, final error shown at once):
| Status | OpenAI path | Anthropic path | Note |
|---|---|---|---|
| 503 | retried | retried | |
| 502, 504 | retried | not run | |
| 500 | retried | not run | |
| 429 | retried (even with `Retry-After`; no quota words) | not run | quota/billing words in body make it non-retryable |
| 529 | NOT retried | NOT retried | regex has 520 and 524 but not 529; only "overloaded" text would match; my body had none |
| 409 | NOT retried | NOT retried | so `lease_ended`/`lease_not_yours` fail at once: good |
| 400, 401, 404, 422 | NOT retried | 400 not retried | |
Caveat: classification is by TEXT: a 409/400 whose body contains words like "503", "timeout", "rate limit", "overloaded", "terminated" would be retried. Keep router refusal bodies free of those words unless a retry is wanted.

## Settings that widen the window (verified)
`retry.maxRetries` in the pi settings file (global `PI_CODING_AGENT_DIR/settings.json`, or project): with 6 retries gaps are 2, 4, 8, 16, 32, 60(cap `maxAgentDelayMs`): refused T=20 recovered at 30.5 s; 503 T=25 recovered at 30.5 s; 503 T=60 recovered at 62.5 s. Window with N retries = sum of gaps (3: 14 s, 4: 30 s, 5: 62 s, 6: 122 s).

## Verdict
**GO-WITH-CHANGES** for "listen early, 503 until ready". It works as designed, but only inside pi's default retry window, which is 14.1 s measured from the first failed request (attempts at about 0, 2, 6, 14 s). Maximum restart-to-ready that default pi tolerates: **about 14 s from the moment the agent's call first fails, counting the supervisor relaunch delay** (13 s OK, 14.0 s OK, 14.5 s fail on 503; ECONNREFUSED: 13 s OK, 15 s fail). The call that happens to fail first is typically AFTER the kill, so effective tolerance is ~14 s minus the time between router death and the agent's next call (worst case only a few seconds of margin if the call was in flight: a streamed response in flight dies as `terminated` and starts the same clock).

## Router start sequence: what is best (numbers)
1. Refuse connections (not listening): worst. Same 14 s window as 503 but error text `Connection error.` carries no phase, and each retry just wastes one of 3. Same recovery curve as 503.
2. Listen early and answer 503: equal window (14.1 s), but the body (`router_starting`, phase) is visible in the error, and 409/400 style non-retried statuses stay unaffected. Retry-After is useless (ignored).
3. HOLD requests (listening, connection accepted/queued, response sent after Ready): BEST. pi does not retry or time out before 300 s: measured 5, 20, 60 and 120 s all succeed at the first attempt with no retry budget consumed; a true hang is only cut at 300.6 s. Macro check: a bound-but-not-accepting socket completes connect() and send() on macOS in 7 ms (python test, so the story-10 ASSUMPTION about the listen queue holds; the listen backlog is finite, so the held count is bounded by it, not tested at scale). Holding requests during replay therefore gives a ready-wait tolerance of ~300 s instead of ~14 s.
Recommendation: for the AGENT-facing socket hold (accept and park, or leave in the kernel backlog) until Ready, with a router-side cap well under 300 s (say 120 s) after which answer 503 `starting` (then pi's 14 s retry gets a few more chances). Keep 503 on the ADMIN socket (health) as designed. If holding is rejected, raise `retry.maxRetries` to 5 (62 s) in the settings used by every pi that talks to the router and note that this multiplies backoff for ALL transient failures too.

## Design consequences
- Story 10 `proxy.start.ready`: the contract says "no model request answered before Ready" and the target says "well below the 14 s window". Change to: agent socket parks requests (does not refuse, does not 503) until Ready, up to a cap (PROPOSED 120 s, must stay below pi's 300 s idle timeout `httpIdleTimeoutMs`); then 503 `starting`. Keep RESTART_READY_BUDGET_S plus supervisor delay below 14 s only as the fallback requirement when held requests time out. Add test: pi `-p` run through a router held in Replaying for 20 s completes with no retry event.
- Story 10: a router that dies mid-stream causes pi `terminated` and a retry with the partial discarded: safe, no router action needed. Document that the 14 s window starts at the first failed call, so supervisor relaunch delay counts.
- Story 32: replay cost must be measured; with hold, any replay up to the router cap is invisible to pi; without hold, replay + bind + relaunch must be < ~10 s to keep margin.
- Error bodies: never put 503/timeout/rate limit/overloaded/terminated words in non-retryable refusals (`lease_ended`, `lease_not_yours` 409, `bound_node_unavailable` 503 is RETRIED by pi: a pinned node that stays down gets 3 retries then the agent sees the error; acceptable but note it).
- 503 `bound_node_unavailable` and `no_capacity` style refusals are retried by pi for 14 s automatically: if those must fail at once, use a status pi does not retry (409/400) or accept the delay.
- Config: if hold is not adopted, the pi settings for Legatus-launched agents must set `retry.maxRetries` (story for pi integration/install).

## Not tested / caveats
- Real model, tool-calling turns, and pi-subagents (subagent child processes) not run: the retry is in pi core, but a subagent whose retries are exhausted fails its task: unverified how pi-subagents reports it.
- Kernel backlog size limit, held connections at scale, a held request when pi's idle timeout is changed.
- Interactive TUI mode (RPC and `-p` used). Retry-After on 429/503 was ignored in my runs on the OpenAI path and 503 on Anthropic path only partly (503 T5 run had none).
- Anthropic 429/500/502/504 and 529 with "overloaded" text not run.
- Phase times are measured from the first request seen by the fake, pi start-up jitter was 0.2-2.4 s.
