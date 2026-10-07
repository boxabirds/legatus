# 04 Admission control and queueing

Status: draft for review. Language: STE-style (not STE-compliant). Written 2026-10-07.

This file defines the cap of each node, the protected window, the hold queue and the answer at the hold limit. File [03](03-affinity-and-keys.md) defines keys and the table. Labels: PROVEN, PROPOSED, ASSUMPTION, NOT TESTED.

## 1. Purpose

A node serves a few requests well and many requests badly. Spike 3 shows that a request beyond the useful concurrency only queues in the engine or slows every other request. The proxy therefore admits a request to a node only when the node has room. It holds the request in the proxy otherwise.

Admission also protects the warm cache. A conversation that waits for its next turn keeps a seat on its node for a fixed time. A new conversation cannot take that seat during that time.

The cache capacity of a node is not always its slot count. Spike 7 measured that llama-server keeps more conversations warm than it has slots. The cause is `--cache-ram`, which saves the state of an idle slot in host memory (section 4.5). The seat count therefore follows `warm_capacity`, not the cap.

## 2. Terms used here

| Term | Meaning |
|---|---|
| Cap | The largest count of requests that run on one node at the same time |
| Warm capacity | The count of conversations that a node keeps warm at one time. The registry or the calibration probe sets it as `warm_capacity`. The default is the declared slot count |
| Seat | A reservation of one warm conversation on a node. A node has `warm_capacity` seats |
| Idle | A conversation is idle from the end of its last request until its next request starts |
| Protected window | The idle time during which a seat belongs to its conversation |
| Probation window | A shorter window for a conversation that has not shown that it returns |
| Hold queue | The ordered list of requests that wait in the proxy for a seat |
| Hold limit | The longest time a request waits in the hold queue |
| Pool | The nodes of one alias |

## 3. Caps

### 3.1 Where the number comes from

A cap is the smaller of the declared concurrency and the measured useful concurrency. A measured value lowers the declared value and never raises it. This follows spike 3 ([s3](evidence/spikes/s3-slots-README.md)).

Spike 3 ran a real Qwen3 1.7B model on an Apple M2 with 16 GB. Absolute speeds changed by a factor of 2 between equal runs, so only ratios and shapes count. The gain from concurrency was 1.3 times for prompt-bound work and at most 1.8 times for decode-bound work (PROVEN, s3 finding 5). Nobody tested a larger model or other hardware (NOT TESTED).

| Engine and setting | Declared | Measured useful concurrency | Load signal | Source of the cap |
|---|---|---|---|---|
| Ollama 0.35.1, default | 1 | 1 | None. `/api/ps` has no busy or queue field | The registry must declare `OLLAMA_NUM_PARALLEL`. The proxy cannot read it |
| Ollama, `OLLAMA_NUM_PARALLEL` 2 or 4 | 2 or 4 | 2 or 4 | None | Same |
| llama-server 0.5.0, default | 4 (unified KV) | 4, gain 1.4 to 1.8 times | `/metrics`: `requests_processing`, `requests_deferred`. Exact | `/props` field `total_slots` (PROVEN) |
| llama-server, `--parallel` 1, 2 or 4 | 1, 2 or 4 | 1, 2 or 4 | Same | `/props` field `total_slots` |
| mlx_lm 0.32.0 | 1 | 1 by throughput. Eight requests run at once | None | The default 1. A higher cap adds latency only |
| vLLM, SGLang | Not measured | Not measured | `/metrics` | Declared in the registry. ASSUMPTION: the engine batch limit (`--max-num-seqs` or `--max-running-requests`) is the ceiling. NOT TESTED |
| Hosted node | None | None | None | No cap in v1. The limits of the provider apply (DEC-050, decided by owner) |

All rows except the last two are PROVEN in [s3](evidence/spikes/s3-slots-README.md). The table repeats the measured values and does not add numbers.

### 3.2 Facts that shape the cap

- Ollama default parallelism is 1. A queued request gets no response header until service starts. A queued request waited 2.8 s to 39.9 s at 8 requests (PROVEN, s3 finding 2).
- llama-server sends headers early even when its queue holds the request. It counts the queue in `requests_deferred` (PROVEN, s3 finding 3).
- mlx_lm admits all requests. Aggregate speed stays at 51 to 58 tokens per second for 1 to 8 requests. Each request gets slower (PROVEN, s3 finding 4).
- Prefill is one shared pool. A second long prompt delays the first by 2.1 to 2.4 times with no gain overall (PROVEN, s3 finding 7).
- With `--parallel N`, each slot gets `-c` divided by N. A node set to `-np 4 -c 16384` refused a prompt of 6850 tokens (PROVEN, s3 finding 3). The context check in [02](02-architecture.md) must use the slot context.

### 3.3 How a cap changes (PROPOSED)

The registry declares the slots. The proxy reads `total_slots` from llama-server at join and on change. A calibration probe measures the knee: the smallest concurrency where the aggregate gain per doubling falls below 1.15 times. The probe must use a decode-bound request. A prefill-bound probe picks wrong values, because its gains are within the run-to-run noise (PROVEN, s3 design consequences). Files [05](05-engine-behaviour.md) and [07](07-registry-and-configuration.md) define calibration.

The cap of a node is `min(declared, measured)`. The proxy must not raise a cap above the declared value.

## 4. Seats and the protected window

### 4.1 The model

A node has `warm_capacity` seats. The default is the declared slot count. A seat holds one conversation. Running requests never exceed the cap, and the cap never exceeds `warm_capacity`, so every running request holds a seat. Section 4.5 says how `warm_capacity` rises above the slot count.

A conversation holds a seat in two states.

| State | Seat | Counts toward the cap |
|---|---|---|
| Running (a request is in flight) | Held | Yes |
| Idle, less than its window since the last request ended | Held and protected | No |
| Idle, at least its window | Held but reclaimable | No |

A reclaimable seat stays with its conversation until another request needs it. The proxy reclaims it lazily. If the conversation returns before that, it keeps the warm state.

### 4.2 Window length (PROPOSED)

| Parameter | Default | Range | Meaning |
|---|---|---|---|
| `window` | 180 s | 10 to 3600 s | Protected window for a mature conversation |
| `probation_window` | 30 s | 0 to `window` | Window for a conversation with fewer than `mature_turns` successful requests, and for every WEAK key |
| `mature_turns` | 2 | 1 to 10 | Successful requests that make a conversation mature (DEC-067, pending owner confirmation) |

The default `window` is 180 s (PROPOSED, no measurement). The anchors are the hosted idle windows of 5, 10 and 30 minutes ([R3](evidence/research/r3-sglang-routing-20261007T022127Z.md)). No source gives the right idle window for a local engine (R3 section 4). The best value depends on how many conversations a node cache can keep. Spike 7 measured two warm conversations on one llama-server slot (section 4.5). It did not measure the idle time after which the engine loses a cache entry, so the value stays PROPOSED.

The probation window is an addition of this file. It exists because a conversation of one request has little to lose. Its prefix is a system prompt and one message. A child agent that runs once never returns. Without a probation window, a finished child holds its seat for 180 s (see section 7).

### 4.3 Seat choice for a new conversation

The proxy gives a seat to a new conversation in this order. It uses the first rule that applies.

1. A node with a free seat. If more than one node qualifies, take the node with the fewest held seats. If still equal, take the node with the fewest running requests. If still equal, take the node that a rendezvous hash of the conversation key ranks first. A rendezvous hash gives the same answer after a restart, so the conversation returns to the node that still holds its cache.
2. A node with a reclaimable seat. Take the seat of the conversation with the longest idle time. If more than one node qualifies, take the longest idle time over all nodes.
3. No seat. The proxy holds the request.

A node with affinity off has no seats. The proxy only checks its cap there. See [03](03-affinity-and-keys.md) section 9.

Rule 1 spreads conversations before it fills a node. This keeps the speed of each request high. The speed of a request falls as the count of running requests rises (s3 findings 4 and 6).

### 4.4 Seat choice for a returning conversation

A request with a table entry goes to the node of its entry. If the conversation still holds its seat, it runs when the cap allows. If another conversation took the seat, the request needs a seat on the same node. The proxy applies rule 1 and rule 2 on that node only. If no seat exists, the request waits in the hold queue for that node. This is strict stickiness ([03](03-affinity-and-keys.md) section 6).

A request that starts a new turn while its conversation has `in_flight` of 1 or more takes no new seat. It counts against the cap only.

### 4.5 Warm capacity

The warm capacity is the count of seats. Spike 7 shows that it is not the slot count for every engine. The test used llama-server b11459 and brew 0.5.0 (b11146) on Qwen3.5-2B Q4_K_M and an Apple M2 with 16 GB ([s7](evidence/spikes/s7-hybrid-cache/README.md)).

- With `-np 1` and the default `--cache-ram` (8192 MiB), two interleaved conversations (A, B, A, B, 6k tokens each) both stayed warm. Each turn after the first recomputed 201 tokens. PROVEN, three of three repetitions.
- With `-np 2` the same two conversations also stayed warm. PROVEN.
- With `-np 1 --cache-ram 0` every turn was cold, about 23 s. PROVEN.
- The cause is that llama-server saves the whole state of an idle slot in host memory. A 6k hybrid conversation took about 129 MiB (91 MiB state and two checkpoints of about 19 MiB). A swap took about 30 ms. PROVEN for this model.
- Not tested: more than two conversations, eviction when the 8 GiB fills, models other than Qwen3.5-2B and the dense control, and streaming.

The proxy therefore sets the seats of a node by the engine type.

| Node | Seats | Source |
|---|---|---|
| Engine with one sequence for each slot and no host cache (Ollama default, mlx_lm.server) | The declared slot count | PROPOSED. ASSUMPTION for mlx_lm: its 10-entry prompt cache (source only) is not measured |
| llama-server with `--cache-ram` 0 | The slot count | PROVEN (s7) |
| llama-server with `--cache-ram` above 0 | The slot count, until the registry or the probe gives a larger value | PROPOSED |
| Any engine, calibrated | The count that the swap test of [05](05-engine-behaviour.md) section 12 kept warm, at most the registry value | PROPOSED |

For llama-server the upper bound by arithmetic is the slot count plus the `--cache-ram` budget divided by the state size of one conversation. The 8192 MiB budget and 129 MiB give about 63 conversations of the tested size. This is arithmetic, not a measurement, and the state size grows with the history. The model is a small Qwen3.5 2B. The rule transfers to Qwen 3.8 Flash-Next and 27B as ASSUMPTION. The sizes and the speeds are NOT TRANSFERABLE: use spike B on the owner machines.

A larger warm capacity lets the proxy keep more conversations on one node and raises the load on that node. The cap still limits the running requests. A conversation that loses its seat keeps its cache until the engine evicts it. The proxy cannot see that event, so it reads the next turn (`timings.cache_n`, [03](03-affinity-and-keys.md)).

## 5. The hold queue

### 5.1 Structure

Each pool has one hold queue. A request enters the queue when no node can admit it. The queue keeps two facts per request: arrival time and key.

Both new requests and requests with a table entry wait in the same queue. When a seat or a cap place frees on a node, the proxy scans the queue in order. It admits the first request that can run on that node. A new request can run on any node of the pool. A request with a table entry can run only on the node of its entry.

A request that waits needs no engine resource. The proxy does not send it to a node. It sends no response bytes either, so the status code stays free until the hold limit (see section 8).

### 5.2 Parameters (PROPOSED)

| Parameter | Default | Range | Meaning |
|---|---|---|---|
| `hold_limit` | 250 s | `window` + 1 to 3600 s | Longest wait in the queue. Start-up warning above 290 s (section 8.2) |
| `max_held` | 64 requests per pool | 1 to 1024 | Largest queue length |

The proxy must refuse at once when the queue is full. The number 64 has the label PROPOSED. The status of the refusal is the hold-limit status of section 8.3. Open WebUI can send 20 parallel sub-agent requests ([Open WebUI notes](../../docs/horizon/openwebui.md), documented, NOT TESTED). The value 64 leaves room for three users.

The proxy must not start with a `hold_limit` that is not larger than `window`. See section 6.

### 5.3 Harness disconnect

When the harness closes the connection, the proxy removes the request from the queue at once. It then frees the position.

### 5.4 Prior art for the queue

| Project | Hold behaviour | Lesson |
|---|---|---|
| HAProxy | `maxconn` per server queues. `timeout queue` bounds the wait. Then 503 | The same semantics as this file. A request that chose a server by hash waits on that server ([R5](evidence/research/r5-gateways-20261007T055410Z.md), documented) |
| Paddler | `BufferedRequestManager` holds requests with a timeout and a `max_buffered_requests` limit | The best hold queue found. It depends on Paddler agents ([R6](evidence/research/r6-routers-20261007T055357Z.md), documented) |
| SGLang gateway | Global token bucket, FIFO, 429 when full, 408 on queue timeout | Global, not per node |
| vLLM router, program scheduling | A request pool holds requests when a backend lacks capacity. KV protecting resume order | A design reference. It uses token capacity |
| LiteLLM | A full slot raises a 429 at once instead of waiting | Not a hold queue |
| llama-swap | `concurrencyLimit` returns 429 at once | Same |
| Ollama | `OLLAMA_MAX_QUEUE` 512, then 503 | The engine queue. The proxy keeps requests out of it |

## 6. Parent and child deadlock

### 6.1 The case

pi-extensible-workflows issue 264 describes a deadlock. A parent holds a concurrency permit and waits for children. The children need permits. The children collect results and break the limit, so the parent waits for ever. The fix in that project is that a child does not take its own permit, because the parent holds one ([R3](evidence/research/r3-sglang-routing-20261007T022127Z.md), documented, application level).

The same shape appears in the proxy. A parent conversation ended its request with a tool call that starts children. It is idle and holds a protected seat.

The children are new conversations. If the window protects every seat of the pool, the children wait for a seat. The parent waits for the children. Nobody makes progress until a window ends.

A pi compaction call has the same shape. It carries no session header, so it is a new conversation ([03](03-affinity-and-keys.md) section 8). Its parent conversation is idle and waits for it.

### 6.2 How the window resolves it

A protected seat becomes reclaimable after its window. No conversation holds a seat for ever. Every held request therefore gets a seat after at most one window if all seats are idle. The invariant is:

If all seats of a pool are idle, a held request gets a seat after at most `window` seconds.

The hold limit must be larger than the window, so that the request does not time out first. This is a configuration check (PRX-ADM-014).

If a running request holds some seats, the wait can be longer. A running request ends when the engine finishes it. The proxy has no timeout for a running request in v1 ([02](02-architecture.md)).

### 6.3 What the resolution costs

The parent can lose its warm seat. The parent runs again after its children and can need a cold prefill. For a mature conversation of 60k tokens this costs 47 s to 230 s (PROVEN, [R4](evidence/research/r4-local-engines-20261007T022141Z.md)).

The wait of the child is up to `window` seconds. With the default this is 180 s. This is long, and a child in a pi workflow can wait for the full time if every seat holds a mature idle conversation.

### 6.4 Why the probation window exists

Without it, a one-shot child holds its seat for a full window after it ends. The next child must wait for it. The pool then admits one child generation per window. With the probation window, a child that has one successful request holds its seat for 30 s (PROPOSED).

The probation window also lets a parent in its first turn give up its seat after 30 s. The loss is small, because its prefix is short.

### 6.5 Options not chosen

| Option | Effect | Status |
|---|---|---|
| Family release: a request with the same Claude Code session id and another agent id takes the idle seat of a parent at once | Children start at once. The parent loses its warm state, and the parent holds the longest prefix | OPEN. NOT TESTED. Not in v1 |
| Overcommit: allow `cap + 1` running requests for a request with a small `max_tokens` | Side requests start at once. Prefill contention rises | OPEN. NOT TESTED |
| A shorter `window` | Faster resolution. More evictions of conversations that return | Set by the parameter |
| Harness hint (a header that says "child of") | Exact. Needs a change in the harness | Parked. See [16](16-parked-capability-manager.md) |

## 7. Worked examples

All examples use nodes A and B. Each has cap 2 and 2 seats. The window is 180 s. The probation window is 30 s. The hold limit is 250 s.

Every request runs for 20 s unless the table says otherwise. Each node has `warm_capacity` equal to its slot count of 2. All conversations use a STRONG key.

A conversation is mature after two successful requests. Times are in seconds. These are design examples, not measurements.

### 7.1 Three conversations on two nodes

| Time | Event | Decision | A seats | B seats |
|---|---|---|---|---|
| 0 | S1 request 1, new key | Both nodes have a free seat and zero held seats. Registry order gives A | S1 (running) | none |
| 5 | S2 request 1, new key | A holds 1 seat, B holds 0. Choose B | S1 (running) | S2 (running) |
| 10 | S3 request 1, new key | A and B hold 1 seat each, and 1 request runs on each. Registry order gives A | S1, S3 (both running) | S2 (running) |
| 20 | S1 request 1 ends | S1 is idle from 20. `done_count` is 1 | S1 (idle), S3 | S2 |
| 60 | S1 request 2 | Table hit on A. S1 holds its seat. Idle 40 s, window not ended. Admit | S1 (running), S3 (idle) | S2 (idle) |
| 70 | S3 request 2 | Table hit on A. S1 and S3 run: 2 of cap 2. Admit | S1, S3 (running) | S2 (idle) |
| 75 | S2 request 2 | Table hit on B. Admit | S1, S3 (running) | S2 (running) |

No request waited. Each conversation stayed on its node. Node A runs at its cap while node B has a free seat. This is the cost of strict stickiness: S1 and S3 cannot move to B.

### 7.2 A fifth conversation on a saturated pool

This continues example 7.1. S1 request 2 ends at 80, S3 request 2 ends at 90, S2 request 2 ends at 95.

| Time | Event | Decision | State after |
|---|---|---|---|
| 100 | S4 request 1, new key | A has no free seat. B has one. Admit on B | A: S1, S3. B: S2, S4 |
| 120 | S4 request 1 ends. S5 request 1, new key | No free seat. Idle times: S1 40 s, S3 30 s, S2 25 s, S4 0 s. All below 180 s. Hold. Deadline is 370 | S5 in the hold queue |
| 260 | S1 idle reaches 180 s (ended at 80) | S1 is the longest idle. Its seat is reclaimable. S5 takes it on A. S5 waited 140 s | A: S5 (running), S3. B: S2, S4 |
| 280 | S5 request 1 ends | S5 is idle from 280. `done_count` is 1, so the probation window of 30 s applies | A: S5 (idle), S3 |
| 290 | S1 request 3 returns | Table hit on A. S1 has no seat. A holds S5 (idle 10 s) and S3 (idle 200 s, over its window). S1 takes the seat of S3. Admit | A: S1 (running), S5 |

S5 waited 140 s, less than the hold limit of 250 s. S1 returned to a node where its seat was gone. The engine can still hold its cache. The proxy does not know it. If the node evicted the cache, S1 pays a cold prefill.

### 7.3 The hold limit is reached

Four conversations hold all seats. Each sends a request every 60 s, so no seat is idle for 180 s.

| Time | Event | Decision |
|---|---|---|
| 0 | S5 request, no seat, no reclaimable seat | Hold. Deadline is 250 |
| 250 | Hold limit reached | The proxy answers 503 with `Retry-After` 30 and a body that names the cause (DEC-064) |
| 252 | pi retries (2 s gap) | The retry is a new request. It enters the queue behind every request that arrived before 252 |
| 256 | No seat yet | Hold. The hold limit restarts at 252 |

### 7.4 Parent and children

One node, cap 2, 2 seats. P and Q are mature conversations. P has a tool call that starts two children C1 and C2.

| Time | Event | Decision | Seats |
|---|---|---|---|
| 0 | P and Q requests end | Both are idle from 0 | P (idle), Q (idle) |
| 5 | C1 and C2 arrive, new keys | No free seat. P and Q are idle 5 s. Hold both. Deadline is 255 | hold: C1, C2 |
| 180 | P and Q idle reach 180 s | Both seats are reclaimable. C1 takes the first, C2 the second. Each waited 175 s | C1, C2 (running) |
| 210 | C1 and C2 end after 30 s | `done_count` is 1. The probation window of 30 s applies | C1, C2 (idle) |
| 215 | P returns with the results | P has no seat. C1 and C2 are idle 5 s, inside probation. Hold | hold: P |
| 240 | C1 and C2 reach 30 s idle | P takes a reclaimable seat. P waited 25 s. P pays a cold prefill | P (running), C2 or C1 |

The children waited 175 s, close to the hold limit. A `window` of 250 s or more makes the hold end first, so PRX-ADM-014 refuses that configuration. The check gives no margin for the time of the engine.

## 8. The hold limit

[Spike D](evidence/spikes/sD-hold-tolerance/README.md) tested the hold with fake servers on 2026-10-07. A fake server held each request with no byte, then answered. No real model ran. The table in section 8.1 gives the harness versions.

### 8.1 What the harness tolerates (PROVEN, spike D)

A hold of 250 s with no byte completed at the first attempt in every harness tested. The table gives the time at which each harness gives up when it gets no byte.

| Harness | Gives up at, with no byte | Note |
|---|---|---|
| curl 8.7.1 | Never (tested to 600 s) | |
| Codex 0.160.1 | Never (tested to 600 s) | After an early head with no event it gives up at 300 s |
| pi 1.0.3 | 299 s | Message `Request timed out.` |
| DeepSeek Harness 0.2.0-rc.2 | 299 s | A keep-alive does not extend it |
| opencode 1.18.35 | 300 s | |
| openai-node 7.30.0, anthropic-node 0.131.0, Node v24.15.0 `fetch` (undici) | 301 s | Undici overrides the 600 s of the SDK. The SDK then retries twice |
| Claude Code 2.1.291 | 360 s | The first-byte limit. It retries after that |
| openai-python 3.26.0, anthropic-python 1.11.0 | 600 s | A hold of 400 s completed |

Claude Code settings: `API_TIMEOUT_MS=100000` shortened the limit to 100 s. `API_TIMEOUT_MS=1800000` did not raise it. The stream timeout settings had no visible effect in any run. The cause is not understood and NOT TESTED further.

The 300 s of pi and the 299 s of DeepSeek Harness count the time until the first byte. Spike D closed the earlier ASSUMPTION about an idle timer for the case with no byte. [Spike s5](evidence/spikes/s5-pi-restart-README.md) measured pi holds of 5 to 120 s and a true hang that ends at 300.6 s.

The stage 1 runs of spike D crashed the fake server at about 700 s. Its results above 700 s are void. Its give-up times below 700 s are valid. Stage 2 repeated every case with a fixed fake.

### 8.2 Why 250 s

The lowest give-up time is 299 s. The limit that binds is this sum.

```
hold_limit + t_head <= 290 s
```

`t_head` is the time from admission until the response head leaves the node. The harness counts the wait for the head as time with no byte. The 290 s keeps a margin of about 9 s under 299 s (PROPOSED).

A cold prefill counts in `t_head` only for an engine that sends no head until it finishes. Ollama sends the head late, at the time to first token (PROVEN, [s3](evidence/spikes/s3-slots-README.md)). llama-server sends the head early. A cold prefill then does not count (PROVEN, s3). mlx_lm sends the head at once (PROVEN, s3).

The default hold limit of 250 s leaves 40 s for `t_head`. A cold prefill of 60k tokens takes 47 s on gufo and about 230 s on llama.cpp (PROVEN, [R4](evidence/research/r4-local-engines-20261007T022141Z.md)). For an engine that sends no early head, the hold limit must be lower. The real `t_head` of each engine under load is NOT TESTED beyond s3.

`hold_limit` can be set per alias (PROPOSED). An operator who serves only harnesses with a higher give-up time (Claude Code, Codex, Python SDKs) can raise it. That case is NOT TESTED.

### 8.3 What the proxy answers

At the hold limit the proxy sends a refusal. The status decides what the harness does. Spike D measured every status on every harness with a hold of 3 s and with a hold of 250 s.

| Status | Retried by | Not retried by |
|---|---|---|
| 503, 502, 504, 500, 529 | Every harness tested | None |
| 429 | Every harness tested except Codex | Codex fails at once with `exceeded retry limit` |
| 408 | Every harness tested except one | DeepSeek Harness on the OpenAI path |
| 409 | The SDKs, Claude Code, Codex, opencode and pi on the OpenAI path | pi on the Messages path, DeepSeek Harness |
| 400 | pi and opencode on the OpenAI path, when the body text matches their pattern | Every other harness |

pi decides on the text of the status and the body ([s5](evidence/spikes/s5-pi-restart-README.md)). pi retried a 409 on the OpenAI path. The body type `server_error` matches its pattern. opencode matches error text in the same way, also on 400 and 409.

The default is status 503 (PROVEN as retried by every harness, DEC-064). Avoid 429, because Codex fails at once. [06](06-protocols-and-harnesses.md) defines the exact error object for each protocol. The body shape is:

```json
{"error":{"type":"capacity_wait_expired","message":"No node had room within the hold limit."}}
```

The body must not contain the words "quota", "billing" or "rate limit". It must not contain `server_error`, `overloaded`, `timeout` or `terminated` in a refusal that must not be retried. pi and opencode retry on those words.

`Retry-After` handling differs by harness.

| Harness | Reads `Retry-After` |
|---|---|
| openai-python, anthropic-python, anthropic-node | Yes. They waited 120 s |
| openai-node | Only below about 60 s. It ignored 120 s |
| Codex | Yes on 503 and 529 (30 s and 120 s waited). On 429 it fails at once |
| Claude Code | Yes up to 60 s (7, 15, 30 and 60 s). At 90 s and 120 s it fails at once with no retry |
| pi, DeepSeek Harness, opencode | No. They ignore it |

The proxy therefore sends no `Retry-After` or a value of 30 s or less. A value of 90 s or more makes Claude Code fail at once. The default is 30 s (PROPOSED, DEC-064, pending owner confirmation).

The registry parameter `hold_limit_status` takes 503 (default), 504 or 529. The registry refuses 429. A refusal that must stop at once uses 400 or 404 and is not a hold-limit answer.

### 8.4 Retry patience and the worst-case wait

The proxy holds each retry again, because each retry is a new request. The worst-case patience of a harness is therefore its number of attempts times the hold limit, plus the gaps. The table gives the attempts and the time of a harness when every attempt fails fast (503 after 3 s, PROVEN, spike D). The last column uses the hold limit of 250 s (PROPOSED).

| Harness | Attempts | Time when each attempt fails fast | Worst case with a hold of 250 s |
|---|---|---|---|
| SDKs (Node and Python) | 3 | About 11 to 20 s | About 750 s |
| pi 1.0.3 | 4 | About 27 to 31 s (gaps 2, 4, 8 s) | About 1014 s |
| DeepSeek Harness | 7 | About 35 to 46 s | About 1750 s |
| opencode | 9 | About 90 to 108 s | About 2250 s |
| Codex | 30 requests | About 117 to 121 s | About 7500 s |
| Claude Code | 11 | About 212 s (gaps 0.6 to 37 s) | About 2750 s |

A retry goes to the end of the queue and loses its place. The proxy does not remember the arrival time of a refused request. The owner dropped that rule on 2026-10-07 (DEC-061). The retry gaps of Claude Code and opencode reach 37 s, so the rule does not help a long wait. The rule stays parked for version 2 with the trigger "users complain of unfair waits". See [14](14-open-questions-and-risks.md).

Every harness retries a dropped connection. The attempts are the same as in the table. curl and `fetch` fail at once.

### 8.5 Alternative rejected: an early head and keep-alive

The proxy can send the response head and keep-alive bytes during the hold. Spike D tested this with a 200 head and a ping every 10 s. Decision DEC-064 rejects it for v1.

| Fact | Detail |
|---|---|
| It helps | pi, opencode, SDK stream calls and Node `fetch`, which then wait for 600 s or more |
| It does not help | DeepSeek Harness. It gave up at 299 s with an early head and pings |
| Codex | A comment line such as `: keep-alive` did not reset its 300 s idle timer. A real SSE event `ping` did |
| Claude Code | It waited to 550 s with an early head and pings and failed at 600 s |
| After the 200 | The status cannot change. Only an error event can end the request |
| The error event | Pi, opencode and Claude Code retry an Anthropic `overloaded_error`. An OpenAI-style error event makes the SDKs, pi on the OpenAI path and DeepSeek Harness fail cleanly with no retry. A non-stream harness gets a JSON parse error |

The proxy must send nothing until the node answers or the hold limit ends. The status code stays free until then.

## 9. Fairness

### 9.1 Between conversations

A conversation with a seat has priority on its own seat. A held request cannot take a seat from a conversation that is inside its window.

Inside the queue, the order is arrival time. The queue is first in, first out. This applies alike to a new request and to a request with a table entry.

### 9.2 Between users

The proxy has no rule between users in v1. It serves held requests first come, first served. The owner dropped per-user ordering and the rule "a request that waited more than half of the hold limit goes first" (2026-10-07). Both stay parked for version 2 with the trigger "users complain of unfair waits". See [14](14-open-questions-and-risks.md).

### 9.3 What fairness does not do

It does not take a seat from a conversation inside the window. It does not limit the running requests of a user. It does not count tokens. A user with long prompts can still take much compute.

## 10. Overload signals

### 10.1 What the proxy knows

| Signal | Source | Use |
|---|---|---|
| Running requests per node | The proxy counter | Cap check. The only signal for Ollama and mlx_lm |
| `requests_processing`, `requests_deferred` | llama-server `/metrics` | Check against the proxy counter. A mismatch means other harnesses use the node |
| `/props` `total_slots` | llama-server | Cap input |
| `/slots` | llama-server | The proxy must not poll it when the node uses `--sleep`: it wakes the engine (PROVEN, s3) |
| `/api/ps` | Ollama | Loaded models. No load |
| Queue length, queue wait, hold-limit refusals | The proxy | Events and metrics. See [08](08-observability-and-admin.md) |

### 10.2 Overload states (PROPOSED)

| State | Condition | Action |
|---|---|---|
| Saturated node | Running equals the cap | No new request. Seats stay |
| Saturated pool | All nodes saturated or without a free seat, and at least one request held | Queue and events |
| Overloaded pool | A request reached the hold limit, or the queue is full | Refusal. Event `hold_refused` |

The proxy must report the state of each pool in metrics. It sets no other flag.

### 10.3 Response header (PROPOSED)

The proxy can add the header `x-legatus-queue-ms` to a response that waited in the queue. The value is the wait in milliseconds. The header is a diagnostic. It is not an input to any harness. The proxy removes every inbound `x-legatus-*` header before it calls a hosted node. See [06](06-protocols-and-harnesses.md).

## 11. Requirements

| ID | Requirement |
|---|---|
| PRX-ADM-001 | The proxy must keep a cap of running requests for each node. |
| PRX-ADM-002 | The proxy must set the cap to the smaller of the declared concurrency and the measured useful concurrency. |
| PRX-ADM-003 | The proxy must not raise a cap above the declared value because of a measurement. |
| PRX-ADM-004 | The proxy must read the declared slots of a llama-server node from `/props` field `total_slots` at join and on change. |
| PRX-ADM-005 | The proxy must use the registry value for the cap of an Ollama node, with the default 1. |
| PRX-ADM-006 | The proxy must use the default cap 1 for an mlx_lm node. |
| PRX-ADM-007 | The proxy must not send a request to a node when the running count of the node equals its cap. |
| PRX-ADM-008 | The proxy must count a request as running from the moment it sends the request to the node. The proxy must stop that count when the response ends or the harness disconnects. |
| PRX-ADM-009 | The proxy must keep `warm_capacity` seats per node. The default is the declared slot count. Changed 2026-10-07: earlier text said seats equal the cap. |
| PRX-ADM-010 | The proxy must keep the seat of an idle conversation for the protected window, with the default 180 s. |
| PRX-ADM-011 | The proxy must use the probation window, with the default 30 s, for a conversation with fewer than `mature_turns` successful requests. |
| PRX-ADM-012 | The proxy must use the probation window for every WEAK key. |
| PRX-ADM-013 | The proxy must start the idle time of a conversation at the end of its last request. |
| PRX-ADM-014 | The proxy must reject a configuration where `hold_limit` is not larger than `window`. |
| PRX-ADM-015 | The proxy must give a new conversation the seat of a node with a free seat before any other seat. |
| PRX-ADM-016 | The proxy must choose, among nodes with a free seat, the node with the fewest held seats. If nodes are equal, the proxy must choose the node with the fewest running requests. If nodes are still equal, the proxy must choose the node that a rendezvous hash of the conversation key ranks first. |
| PRX-ADM-017 | The proxy must give a new conversation the reclaimable seat with the longest idle time when no free seat exists. The proxy must compare the idle times over all nodes of the pool. |
| PRX-ADM-018 | The proxy must reclaim a seat only when a request needs it. |
| PRX-ADM-019 | The proxy must not take a seat from a conversation inside its window. |
| PRX-ADM-020 | The proxy must hold a request in the hold queue when no node can admit it. |
| PRX-ADM-021 | The proxy must keep one hold queue per pool. |
| PRX-ADM-022 | The proxy must send no byte of a response to the harness while the request is in the hold queue. |
| PRX-ADM-023 | The proxy must keep a request with a table entry in the hold queue until the node of its entry can admit it. |
| PRX-ADM-024 | The proxy must admit a held new request on any node of the pool that frees a seat. |
| PRX-ADM-025 | The proxy must remove a request from the hold queue when the harness disconnects. |
| PRX-ADM-026 | The proxy must answer a request at once with the refusal when the hold queue holds `max_held` requests. |
| PRX-ADM-027 | The proxy must end the hold of a request after `hold_limit` seconds, with the default 250 s. |
| PRX-ADM-028 | The proxy must answer a request at the hold limit with the status of `hold_limit_status`. The default is 503 (DEC-064). |
| PRX-ADM-029 | The proxy must send no `Retry-After` header, or a `Retry-After` header of 30 s or less, in the refusal at the hold limit. The default is 30 s (pending owner confirmation, DEC-064). |
| PRX-ADM-030 | The proxy must keep the words "quota", "billing" and "rate limit" out of the refusal body. |
| PRX-ADM-031 | REMOVED 2026-10-07. The owner dropped the rule that keeps the arrival time of a refused request. |
| PRX-ADM-032 | REMOVED 2026-10-07. The owner dropped per-user ordering of held requests. |
| PRX-ADM-033 | REMOVED 2026-10-07. The owner dropped the rule that serves a request first after half of the hold limit. |
| PRX-ADM-034 | REMOVED 2026-10-07. The setting `user_key` does not exist in v1. |
| PRX-ADM-035 | The proxy must apply no cap and no hold to a hosted node in v1. |
| PRX-ADM-036 | The proxy must not poll `/slots` on a llama-server node that runs with `--sleep`. |
| PRX-ADM-037 | The proxy must write the events `hold_start`, `hold_end` and `hold_refused` with the wait, the pool and the outcome. |
| PRX-ADM-038 | The proxy must report in metrics, for each pool, the queue length, the oldest wait and the count of refusals. |
| PRX-ADM-039 | The proxy must check the context of the prompt against the slot context of the node and not against the total context. |
| PRX-ADM-040 | The proxy must load `window`, `probation_window`, `mature_turns`, `hold_limit`, `max_held` and `hold_limit_status` from the registry. |
| PRX-ADM-041 | The proxy must allow `hold_limit` per alias. |
| PRX-ADM-042 | The proxy must pass a refusal or an error that a node sends to the harness as it is, with no retry. |
| PRX-ADM-043 | The proxy must write a warning event at start when `hold_limit` is above 290 s. |
| PRX-ADM-044 | The proxy must accept the values 503, 504 and 529 for `hold_limit_status` and must refuse the value 429. |
| PRX-ADM-045 | The proxy must send no response head, no keep-alive byte and no SSE comment while it holds a request. |
| PRX-ADM-046 | The proxy must treat a retry of a refused request as a new request with a new place at the end of the queue. |
| PRX-ADM-047 | The proxy must keep the words `server_error`, `overloaded`, `timeout` and `terminated` out of the body of a refusal that must not be retried. |
| PRX-ADM-048 | The proxy must make the hold-limit refusal body of the OpenAI path and of the Responses path in the OpenAI error shape. |
| PRX-ADM-049 | The proxy must set `warm_capacity` of a node to a value of at least the cap of that node. |
| PRX-ADM-050 | The proxy must raise `warm_capacity` of a node above its slot count only in two cases. In the first case, the registry declares a value. In the second case, the calibration probe shows that a second conversation stays warm after the slot serves another conversation. |
| PRX-ADM-051 | The proxy must keep the seats of a node with one sequence for each slot and no host cache equal to its slot count. |
| PRX-ADM-052 | The proxy must report in the admin read the `warm_capacity` of each node, its source (declared or calibrated) and the date of the calibration. |
| PRX-ADM-053 | The proxy must send the refusal at the hold limit of a `/v1/responses` request with status 503 and never with status 429. |

## 12. Tests

Tests run on the simulated cluster of [13](13-test-fixtures-and-scenarios.md) with virtual time.

| Test | Check | Requirements |
|---|---|---|
| Example 7.1 | Placement and no wait | PRX-ADM-015, 016 |
| Example 7.2 | Hold, lazy reclaim, S1 returns | PRX-ADM-017, 018, 019 |
| Example 7.3 | Refusal at 250 s, `Retry-After`, retry goes to the end | PRX-ADM-027, 028, 029 |
| Example 7.4 | Parent and children resolve inside the hold limit | PRX-ADM-010, 011, 014 |
| Cap property | Running count never exceeds the cap under seeded random load | PRX-ADM-007, 008 |
| Order | Held requests leave the queue in order of arrival | PRX-ADM-020, 024 |
| Configuration | The proxy refuses `hold_limit` at or below `window` | PRX-ADM-014 |
| Hold tolerance by harness | A held request of 250 s completes at the first attempt in every harness of table 8.1 | PRX-ADM-027, 045. PROVEN on fakes (spike D). [FIX-306](13-test-fixtures-and-scenarios.md) |
| Status at the hold limit | Every harness retries 503. The registry refuses 429 | PRX-ADM-028, 044. FIX-307 |

## 13. Risks and open points

| Item | Text | Settled by |
|---|---|---|
| Window value | 180 s has the label PROPOSED. No source gives a value for a local engine. Spike 7 measured warm swaps, not an idle time limit | A trace replay and spike B |
| Seats equal slots | A node with a host cache keeps more warm conversations than its slots (PROVEN for llama-server with `--cache-ram`, two conversations, spike 7). Section 4.5 sets `warm_capacity`. Eviction beyond two conversations is NOT TESTED | A probe with more than two conversations. Spike B on the owner machines |
| Hold of 250 s | PROVEN at the first attempt in every harness tested (spike D, fakes). Open WebUI is NOT TESTED | An Open WebUI run |
| Many holds at once | Spike D did not test many simultaneous holds. They are NOT TESTED | A load test with 64 held requests |
| Hold beyond 700 s | The stage 1 results above 700 s are void | A rerun, if the operator wants a hold limit above 600 s |
| Status code | CLOSED. Every harness tested retries 503 (PROVEN, spike D). Open WebUI is NOT TESTED | An Open WebUI run |
| Cold prefill after the hold | `hold_limit` plus `t_head` must stay at or below 290 s. The real `t_head` of each engine under load is NOT TESTED | Per alias hold limit. Time to headers per engine |
| Side request wait | A compaction call can wait for a window while its parent idles | Family release or hint. OPEN |
| Probation window | An addition of this file. No measurement | A trace replay with workflows |
| Seat accounting | The proxy models the engine cache and does not read it | Cache feedback in [03](03-affinity-and-keys.md) |

Items for [14](14-open-questions-and-risks.md): window value, seats versus cap, time to headers per engine, many simultaneous holds, side request wait, probation window.

## Sources

- [s3 slots versus concurrency](evidence/spikes/s3-slots-README.md): caps, queue behaviour, load signals.
- [s7 hybrid cache](evidence/spikes/s7-hybrid-cache/README.md): warm conversations beyond the slot count with `--cache-ram`.
- [r7 Responses API](evidence/research/r7-responses-api-20261007T081115Z.md): Codex retry behaviour at the hold limit.
- [s5 pi restart and hold](evidence/spikes/s5-pi-restart-README.md): retry timeline, status table, hold tolerance, `Retry-After`.
- [Spike D, hold tolerance](evidence/spikes/sD-hold-tolerance/README.md): give-up times, status matrix, `Retry-After`, retry patience, early head.
- [R3 SGLang and routing](evidence/research/r3-sglang-routing-20261007T022127Z.md): hosted idle windows, deadlock reference (pi-extensible-workflows issue 264), CONCUR.
- [R5 gateways](evidence/research/r5-gateways-20261007T055410Z.md): HAProxy `maxconn`, `timeout queue`, LiteLLM, Bifrost.
- [R6 routers](evidence/research/r6-routers-20261007T055357Z.md): Paddler, SMG, vLLM router, llama-swap, proxycache.
- [R4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md): cold prefill cost.
- [Open WebUI notes](../../docs/horizon/openwebui.md): parallel sub-agents.
- Sibling files: [02](02-architecture.md), [03](03-affinity-and-keys.md), [05](05-engine-behaviour.md), [06](06-protocols-and-harnesses.md), [07](07-registry-and-configuration.md), [08](08-observability-and-admin.md), [13](13-test-fixtures-and-scenarios.md), [14](14-open-questions-and-risks.md), [16](16-parked-capability-manager.md).
