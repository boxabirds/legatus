# 09 Restart and failure

Status: draft for review. Date: 2026-10-07. Requirement IDs: PRX-REST-001 and up.

This file defines what the proxy does at start, at restart, with bad files, and when a node fails or returns. It also lists what v1 does not do and what the harness sees in each case. Terms are in [15-glossary.md](15-glossary.md).

Logging is in [08-observability-and-admin.md](08-observability-and-admin.md). Admission and the hold limit are in [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md). Affinity state is in [03-affinity-and-keys.md](03-affinity-and-keys.md). The registry file is in [07-registry-and-configuration.md](07-registry-and-configuration.md).

## 1. Principles

- v1 has no retries, no failover and no circuit breaker. v1 has no timeout except the hold limit.
- A node failure reaches the harness. The harness decides whether to retry.
- A conversation whose node is unavailable moves to another node.
- All proxy state is soft state. The proxy has no journal and no recovery work.
- The proxy never answers with an invented success. Each request has one recorded outcome.

The first rule is a decision of the owner (see [01-decisions.md](01-decisions.md)). It has a cost, which section 8 states.

## 2. Evidence: how pi 1.0.3 reacts

Spike 5 ran pi 1.0.3 against a fake endpoint. See [spike 5](evidence/spikes/s5-pi-restart-README.md). All rows are PROVEN by that spike unless the text says otherwise.

| Case | What pi 1.0.3 does |
|---|---|
| Connection refused | Retries 3 times, at about 0, 2, 6 and 14 s. Gives up after 14.3 s with `Connection error.`. |
| Status 503 | The same timeline. Recovers when the endpoint answers 200 within 14 s of the first failure. Fails at 14.5 s. |
| Status 500, 502, 504 | Retried (OpenAI path). |
| Status 429 | Retried, also with `Retry-After`. |
| Status 400, 401, 404, 409, 422 | Not retried. |
| Status 529 | Not retried, unless the body says "overloaded". |
| `Retry-After` header | Ignored, also at 120 s. |
| Connection cut during a stream | Error `terminated`. Retried. pi drops the partial text, and the retry request does not contain it. |
| First byte after 5, 20, 60 or 120 s | No retry. The request completes. |
| No byte at all | Gives up at 300.6 s with `Request timed out.` and retries once. |

pi decides to retry from a text match on the status and the body, not from the status alone. A refusal body that contains words such as "503", "timeout", "rate limit", "overloaded" or "terminated" can cause a retry. The proxy must keep these words out of an error body unless it wants a retry.

The same spike ran the Anthropic path. Refused and 503 cases gave the same result. Anthropic-path statuses 429, 500, 502, 504 and 529 with "overloaded" text were NOT TESTED. Nobody tested Claude Code for restart. Spike D showed that every harness retries a dropped connection. Spike round 1 showed that an `overloaded_error` makes both pi and Claude Code retry ([spike decisions](../decisions/2026-10-spike-decisions.md)).

## 3. Restart sequence

### 3.1 Why the proxy listens at once and holds

Without a hold, a restart has only the retry window of the harness. For pi, this window is about 14 s from the first failed request. The delay of the supervisor counts in it.

A held request has a much larger window. In spike 5, pi waited 120 s for a first byte and still succeeded with no retry. In spike D, a hold of 250 s completed at the first attempt in every harness tested. The lowest give-up time with no byte is 299 s (pi, DeepSeek Harness). See [04](04-admission-control-and-queueing.md), section 8.1.

A refused connection or a 503 also uses one of the 3 retries. A refused connection shows no reason.

### 3.2 Sequence

1. The process starts. It opens the log directory and takes the lock (PRX-OBS-007).
2. The process binds the port for the harness. It accepts connections from this moment.
3. The process loads the registry. If the registry is bad, the process exits (section 5).
4. The process reads the node load signals and the first health result for each node.
5. The process sets `ready` and writes a `ready` event.
6. The process releases the held requests, oldest first.

- **PRX-REST-001** The proxy must bind the port for the harness before it loads the registry and before it contacts any node.
- **PRX-REST-002** The proxy must accept connections and read requests while it is not ready, and must hold each request until the proxy is ready.
- **PRX-REST-003** The proxy must not refuse a connection and must not answer 503 because it is not ready, until the hold limit ends.
- **PRX-REST-004** The proxy must release held requests in order of arrival when it becomes ready.
- **PRX-REST-005** The proxy must count the wait for ready in `queue_wait_ms`. The wait counts against the same hold limit as a wait for a cap place.
- **PRX-REST-006** The proxy must answer a request at the hold limit with the hold-limit error from [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md). This holds for a wait for ready and for a wait for a cap place.
- **PRX-REST-007** The proxy must set ready only after it loads the registry and every node has a first health result or a probe failure.
- **PRX-REST-008** The proxy must wait at most 5 s for the first health result of a node, then mark the node `down`. The value is PROPOSED.
- **PRX-REST-009** The proxy must limit the number of held requests. A request beyond the limit gets the hold-limit error at once. The limit is `max_held` of [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md), with the default 64 (PROPOSED).

The hold limit is PROPOSED at 250 s. The sum of the hold limit and the time until the node head must stay at or below 290 s (PROVEN limit, spike D). See file 04. A listen backlog is finite. The spike showed that a bound socket accepts a connect and a send in 7 ms on macOS (python test). It did not test a large number of held connections.

### 3.3 Shutdown and supervisor

- **PRX-REST-010** On SIGTERM or SIGINT the proxy must flush the event queue, write a `shutdown` event and exit. It must not wait for streams that run. This rule is PROPOSED.
- **PRX-REST-011** The proxy must close each stream that runs so that the harness sees an abrupt end. The proxy must send no clean stream terminator.

A cut stream gives pi the error `terminated`. pi retries and drops the partial text (spike 5). A drain period delays the restart. Whether a drain period pays is OPEN.

The proxy does not restart itself. An operator tool does this, for example `launchd` or systemd. The delay between the death and the new listener counts against the 14 s window of pi. The operator must set the restart delay of the supervisor to at most 1 s. The value is PROPOSED. The proxy has no code for the supervisor.

## 4. Soft state recovery

The proxy loses the affinity table, the table of response ids, the protected windows, the hold queue, the counters and the health states at restart. The proxy rebuilds them from traffic. A node can still hold the cache of a conversation after the restart. A llama-server node with a host cache keeps more warm conversations than its slots (spike 7). A conversation that comes back can find its state intact when the rendezvous hash picks the same node.

What the harness sees: a request after the restart arrives as a request of an unknown conversation. The proxy places it as a new conversation. If the proxy picks the node that holds the warm cache, the engine reuses its cache and the turn is warm. If the proxy picks another node, the turn pays a full prefill. The engine caches themselves survive a proxy restart.

Cost: the cluster size is an ASSUMPTION. The sources report the prefill times. A full prefill at 60k tokens takes 47 s on gufo and 230 s on llama.cpp ([R4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md)). A cluster of 5 nodes with 4 conversations each has up to 20 conversations that can come back at once. A bad placement of each one costs one full prefill per conversation. Pending spike 7 for hybrid models: the cost of a cold prefill on this machine.

Two measures cut this cost without any stored state.

- **PRX-REST-012** The proxy must place a new conversation on the node that a rendezvous hash of the key ranks first. This applies when several nodes have a free seat. The coordinator accepted this rule on 2026-10-07 (PROPOSED until measured). It makes the placement after a restart equal to the placement before it, when load is equal. File 04 (PRX-ADM-016) uses it as the last tie-break.
- **PRX-REST-013** The proxy must read the load signals of each node at start. These are llama-server `/metrics` and Ollama `/api/ps`. The proxy must count work that already runs as part of the cap.

- **PRX-REST-014** The proxy must record the first turn after a restart with `cache_class` `first`. It must record a later cold turn as `cold`. The cost of the restart then shows in the cold-turn rate (file 08).
- **PRX-REST-015** The proxy must not write the affinity table to disk in v1.

A conversation that came back after a restart and found a warm cache is a warm turn. This is the success case of PRX-REST-012.

## 5. Bad file handling

| File | Bad at start | Bad at reload | The proxy |
|---|---|---|---|
| Registry | Exit with an error | Keep the last good registry | Serves on |
| Admin token | Admin process stays off | Admin process goes off | Serves on |
| Log directory | Log paused | Log paused | Serves on |
| Salt file | Create a new file when none exists. Exit when the file cannot be read. | Not read again | Serves on |

### 5.1 Registry

A bad registry is a file that does not exist, cannot be read, cannot be parsed or is not valid under the rules in [07-registry-and-configuration.md](07-registry-and-configuration.md). A reload is atomic: the proxy uses the whole new registry or none of it.

- **PRX-REST-016** At start, when the registry is bad, the proxy must exit with a non-zero code. The error must name the file and the rule. No last good registry exists.
- **PRX-REST-017** At reload, the proxy must keep using the last good registry when the new file is bad.
- **PRX-REST-018** The proxy must write a `registry_rejected` event with the error list, set `legatus_registry_ok` to zero, and show the error in the admin status.
- **PRX-REST-019** The proxy must apply a good registry to new requests only. A request that runs keeps the registry that it started with.
- **PRX-REST-020** The proxy must move a conversation to another node of its alias when a good registry removes its node.
- **PRX-REST-021** The proxy must keep the last good registry in memory only, and must not write a copy to disk. This rule is PROPOSED.

The proxy binds the port first (PRX-REST-001). A bad registry at start therefore gives a short window where the proxy accepts a connection and then closes it. This window is short. A held request at that time sees a connection that ends with no response. The harness treats it as a connection error and retries. This case is an operator error, and the exit code and the message show it.

### 5.2 Token file

The admin process owns the token file. A bad token file is a file that does not exist, has no content or cannot be read. The admin process then does not open its listener and does not accept any token. It does not fall back to an old token. This rule is the reverse of the registry rule, because the token protects data.

- **PRX-REST-022** The admin process must stay off when the token file is bad, and must write the reason to standard error.
- **PRX-REST-023** The proxy must serve requests with no change when the token file is bad.
- **PRX-REST-024** The admin process must turn on again when the file becomes good, with no restart of the proxy.

## 6. A second proxy on the same port

- **PRX-REST-025** The proxy must exit with a non-zero code and one line on standard error when the bind of the harness port fails.
- **PRX-REST-026** The proxy must not set the socket option `SO_REUSEPORT`. With this option, two proxies share one port, keep two affinity tables and split conversations.
- **PRX-REST-027** The proxy must exit when it cannot get the lock on the log directory (PRX-OBS-007), also when its own bind worked.
- **PRX-REST-028** A second proxy that fails must not change the state of the first proxy.

## 7. Node failure and node return

### 7.1 Health state

The proxy keeps one health state for each node: `up` or `down`. The state is not a circuit breaker. It has no counters, no half-open state and no timer that reopens it. These are the only inputs.

- **PRX-REST-029** The proxy must set a node to `down` when the node agent or a health probe reports the node as down.
- **PRX-REST-030** The proxy must set a node to `down` when a connection to the node fails. The proxy must do the same when the node cuts a stream before its end. This rule is PROPOSED.
- **PRX-REST-031** The proxy must set a node to `up` only after a successful health probe.
- **PRX-REST-032** The proxy must probe a `down` node at a fixed interval of 2 s. The value is PROPOSED. The probe must not use `/slots` on a llama-server with `--sleep`.
- **PRX-REST-033** The proxy must write a `node_state` event for each change.

Reason for PRX-REST-030: a request that fails on a dead node tells the proxy more than the next probe. The cost is that one cut stream can set a healthy node to `down` for one probe interval. This cost is PROPOSED to accept.

### 7.2 What the harness sees

The status codes below are PROPOSED unless marked. The pi column is PROVEN by spike 5 for the OpenAI path. The Claude Code column is NOT TESTED unless marked.

| Case | Proxy action | Status the harness gets | pi 1.0.3 | Claude Code |
|---|---|---|---|---|
| Node `down` before the request | Move the conversation. Serve from another node. | 200 | Nothing to see. The turn is cold. | Same. |
| Node dead, health not known yet. Connection refused. | Set node `down`. Answer. | 502 | Retried after 2 s. The retry moves the conversation. | ASSUMPTION: retried. |
| Node dies after the first byte, chat path | Close the connection to the harness with no terminator. Set node `down`. | None. The stream ends. | `terminated`. Retried. Partial dropped. The retry moves the conversation. | Not applicable. Claude Code does not use the chat path. |
| Node dies after the first byte, Messages path | Send one `overloaded_error` event, then close. Set node `down`. | None. The stream ends with the event. | Retried. The retry moves the conversation. | Retries an `overloaded_error` (round 1). Retries an abrupt close too (PROVEN, spike D). |
| Node dies after the first byte, Responses path | Close the connection with no terminator. Set node `down`. | None. The stream ends. | Not applicable. | Codex retries a dropped connection (30 requests, PROVEN, spike D). |
| Node answers with an error status | Pass the status and body through. Record it. No move. | The status of the node | 500, 502, 503, 504, 429 retried to the same node. 400, 401, 404, 409, 422 not retried. | NOT TESTED. |
| Node accepts and sends no byte | Wait. Hold the cap place. | None until the harness gives up | Gives up at 299 s to 300.6 s. Retries once to the same node. | Gives up at 360 s, then retries (PROVEN, spike D). |
| Harness closes its connection | Cancel the node request. Free the cap place. | None | Not applicable | Not applicable |
| No node of the alias is `up` | Answer at once. | 503 | Retried 3 times in 14 s. Then the error shows. | ASSUMPTION: retried. |
| Hold limit reached | Answer with the hold-limit error. 503 with no `Retry-After` or 30 s or less (DEC-064). See file 04. | 503 | Retries (PROVEN, spike D). | Retries (PROVEN, spike D). |
| Proxy restarts, request not yet sent | Hold. | 200 later | No retry. | NOT TESTED. |
| Proxy dies during a stream | None. | None. The stream ends. | `terminated`. Retried. | NOT TESTED. |

- **PRX-REST-034** The proxy must pass the status code and the body of a node error to the harness with no change. It must not turn a node error into another status.
- **PRX-REST-035** The proxy must answer 502 when a connection to a node fails before the first byte. The status is PROPOSED.
- **PRX-REST-036** The proxy must answer 503 when no node of the alias is `up`. The status is PROPOSED.
- **PRX-REST-037** The proxy must write an error body in the shape of the protocol that the harness used (see [06-protocols-and-harnesses.md](06-protocols-and-harnesses.md)).
- **PRX-REST-038** The proxy must keep the words "timeout", "rate limit", `server_error`, "overloaded" and "terminated" out of a body that it makes. The hold-limit error is the exception, because the proxy wants a retry here.
- **PRX-REST-039** When a node cuts a stream on the chat path, the proxy must end the stream with an abrupt close. It must not send a clean terminator or an error event after a partial body.
- **PRX-REST-052** When a node cuts a stream on the Responses path, the proxy must close the stream abruptly. It must send no error event.
- **PRX-REST-053** The proxy must not write the table of response ids to disk. After a restart, a request with `previous_response_id` goes to the node that the key chooses, and the proxy passes the error of that node (PRX-PROTO-061). This rule is PROPOSED.
- **PRX-REST-051** When a node cuts a stream on the Messages path, the proxy must send one `overloaded_error` event and then close. The owner approved this default on 2026-10-07 (DEC-058). Spike D showed that Claude Code also retries an abrupt close (PROVEN). The default stays.
- **PRX-REST-040** The proxy must cancel the node request and free the cap place when the harness closes its connection.
- **PRX-REST-041** The proxy must decide the node of a retried request again from the current health state, because the retry is a new request.

A conversation moves only when its node is `down` or removed. A move gives a full prefill on the new node. The cost is in section 4. The proxy records the move in the `move_reason` field.

### 7.3 Node return

- **PRX-REST-042** The proxy must set a returned node to `up` after a good probe, and must let new conversations use it.
- **PRX-REST-043** The proxy must not move a conversation back to the node that it left. Strict stickiness holds on the new node.
- **PRX-REST-044** The proxy must keep the table entries of conversations that never left the node. It must not assume that the cache of the node is cold.
- **PRX-REST-045** The proxy must start a calibration of the node when the version or the model of the node differs from the registry. File 07 defines the calibration.

An engine that restarted has an empty cache. The conversations that stayed on the node then show a cold turn in the log. This is the right result: the cold-turn measure finds a lost cache without a special rule (see [08-observability-and-admin.md](08-observability-and-admin.md)).

## 8. What v1 does not do

| Not in v1 | Result for the harness |
|---|---|
| Retry a failed request | The harness sees the failure. pi retries 3 times in 14 s. |
| Fail over a request to another node | A request that fails on a node fails. The next request of the conversation moves. |
| Circuit breaker | A bad node that still answers a probe keeps its conversations. |
| First-byte timeout | A silent node holds a cap place until the harness closes. pi waits 300 s. |
| Idle timeout in a stream | A stalled stream holds a cap place until the harness closes. |
| Health from request quality | A node that answers 500 to each request stays `up`. |
| Drain at shutdown | A running stream ends in `terminated`. |

RISK: spike round 1 found that router timeouts were mandatory. With a first-byte timeout of 3 s, pi recovered in 3.4 s. With no timeout it waited 300 s ([spike decisions](../decisions/2026-10-spike-decisions.md), section 3.2).

The owner chose v1 with no timeout except the hold limit. The cost is a stall of up to 300 s on each hung node, and a leaked cap place until the harness closes. The seams in section 9 make a timeout cheap to add. [14-open-questions-and-risks.md](14-open-questions-and-risks.md) records this risk.

## 9. Seams for v2 resilience

The seams cost almost nothing in v1. They keep v2 resilience small.

- **PRX-REST-046** The proxy must have exactly one function in the code that opens a connection to a node and sends a request. All routes must use it.
- **PRX-REST-047** The proxy must record exactly one outcome for each request. The outcome is a value from the closed list below.
- **PRX-REST-048** The proxy must make the placement function accept a list of nodes to exclude, empty in v1.
- **PRX-REST-049** The proxy must record whether it sent a byte to the harness (`first_byte_sent`), because a failover is only safe before the first byte.
- **PRX-REST-050** The proxy must feed the health state only from recorded outcomes and probes, so that a v2 breaker reads the same stream.

Closed list of outcomes:

| Outcome | Meaning |
|---|---|
| `ok` | The node answered and the response ended well. |
| `node_error_status` | The node answered with an error status. The proxy passed it on. |
| `node_connect_failed` | The connection to the node failed before the first byte. |
| `node_stream_cut` | The node closed a stream before its end. |
| `client_closed` | The harness closed its connection first. |
| `hold_limit` | The request waited for the hold limit. |
| `no_node` | No node of the alias was `up`. |
| `context_guard` | The prompt estimate exceeded the loaded context of the node. |
| `bad_request` | The proxy refused the request for a protocol reason. |
| `unauthorized` | The client token was missing or wrong. |
| `proxy_error` | An internal fault of the proxy. |

Where v2 plugs in:

| v2 feature | Where it attaches | Needs in v1 |
|---|---|---|
| Retry | Wraps the single call site | PRX-REST-046 |
| Failover before the first byte | Calls placement again with an exclude list | PRX-REST-048, 049 |
| Circuit breaker | Reads the outcome stream | PRX-REST-047, 050 |
| First-byte timeout | A timer at the single call site | PRX-REST-046 |

## 10. Restart behaviour tests

Local labels. The fixture IDs belong to [13-test-fixtures-and-scenarios.md](13-test-fixtures-and-scenarios.md). Tests with virtual time use an in-memory transport (spike 2). Tests with a real socket or a real pi use real time.

| Test | Check | Requirement |
|---|---|---|
| REST-T01 | Start the proxy. A connect succeeds before the proxy loads the registry. | PRX-REST-001 |
| REST-T02 | Send a request while the proxy starts. Make ready 20 s later. The request completes once, with no error and `queue_wait_ms` near 20 s. | PRX-REST-002, 003, 005 |
| REST-T03 | Run the real pi 1.0.3 with `-p` through a proxy that holds requests for 20 s. The run completes with no retry event. | PRX-REST-002 |
| REST-T04 | Make ready later than the hold limit. The request gets the hold-limit error at the limit. | PRX-REST-006 |
| REST-T05 | Hold 64 requests during start. All complete in order of arrival. The 65th gets the hold-limit error at once. | PRX-REST-004, 009 |
| REST-T06 | Make one node give no health result. The proxy is ready after 5 s and the node is `down`. | PRX-REST-007, 008 |
| REST-T07 | Restart the proxy with the same traffic. The table is empty. The first request of each conversation is `first`. | PRX-REST-014, 015 |
| REST-T08 | Restart with equal load on 3 nodes. Each conversation goes to the same node as before the restart. | PRX-REST-012 |
| REST-T09 | Restart with a stub that keeps its cache, then with a stub that loses it. The turn is warm, then cold. | PRX-REST-014 |
| REST-T10 | Leave a request running on a llama-server stub, then restart. The proxy counts it against the cap. | PRX-REST-013 |
| REST-T11 | Start with a bad registry. The proxy exits non-zero and the message names the rule. | PRX-REST-016 |
| REST-T12 | Reload a bad registry. Requests still use the last good registry. The event, the metric and the status show the error. | PRX-REST-017, 018 |
| REST-T13 | Reload a registry that removes a node. Its conversations move. A running request finishes on the old registry. | PRX-REST-019, 020 |
| REST-T14 | Start with a bad token file. The admin process is off. Requests succeed. Fix the file. The admin process turns on. | PRX-REST-022 to 024 |
| REST-T15 | Start a second proxy on the same port while a stream runs. The second exits. The stream is not affected. | PRX-REST-025, 028 |
| REST-T16 | Start a second proxy on another port with the same log directory. The second exits. | PRX-REST-027 |
| REST-T17 | Take a node down before a request. The conversation moves. The status is 200. The log shows `moved`. | PRX-REST-029, 041 |
| REST-T18 | Refuse the connection of a node. The status is 502. The next request moves. | PRX-REST-030, 035, 041 |
| REST-T19 | Cut a chat stream. The harness sees an abrupt end with no terminator. The outcome is `node_stream_cut`. | PRX-REST-039 |
| REST-T26 | Cut a Messages stream. The harness sees one `overloaded_error` event and then a close. The outcome is `node_stream_cut`. | PRX-REST-051 |
| REST-T20 | Make a node answer 500 and 429. The harness gets the same status and body. | PRX-REST-034 |
| REST-T21 | Close the harness connection during a stream. The proxy cancels the node request. The cap place is free. | PRX-REST-040 |
| REST-T22 | Bring a node back. New conversations use it. A moved conversation stays where it is. | PRX-REST-042, 043 |
| REST-T23 | Send SIGTERM during a stream. The stream ends abruptly. The proxy writes a `shutdown` event. | PRX-REST-010, 011 |
| REST-T24 | Run a seeded run of 10000 requests with faults. Each request has exactly one outcome. All counters return to zero. | PRX-REST-047 |
| REST-T25 | Make a node silent. The request waits. The harness closes. The cap place is free. | PRX-REST-040 |

## Open items

- CLOSED: the status for the hold limit (file 04) is 503 (DEC-064, PROVEN in spike D). PRX-REST-006 uses it.
- OPEN: a drain period at shutdown (section 3.3).
- CLOSED: the coordinator accepted PRX-REST-012. PRX-ADM-016 uses the rendezvous hash as its last tie-break (coordinator decision, 2026-10-07).
- OPEN: restart behaviour of Claude Code and other harnesses. NOT TESTED. The hold tolerance is PROVEN (spike D).
- RISK: no first-byte timeout (section 8).
- RISK: held connections at scale and the listen backlog size. NOT TESTED (spike 5, spike D).

## Sources

- [Spike 5: pi restart and retry behaviour](evidence/spikes/s5-pi-restart-README.md)
- [Spike D: hold tolerance](evidence/spikes/sD-hold-tolerance/README.md)
- [Spike 2: paused time](evidence/spikes/s2-paused-time-README.md)
- [R4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md)
- [Spike round 1 decisions](../decisions/2026-10-spike-decisions.md)
- [Scope reset](../decisions/2026-10-scope-reset.md)
- [Observability and admin](08-observability-and-admin.md)
