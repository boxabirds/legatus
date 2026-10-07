# 02 Architecture

Status: draft for review. Date: 2026-10-07. Owner of IDs: PRX-SCOPE and PRX-PERF.

This file describes the parts of the Legatus v1 proxy. It also gives the path of one request, the state, the processes, the resource targets and the failure behaviour. Terms are in the [glossary](15-glossary.md). Decisions are in the [decision log](01-decisions.md). Scope is in [vision and scope](00-vision-and-scope.md).

Labels: PROVEN means measured or read from source, with a reference. PROPOSED means a design choice that nobody measured. ASSUMPTION means believed and not checked. NOT TESTED means no test exists.

## 1. Scope rules for the architecture

| ID | Requirement |
| --- | --- |
| PRX-SCOPE-001 | The proxy must work as a reverse proxy between existing harnesses and LLM endpoints. |
| PRX-SCOPE-002 | The proxy must need no change in the harness. |
| PRX-SCOPE-003 | The proxy must ship as one static binary written in Rust. |
| PRX-SCOPE-004 | The proxy must keep all routing state in memory and must use no database. |
| PRX-SCOPE-005 | The proxy must write its logs as JSON lines. |
| PRX-SCOPE-006 | The proxy must not retry a request, fail over a request or open a circuit breaker in v1. |
| PRX-SCOPE-007 | The proxy must apply no timeout in v1 except the hold limit. |
| PRX-SCOPE-008 | The proxy must not start, stop or manage an engine process. |
| PRX-SCOPE-009 | The proxy must not store prompt text or completion text. |
| PRX-SCOPE-010 | The proxy must not translate between OpenAI chat completions, OpenAI Responses and Anthropic Messages. |
| PRX-SCOPE-011 | The proxy must keep the seams in section 6 so that v2 can add retries and failover without a rewrite. |

The capability manager, the lease protocol, harness adapters, price tracking and conversation capture are outside v1. See [parked capability manager](16-parked-capability-manager.md).

PRX-SCOPE-007 differs from the result of round 1. Round 1 found that timeouts are necessary. The owner deferred timeouts to v2. The cost: a hung node holds pi for 300 s. The pi retry then returns to the same node ([spike decisions](../decisions/2026-10-spike-decisions.md), section 3.2, PROVEN for the prototype).

## 2. Component list

| Component | Process | Job | Detail file |
| --- | --- | --- | --- |
| Listener | proxy | Accept HTTP connections. Start at once, before the registry is ready. | [09](09-restart-and-failure.md) |
| Protocol layer | proxy | Recognise the path. Read the model name, the stream flag and the key fields. Leave all other bytes unchanged. | [06](06-protocols-and-harnesses.md) |
| Key extractor | proxy | Compute the affinity key from a header or from the body. | [03](03-affinity-and-keys.md) |
| Registry | proxy | Hold nodes, aliases and patches. Read from a file. | [07](07-registry-and-configuration.md) |
| Affinity table | proxy | Map a key to a node and a last-seen time. | [03](03-affinity-and-keys.md) |
| Admission controller | proxy | Count running requests per node. Apply the cap and the protected window. | [04](04-admission-control-and-queueing.md) |
| Hold queue | proxy | Hold requests that have no free seat or cap place. Release them first come, first served. | [04](04-admission-control-and-queueing.md) |
| Node client | proxy | Send the request to the node. Return the response stream. | this file |
| SSE passthrough | proxy | Copy response bytes to the harness with no buffering and no change. | [06](06-protocols-and-harnesses.md) |
| Cache feedback | proxy | Read cache fields from the response. Update the reuse measure of the node. | [05](05-engine-behaviour.md) |
| Event log writer | proxy | Write one metadata event per request to disk. | [08](08-observability-and-admin.md) |
| Node agent | node machine | Report health, load and machine readings to the proxy. | [07](07-registry-and-configuration.md) |
| Calibrator | proxy or tool | Measure a node at join and on change. Write the measured profile. | [05](05-engine-behaviour.md) |
| Admin API | admin | Serve read-only versioned data. Require a token. | [08](08-observability-and-admin.md) |
| Dashboard | admin | Show the admin data in a web page. | [08](08-observability-and-admin.md) |

The node agent and the calibrator are optional in the first release. The proxy must run with a registry file alone (PRX-SCOPE-012).

| ID | Requirement |
| --- | --- |
| PRX-SCOPE-012 | The proxy must route requests with a registry file alone when no node agent runs. |

## 3. Context diagram

```
  harness (pi, Claude Code, Open WebUI, ...)
        |  OpenAI chat, OpenAI Responses or Anthropic Messages
        v
  +--------------------------- proxy process (Rust) ----------------------------+
  | listener -> protocol layer -> key extractor -> affinity table               |
  |                                     |                |                      |
  |                                     v                v                      |
  |                              admission controller <- registry (file)        |
  |                                     |        \                              |
  |                                     |         +-> hold queue                |
  |                                     v                                       |
  |                               node client -> SSE passthrough -> harness       |
  |                                     |                |                      |
  |                                     |                +-> cache feedback     |
  |                                     v                                       |
  |                               event log writer --> JSON lines on disk       |
  +-------------------------------------|---------------------------------------+
        |                  |                              ^
        v                  v                              | read-only
   local engine        hosted node               +------------------+
   (llama-server,      (key held by the          | admin process    |
    Ollama, mlx_lm,     proxy, passthrough)      | API + dashboard  |
    vLLM, SGLang)                                +------------------+
        ^
        | health, load, machine readings
   node agent (one per machine, optional)  ---->  proxy
```

## 4. Request flow

The numbered steps describe one request. The proxy runs them in order.

1. Accept the connection and read the request line and the headers. When the registry defines a hosted node, check the client token. Answer 401 when the token is absent or wrong.
2. Recognise the protocol from the path. Pass another OpenAI-style path (embeddings, audio, images) to a node that serves it (approved by the owner, DEC-004). Serve the Responses API path `/v1/responses` like the chat path, as pass-through with no translation (DEC-062, DEC-065). Send it only to a node with the flag `responses`. Answer 404 when the pool has no such node. Engine support is in [05](05-engine-behaviour.md) section 14.
3. Read the `model` value, the `stream` flag and the key fields from the body. Do not change the other bytes.
4. Look up the `model` value in the registry. If it names no alias, answer with a protocol-shaped 404 error.
5. Compute the affinity key from the configured header, or from the hash of the first system message and the first non-system message.
6. Look up the key in the affinity table. If the node is available, select it.
7. If the key has no entry, ask the admission controller for a node with a free seat. The seats of a node follow its `warm_capacity` (DEC-066). Then apply the protected window rule.
8. If no node has a free seat or a free cap place, put the request in the hold queue. Wait until a cap place frees or the hold limit ends.
9. If the hold limit ends, answer with status 503 and no `Retry-After`, or `Retry-After` of 30 s or less (DEC-064). Send no byte before this answer.
10. Apply the node patches. Rewrite the `model` value to the model name of the node. Remove inbound credentials and `x-legatus-*` headers for a hosted node. Add the node key.
11. Check the prompt size estimate against the loaded context of the node. If it is too large, answer with a context error.
12. Send the request to the node. Count the request as a running request.
13. Copy the response bytes to the harness as they arrive. Do not buffer. Do not change them.
14. Read the cache fields from the response. Update the reuse measure and the cold-turn flag.
15. Update the affinity table with the node and the time. Count the request as finished and release its cap place.
16. Write one event to the event log. The event holds metadata only.

If the node fails before the first byte, the proxy returns the node error to the harness. The harness decides whether to retry. A retry is a new request that starts at step 1.

| ID | Requirement |
| --- | --- |
| PRX-SCOPE-013 | The proxy must run the request steps in the order of section 4. |
| PRX-SCOPE-014 | The proxy must start step 13 before it reads the end of the response. |
| PRX-SCOPE-015 | The proxy must release the cap place of a request when the request ends for any reason. A harness disconnect is one such reason. |
| PRX-SCOPE-016 | The proxy must write the event of step 16 for every request. This includes refused requests and failed requests. |
| PRX-SCOPE-028 | The proxy must require a client token on every request when the registry defines a hosted node. The proxy must serve requests with no token when the registry defines none. See PRX-SEC-013 to PRX-SEC-016 in [07](07-registry-and-configuration.md). |
| PRX-SCOPE-029 | The proxy must make no claim above the design ceiling of 20 nodes and 100 concurrent conversations. PROPOSED. The owner approved the ceiling on 2026-10-07 (DEC-052). |

## 5. State list

| State | Where | Lost at restart | Rebuilt by | Notes |
| --- | --- | --- | --- | --- |
| Registry (nodes, aliases, patches) | Memory, read from the registry file | No | Read the file again | The file is the source of truth. |
| Measured profile of a node | Memory, saved in a file next to the registry (PROPOSED) | No | Read the file or calibrate again | OPEN: file or in memory only. See [07](07-registry-and-configuration.md). |
| Affinity table (key to node, last-seen time) | Memory | Yes | Traffic | Soft state. No journal. See [03](03-affinity-and-keys.md). |
| Running count per node | Memory | Yes | Zero at start | Requests in flight end with the restart. |
| Hold queue | Memory | Yes | Harness retries or reconnects | The harness sees a closed connection. |
| Cache reuse measure per node and harness | Memory | Yes | Traffic | Starts with no data. The node gets the default policy. |
| Node health and load readings | Memory | Yes | Node agent reports and engine probes | |
| Event log | Disk, JSON lines | No | Not applicable | Metadata only. A free-disk guard pauses logging. See [08](08-observability-and-admin.md). |
| Hosted node keys | Environment or a secret file | No | Read at start | Never written to the log or the admin API. |
| Admin token | Environment or a secret file | No | Read at start | |

The proxy has no other state on disk. The proxy has no database and no journal of the affinity table.

| ID | Requirement |
| --- | --- |
| PRX-SCOPE-017 | The proxy must hold the affinity table, the hold queue and the running counts in memory only. |
| PRX-SCOPE-018 | The proxy must start with an empty affinity table and must need no recovery step. |
| PRX-SCOPE-019 | The proxy must write to disk only the event log and files that the operator configures. |

## 6. Process model

| Process | Count | Runs on | Job | Failure effect |
| --- | --- | --- | --- | --- |
| Proxy | 1 | A machine on the network of the nodes | Everything in section 2 marked proxy. | Harness requests fail or wait. See section 9. |
| Node agent | 1 per machine, optional | Each machine with an engine | Report health, load and machine readings. | The proxy uses engine probes and its own running count. |
| Admin | 1 | Same machine as the proxy (PROPOSED) | Read-only API and the dashboard. | No effect on routing. |

The admin process is separate so that a dashboard fault or a heavy query cannot slow a request. The admin process reads the event log and a read-only local interface of the proxy. File [08](08-observability-and-admin.md) gives that interface the label PROPOSED.

The proxy uses one async runtime with a small fixed set of tasks. The per-request work is: parse, select, send, copy. The event log writer runs in its own task and receives events through a bounded channel.

| ID | Requirement |
| --- | --- |
| PRX-SCOPE-020 | The proxy must run the admin API and the dashboard in a process that is separate from the routing process. |
| PRX-SCOPE-021 | The proxy must never block a request task on a disk write. |
| PRX-SCOPE-022 | The proxy must keep three seams for v2. The seams are one send function for the node client, a monotonic clock that tests can control, and a trait for the log writer. |

The seams come from [spike 2](evidence/spikes/s2-paused-time-README.md) (PROVEN there): tokio paused time works with an in-memory transport and does not work with real sockets.

## 7. Resource targets

| Target | Value | Source | Label |
| --- | --- | --- | --- |
| Added time to first byte | 0.06 ms | Round 1 prototype, [spike decisions](../decisions/2026-10-spike-decisions.md) section 3.2 | PROVEN for the prototype only |
| Added time per SSE chunk | 0.05 ms | Same | PROVEN for the prototype only |
| Resident memory | 3 to 6 MB | Same | PROVEN for the prototype only |
| Added time to first byte, v1 | At most 1 ms at p99 | Owner design | PROPOSED |
| Resident memory, v1, 10 000 table entries | At most 50 MB | Owner design | PROPOSED |
| Design ceiling | 20 nodes and 100 concurrent conversations | Owner decision, 2026-10-07 (DEC-052) | PROPOSED. Approved by the owner |
| Hold limit | 250 s | `hold_limit` plus the time until the node sends its response head must stay at or below 290 s (spike D) | PROPOSED |
| Request timeout of pi | 300 s | [spike s5](evidence/spikes/s5-pi-restart-README.md), case 4 | PROVEN |
| Held request that pi tolerates | At least 250 s | [Spike D](evidence/spikes/sD-hold-tolerance/README.md) (earlier: 120 s, spike s5) | PROVEN |
| Held request of 250 s, every harness tested | Completes at the first attempt | [Spike D](evidence/spikes/sD-hold-tolerance/README.md), table 2 | PROVEN |
| Pi retry window for refusals | About 14 s | Same | PROVEN |

The prototype had no admission control, no event log and no key extraction from the body. It ran with a few conversations. The v1 numbers are not a promise that the prototype numbers will hold. A benchmark must measure them again. See [test fixtures](13-test-fixtures-and-scenarios.md).

| ID | Requirement |
| --- | --- |
| PRX-PERF-001 | The proxy must add at most 1 ms at p99 to the time to first byte on a local node. |
| PRX-PERF-002 | The proxy must add at most 1 ms at p99 to the time between two SSE chunks. |
| PRX-PERF-003 | The proxy must stay at or below 50 MB resident memory with 10 000 table entries and 100 concurrent conversations. |
| PRX-PERF-004 | The proxy must copy the response without a copy of the whole body in memory. |
| PRX-PERF-005 | The proxy must hold its requests with no thread per request. |
| PRX-PERF-006 | The proxy must hold at least 100 held requests without a change in the added time for running requests. |
| PRX-PERF-007 | The proxy must report the measured value of each target in a benchmark report before a release. |

PRX-PERF-001 to PRX-PERF-003 and PRX-PERF-006 have the label PROPOSED. The benchmark settles them.

## 8. Load on the engines

The proxy sends no extra request to an engine on the request path. The proxy does not warm caches and does not probe a node during a request. Probes run on a timer and use the cheap signals in [05](05-engine-behaviour.md). The proxy never requests `/slots` on a llama-server that has `--sleep` on, because the request wakes the engine.

## 9. Failure behaviour summary

Details are in [09](09-restart-and-failure.md). The harness sees the node error. The proxy adds no recovery in v1.

| Failure | Proxy behaviour | What the harness sees |
| --- | --- | --- |
| Proxy restarts | Listen at once. Hold requests until ready. | A held request, then a normal answer. Every harness tested tolerates a hold of 250 s (PROVEN, spike D). |
| Proxy restarts while a stream runs | The stream ends. | pi reports `terminated` and retries (PROVEN, s5 case 3b). |
| Node refuses the connection | Return an error. Mark the node unavailable. | A 5xx error. pi retries for about 14 s. |
| Node answers with an error status | Pass the status and body. | The node error. |
| Node dies mid-stream | End the stream as in [06](06-protocols-and-harnesses.md), section 5. Chat path: abrupt close. Messages path: one `overloaded_error` event, then close. Responses path: abrupt close. | A broken stream or an error event. The harness retries (PROVEN for a dropped connection in every harness, spike D). |
| Node hangs | Wait. The proxy has no timeout in v1. | pi waits 300 s, then retries (PROVEN for the prototype). |
| Node of the conversation unavailable | Move the conversation to another node. | A full prefill on the new node. |
| Every node of an alias busy | Hold until the hold limit. | A held request, then a retry-later error (503). Every harness retries it (PROVEN, spike D). |
| Every node of an alias unavailable | Return a 5xx error that names the alias. | An error. |
| Registry file invalid at start | Refuse to start. Name the error. | Connection refused. |
| Registry file invalid at reload | Keep the old registry. Log the error. | No change. |
| Log disk almost full | Pause logging. Write one event that says so. | No change. |
| Harness disconnects | Release the cap place. Close the node request. | Not applicable. |

| ID | Requirement |
| --- | --- |
| PRX-SCOPE-023 | The proxy must keep routing when the event log pauses. |
| PRX-SCOPE-024 | The proxy must keep routing when the admin process stops. |
| PRX-SCOPE-025 | The proxy must close the node request when the harness closes its connection. |
| PRX-SCOPE-026 | The proxy must refuse to start with an invalid registry file. |
| PRX-SCOPE-027 | The proxy must keep the previous registry when a reload fails. |
| PRX-SCOPE-030 | The proxy must serve `POST /v1/responses` in v1 as a third protocol path (DEC-062). |
| PRX-SCOPE-031 | The proxy must send a Responses request only to a node whose registry flag `responses` is true. The default of the flag is false. |
| PRX-SCOPE-032 | The proxy must answer 404 on `/v1/responses` when the pool has no node with the flag `responses` (DEC-065, owner decision pending). |
| PRX-SCOPE-033 | The proxy must pass a Responses request and its stream without translation and without a change of the bytes, except the `model` value. |

PRX-SCOPE-025 has the label PROPOSED. The mlx_lm engine ignores a cancelled prefill (PROVEN, spike decisions section 3.5). The node can stay busy after the proxy closes the request. The proxy must count that node as busy until the node answers or the connection ends (OPEN: measure).

## Sources

- [Spike decisions, round 1](../decisions/2026-10-spike-decisions.md): router prototype figures and engine facts.
- [Scope reset](../decisions/2026-10-scope-reset.md).
- [Spike s2, paused time](evidence/spikes/s2-paused-time-README.md): seams for the clock and the transport.
- [Spike s5, pi restart](evidence/spikes/s5-pi-restart-README.md): hold tolerance, retry window, request timeout.
- [Spike D, hold tolerance](evidence/spikes/sD-hold-tolerance/README.md): hold of 250 s in every harness, give-up times.
- [Spike s3, slots](evidence/spikes/s3-slots-README.md): engine concurrency.
- [Spike 7, hybrid cache](evidence/spikes/s7-hybrid-cache/README.md): warm capacity beyond the slot count.
- [Research r7](evidence/research/r7-responses-api-20261007T081115Z.md): Responses API pass-through.
- [Research r3](evidence/research/r3-sglang-routing-20261007T022127Z.md) and [r6](evidence/research/r6-routers-20261007T055357Z.md): routers with held requests.
