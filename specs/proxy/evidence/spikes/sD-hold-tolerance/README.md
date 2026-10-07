# Spike D: hold tolerance of harnesses and SDKs (all FAKES, no real model, no lock)

Gate: does a v1 proxy that HOLDS a request (no byte) up to 250 s survive each client? Verdict: GO-WITH-CHANGES (see Verdict).
Versions: claude 2.1.291 (latest npm 2.1.292), pi 1.0.3 (latest 1.0.4, pinned by project), dsh 0.2.0-rc.2 (latest), opencode-ai 1.18.35 (installed in agents/, latest; brew 1.0.134 NOT used), codex 0.160.1 (installed in agents/, latest; global 0.157.1 NOT used), openai-node 7.30.0, @anthropic-ai/sdk node 0.131.0, openai-python 3.26.0, anthropic-python 1.11.0 (uv run --with, pinned), node v24.15.0 (undici fetch), curl 8.7.1.
Method: `tools/slowfake.mjs` (scripted OpenAI chat, Anthropic Messages, OpenAI Responses; scenario in URL path: mode hold|eh|err|drop|eherr, t, ka ping|comment|none|space, status, fail, ra). It logs every request and every connection close (client give-up time measured server-side). `tools/job.sh` runs one client; `tools/phase.sh` runs a job list in parallel; `tools/analyze.mjs` and `tools/matrix.mjs` summarise. Runs: runs/stage1 (T=700 probes), stage2 (T grid), err (status matrix), stage3/4 (hold 250 then error), stage5 (reruns), stage6/7 (Claude Code settings, Retry-After). Each run dir has env.txt (versions), jobs.txt, jobs/<id>/cmd.txt (exact command), stdout/stderr, fake.jsonl. results/ has the analysis dumps and the grid. Nothing was overwritten.
Defects found in my own harness (disclosed): (1) stage1 fake crashed at about 700 s on the first non-stream early-header response (headers already sent): stage1 results after 700 s are void; its give-up times (all below 700 s) are valid. Fixed, stage2 reran everything with the fixed fake. (2) The `err` phase process died with 4 jobs unfinished (codex drop5/eherr25, opencode eherr25): rerun as stage5. (3) dsh and opencode send TWO requests at session start (title/small-model call); both are held. For opencode, stage3 `fail=1` hit the title call, so stage4 reran it with fail=2.

## Table 1: give-up time by client (hold = no byte; ehnone = 200 head sent, then nothing; ehka = 200 head + keep-alive every 10 s)
| Client | hold (no byte) gives up at | ehnone | ehka | Evidence |
|---|---|---|---|---|
| curl | never (600 s ok) | never | ok | stage2 hold T=600 |
| node fetch (undici) | 301 s (headers timeout) | 301 s (body timeout) | ok to 600 | stage1, stage2 |
| openai-node, anthropic-node (stream and non-stream) | 301 s, then 2 retries (the 600 s SDK timeout is overridden by undici 300 s) | 301 s | ok to 600 (stream); non-stream with leading spaces ok to 600 | stage1, stage2 |
| openai-python, anthropic-python | 600 s (400 ok) | 600 s read timeout | ok to 600 | stage2 |
| pi 1.0.3 | 299 s `Request timed out.` | 301 s | ok (600 ok) | stage1, stage2 |
| DeepSeek Harness 0.2.0-rc.2 | 299 s (`Connection error` after 4 tries) | 299 s | 299 s: NOT extended | stage1, stage2 |
| opencode 1.18.35 | 300 s | 300 s | ok to 400 (to 690 in stage1 until fake crash) | stage1, stage2 |
| Codex 0.160.1 (Responses) | never (600 s ok) | 300 s `idle timeout waiting for SSE` | comment `: keep-alive` does NOT reset (fails 300); SSE event `ping` resets (400 ok) | stage1, stage2 |
| Claude Code 2.1.291 | 360 s (first-byte limit), then retries | 300 s | ok to 550, fails at 600 | stage1, stage2 |
Claude Code settings: `API_TIMEOUT_MS=100000` shortens the hold limit to 100 s; `API_TIMEOUT_MS=1800000` does NOT raise anything (hold still 360 s, early-head still dies at 600 s); `CLAUDE_STREAM_FIRST_BYTE_TIMEOUT_MS` and `CLAUDE_STREAM_IDLE_TIMEOUT_MS` (with or without `CLAUDE_ENABLE_STREAM_WATCHDOG=1`) had NO visible effect in any run (60000 ms values did not fail a 200 s wait). Not understood.
SDK defaults: 600 s timeout (x-stainless-timeout 600), 2 retries (3 attempts), retry delays 0.4 to 1 s, retried statuses 408, 409, 429, 500, 502, 503, 504, 529 and connection errors; not 400.

## Table 2: completed T grid (stage2: ok = completed at first attempt; F@s = first connection closed by the client at s)
anthropic-node-nonstream|eh/space  120:ok  250:ok  300:ok  400:ok  600:ok
anthropic-node-nonstream|hold      15:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:F@301()
anthropic-node-stream|eh/comment   400:ok
anthropic-node-stream|eh/none      250:ok
anthropic-node-stream|eh/ping      120:ok  250:ok  300:ok  400:ok  600:ok
anthropic-node-stream|hold         15:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:F@301()
anthropic-py-nonstream|eh/space    400:ok  600:ok
anthropic-py-nonstream|hold        60:ok  250:ok  400:ok  600:F()
anthropic-py-stream|eh/comment     400:ok
anthropic-py-stream|eh/none        250:ok
anthropic-py-stream|eh/ping        400:ok  600:ok
anthropic-py-stream|hold           60:ok  250:ok  400:ok  600:F()
cc@API_TIMEOUT_MS=100000|hold      200:F@99( | cc api_retry events=2 [nu)
cc@API_TIMEOUT_MS=1800000+CLAUDE_STREAM_FIRST_BYTE_TIMEOUT_MS=1800000|hold 700:F@360( | cc api_retry events=2 [nu)
cc@API_TIMEOUT_MS=1800000|eh/ping  700:F@600( | cc api_retry events=0)
cc@API_TIMEOUT_MS=1800000|hold     450:F@360( | cc api_retry events=1 [nu)
cc@CLAUDE_STREAM_FIRST_BYTE_TIMEOUT_MS=1800000|hold 450:F@360( | cc api_retry events=1 [nu)
cc@CLAUDE_STREAM_FIRST_BYTE_TIMEOUT_MS=60000|hold 200:ok
cc|eh/ping                         120:ok  250:ok  400:ok  550:ok
cc|hold                            30:ok  120:ok  250:ok  300:ok  340:ok  380:F@360( | cc api_retry events=1 [nu)  450:F@360( | cc api_retry events=1 [nu)
codex|eh/comment                   300:F(Reconnecting... 1/5 (stream )
codex|eh/ping                      120:ok  250:ok  400:ok
codex|hold                         60:ok  250:ok  300:ok  400:ok  600:ok
curl-anth-nonstream|eh/space       120:ok  400:ok
curl-anth-nonstream|hold           15:ok  30:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:ok  600:ok
curl-anth-stream|eh/ping           120:ok  400:ok
curl-anth-stream|hold              15:ok  30:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:ok  600:ok
curl-oai-nonstream|eh/space        120:ok  400:ok
curl-oai-nonstream|hold            15:ok  30:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:ok  600:ok
curl-oai-stream|eh/ping            120:ok  400:ok
curl-oai-stream|hold               15:ok  30:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:ok  600:ok
dsh-anth|eh/ping                   250:ok  300:F@299()
dsh-anth|hold                      60:ok  120:ok  250:ok  300:F@299()
dsh-oai|eh/none                    400:F@299()
dsh-oai|eh/ping                    120:ok  250:ok  300:F@299()
dsh-oai|hold                       15:ok  60:ok  120:ok  180:ok  250:ok  300:F@299()
fetch-anth-nonstream|eh/space      120:ok  250:ok  300:ok  400:ok  600:ok
fetch-anth-nonstream|hold          15:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:F@301( TypeError: fetch failed / c)
fetch-anth-stream|eh/comment       400:ok
fetch-anth-stream|eh/ping          120:ok  250:ok  300:ok  400:ok  600:ok
fetch-anth-stream|hold             15:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:F@301( TypeError: fetch failed / c)
fetch-oai-nonstream|eh/space       120:ok  250:ok  300:ok  400:ok  600:ok
fetch-oai-nonstream|hold           15:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:F@301( TypeError: fetch failed / c)
fetch-oai-stream|eh/none           250:ok
fetch-oai-stream|eh/ping           120:ok  250:ok  300:ok  400:ok  600:ok
fetch-oai-stream|hold              15:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:F@301( TypeError: fetch failed / c)
oc-anth|eh/ping                    250:ok  400:ok
oc-anth|hold                       60:ok  120:ok  250:ok  300:F()
oc-oai|eh/ping                     120:ok  250:ok  400:ok
oc-oai|hold                        15:ok  60:ok  120:ok  180:ok  250:ok  300:F@300()
openai-node-nonstream|eh/space     120:ok  250:ok  300:ok  400:ok  600:F()
openai-node-nonstream|hold         15:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:F@301()
openai-node-stream|eh/none         250:ok
openai-node-stream|eh/ping         120:ok  250:ok  300:ok  400:ok  600:ok
openai-node-stream|hold            15:ok  60:ok  120:ok  180:ok  250:ok  300:ok  400:F@301()
openai-py-nonstream|eh/space       400:ok  600:ok
openai-py-nonstream|hold           60:ok  250:ok  400:ok  600:F()
openai-py-stream|eh/none           250:ok
openai-py-stream|eh/ping           400:ok  600:ok
openai-py-stream|hold              60:ok  250:ok  400:ok  600:F()
pi-anth|eh/ping                    250:ok  400:ok
pi-anth|hold                       60:ok  120:ok  250:ok  300:F@299(Request timed out. | pi retr)
pi-oai|eh/ping                     120:ok  250:ok  400:ok  600:ok
pi-oai|hold                        15:ok  60:ok  120:ok  180:ok  250:ok  300:F@299(Request timed out. | pi retr)

Not run in the grid (inferred, labelled): openai-node/fetch/pi/dsh/opencode hold T=600 (fail, because 300 s fails); python SDK T=15/30/180/300; stage1 T=700 probes show the same limits.

## Table 3: retry behaviour at the hold limit (fake answers status after 3 s; fresh runs `err`; attempts = HTTP requests; main result)
Statuses retried: 503, 502, 504, 500, 529 by EVERY client. 429 by every client except Codex (fails at once, `exceeded retry limit`). 408 by all except dsh on the OpenAI path. 409 by SDKs, Claude Code, Codex, opencode; NOT by pi-anthropic or dsh; pi-OpenAI retried it (the body type `server_error` matches pi regex). 400: not retried except pi-OpenAI and opencode-OpenAI (OpenAI error shape).
Total patience when every attempt fails (503, 3 s hold): SDKs 3 attempts, 11-20 s; pi 4 attempts, 27-31 s (gaps 2, 4, 8); dsh 7 attempts, 35-46 s; opencode 9 attempts, 90-108 s; Codex 30 requests, 117-121 s; Claude Code 11 attempts, 212 s (gaps 0.6, 1.2, 2.6, 4.6, 9.8, 17.7, 37, 32, 36, 33 s).
Retry-After: honoured by openai-python, anthropic-python, anthropic-node (120 s waited), openai-node (only below about 60 s: ignored at 120), Codex on 503/529 (30 and 120 s waited; on 429 Codex fails at once), Claude Code (7, 15, 30, 60 s honoured; 90 and 120 s: FAILS AT ONCE, no retry). Ignored by pi, dsh, opencode.
Hold 250 s then status (fail=1, then the retry is answered at once; stage3/4): completed after retry for 503, 529, 429, 504 and 408 for Claude Code, pi (both paths), dsh (both), opencode (both), SDK stream clients (429, 503, 529); Codex completed for 503, 504, 529, 408 and FAILED for 429. dsh-OpenAI failed for 408.

## Table 4: failure after the 200 head (mode eherr: 200 head, pings, then error event at 25 s)
- openai/anthropic SDK stream (node, python): 1 request, no retry, clean `APIError` with the message; non-stream SDK with early head plus spaces: ugly `SyntaxError`/`JSONDecodeError` (body is not JSON).
- pi Anthropic: error event `overloaded_error` RETRIED 3 times (4 x 25 s). pi OpenAI (`data: {"error"}`): not retried, clean message.
- dsh: fails, no retry, message shown (2 requests). opencode: retried (7 to 9 requests), then fails with the message. Codex: 5 reconnects then fails (`stream closed before response.completed`). Claude Code: retried 3 times (2 stream, then non-stream fallback), final text was an ugly JSON parse error (partly an artifact of my fake answering the non-stream fallback with SSE text).
- Drop of the connection at 5 s: every client sees a connection error and retries (SDK 3, pi 4, dsh 7, opencode 9, Codex 30, Claude Code 11); curl and fetch fail at once.

## Verdict: GO-WITH-CHANGES
250 s hold completes at the first attempt for every client tested (T=250 ok in all rows). The common ceiling is 299 s (pi, dsh, opencode, undici-based Node SDKs); it covers hold plus the time until the response head leaves the node, so the budget is `hold_limit + node time to headers <= about 290 s`. Early headers help only some clients (not dsh; Codex only with a real SSE event) and change the error semantics: default must stay "send nothing".
