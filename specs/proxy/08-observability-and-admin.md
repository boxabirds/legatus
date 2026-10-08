# 08 Observability and admin

Status: draft for review. Date: 2026-10-07. Requirement IDs: PRX-OBS-001 and up.

This file defines what the proxy records and what it never records. It also defines the cold-turn measure, the metrics, and the read-only admin process. The glossary defines the terms: [15-glossary.md](15-glossary.md).

Restart and failure behaviour is in [09-restart-and-failure.md](09-restart-and-failure.md). Affinity keys are in [03-affinity-and-keys.md](03-affinity-and-keys.md). Admission and queueing are in [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md). Engine response fields are in [05-engine-behaviour.md](05-engine-behaviour.md). The registry is in [07-registry-and-configuration.md](07-registry-and-configuration.md).

## 1. Purpose and limits

The owner wants to see whether affinity pays. The proxy must show, for each node and each harness, how many prompt tokens the engine reused and how many it computed again. A cold turn at 60k tokens costs 47 s on gufo and 230 s on llama.cpp. The source is the owner repo notes, as read in [R4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md). The log and the metrics exist to make that cost visible.

v1 observability has four parts:

1. A per-request event log on local disk.
2. A metrics endpoint in Prometheus text format.
3. A read-only admin API.
4. A dashboard that uses the admin API.

The admin API, the metrics endpoint and the dashboard run in a separate process (section 9). The proxy has no control action in v1.

## 2. Per-request event log

### 2.1 Format and rules

The proxy writes one JSON object per line. The proxy writes one `request` event for each request, when it knows the outcome. The proxy also writes system events (section 2.4).

- **PRX-OBS-001** The proxy must write exactly one `request` event for each accepted request. This includes a request that ends in an error or a harness disconnect.
- **PRX-OBS-002** The proxy must write the event after it knows the outcome, with the single outcome value defined in [09-restart-and-failure.md](09-restart-and-failure.md).
- **PRX-OBS-003** The proxy must write events to files named by UTC day, in the form `events-YYYY-MM-DD.jsonl`, in one configured log directory. This file naming is PROPOSED.
- **PRX-OBS-004** The proxy must write events through a bounded in-memory queue and a writer task, so that a slow disk never delays a response.
- **PRX-OBS-005** The proxy must count an event that it drops because the queue is full, and must show the count in the metric `legatus_event_log_dropped_total`.
- **PRX-OBS-006** The proxy must create log files with the mode 0600.
- **PRX-OBS-007** The proxy must hold an exclusive lock on the log directory while it runs, so that two proxies never write the same files.

### 2.2 Fields of a `request` event

All fields below are metadata. The Source column says where the value comes from. When the proxy has no value for a field, it writes `null`, never an invented value.

| Field | Type | Meaning |
|---|---|---|
| `schema` | integer | Event schema version. Starts at 1. |
| `event` | string | The value `request`. |
| `ts` | string | UTC time when the request arrived, RFC 3339, milliseconds. |
| `request_id` | string | Proxy-made unique identifier. Never taken from a harness header. |
| `harness` | string | Harness label from the route configuration, for example `pi` or `claude-code`. The value `unknown` when no rule matches. |
| `protocol` | string | `openai_chat`, `anthropic_messages` or `passthrough`. |
| `path` | string | Request path without the query string. |
| `stream` | boolean | True when the harness asked for streaming. |
| `alias` | string | Requested alias. The value `unknown_alias` when the alias is not in the registry. |
| `node` | string | Registry identifier of the chosen node. Null when no node was chosen. |
| `node_model` | string | Model name that the proxy wrote into the request. |
| `session_ref` | string | Non-reversible reference of the conversation key (section 3). |
| `key_source` | string | `header:<name>`, `derived` or `none`. The header name only, never the value. |
| `placement` | string | `affinity`, `new_warm_slot`, `new_evict_idle`, `held_then_placed`, `least_loaded`, `moved` or `none`. |
| `move_reason` | string | Set only when `placement` is `moved`. For example `node_unavailable` or `node_removed`. |
| `move_cost_tokens` | integer | Set only when `placement` is `moved`. The prompt tokens of the previous turn. The proxy must recompute them on the new node. |
| `node_running` | integer | Requests running on the node at the decision. |
| `node_cap` | integer | Cap of the node at the decision. |
| `held` | boolean | True when the request waited in the hold queue. |
| `queue_wait_ms` | integer | Time from arrival to dispatch. Zero when not held. |
| `outcome` | string | One value from the closed list in [09-restart-and-failure.md](09-restart-and-failure.md). |
| `status` | integer | Status code that the harness received. Null when the proxy cut the connection. |
| `upstream_status` | integer | Status code that the node returned. Null when the node gave none. |
| `first_byte_sent` | boolean | True when the proxy sent at least one response byte to the harness. |
| `ttfb_ms` | integer | Time from dispatch to the first byte from the node. |
| `duration_ms` | integer | Time from arrival to the end of the response. |
| `overhead_ms` | integer | Time that the proxy spent before it sent the request to the node, without queue wait. |
| `prompt_tokens` | integer | Total prompt tokens as the engine reports them. |
| `cached_tokens` | integer | Prompt tokens that the engine reused from its cache. |
| `completion_tokens` | integer | Completion tokens as the engine reports them. |
| `cache_class` | string | `first`, `warm`, `cold` or `unknown` (section 4). |
| `expected_reuse` | integer | Prompt tokens of the previous turn of this conversation. |
| `recomputed_tokens` | integer | Tokens that the engine computed again (section 4). |
| `cache_basis` | string | Name of the response field that gave `cached_tokens`. |
| `bytes_in` | integer | Request body bytes from the harness. |
| `bytes_out` | integer | Response body bytes to the harness. |

### 2.3 What the proxy never logs

- **PRX-OBS-008** The proxy must not write prompt text, system prompt text, tool definitions, tool arguments or tool results. This covers every log, metric, label and admin response.
- **PRX-OBS-009** The proxy must not write completion text or reasoning text. This covers every log, metric, label and admin response.
- **PRX-OBS-010** The proxy must not write a credential: an `Authorization` header, an `x-api-key` header, a cookie, a hosted node key or the admin token.
- **PRX-OBS-011** The proxy must not write a raw session value. A raw session value is a session header value, the input of a key hash, or the first message of a conversation.
- **PRX-OBS-012** The proxy must not write the body of an error that a node returns. A node can repeat prompt text in an error.
- **PRX-OBS-013** The proxy must not write a header value that is not on a fixed allow list. The allow list holds only the header names that the event fields above need.
- **PRX-OBS-014** The proxy must write an alias only when the alias is in the registry. Otherwise the proxy must write `unknown_alias`.

The last rule means that a harness cannot add text to the log through the `model` field.

### 2.4 System events

The proxy writes these events to the same files: `start`, `ready`, `registry_loaded`, `registry_rejected`, `node_state`, `log_paused`, `log_resumed` and `shutdown`. Each has `schema`, `event`, `ts` and fields that suit the event. A `registry_rejected` event carries the error list from validation, never file content. A `node_state` event carries the node, the old state, the new state and the reason.

- **PRX-OBS-015** The proxy must write a `node_state` event for each change of the health state of a node.
- **PRX-OBS-016** The proxy must write a `registry_rejected` event when it refuses a registry file, and must keep the last good registry (see [09-restart-and-failure.md](09-restart-and-failure.md)).
- **PRX-OBS-058** The proxy must write the field `move_cost_tokens` in a `request` event with the placement `moved`. The value is the `prompt_tokens` that the previous turn of the conversation stored in the table entry (PRX-OBS-024).
- **PRX-OBS-059** The proxy must set `cache_class` to `first` when no previous turn of the conversation key is stored (new conversation, restart, compaction or expiry). The proxy must leave first turns out of the cold count. PROPOSED.

## 3. Session reference

The admin side never shows a raw session value. It shows a harness label and a session reference.

- **PRX-OBS-017** The proxy must compute `session_ref` as the first 8 bytes of an HMAC-SHA-256 of the conversation key with a secret salt. The proxy must write the 8 bytes as 16 hexadecimal characters. This construction is PROPOSED.
- **PRX-OBS-018** The proxy must derive the salt from the persistent secret in `state_dir` (32 random bytes in a file with the mode 0600, created at first start and reused at each later start). The proxy must not write a second secret file. PROPOSED.
- **PRX-OBS-019** The proxy must not write the salt to a log or send it to the admin process.
- **PRX-OBS-020** The proxy must compute the reference from the final conversation key. The salt then also covers a derived key, which is a hash of the first messages.

Reasons for the construction:

- A plain hash of a short session value, such as a counter, is easy to reverse. The secret salt stops this.
- A stable salt keeps the same reference across restarts, so a person can follow one conversation across a restart.
- A 64-bit reference is enough to tell conversations apart in a cluster of about 5 machines (PROPOSED).
- To break the link between old logs and new logs, the operator removes the salt file. All references then change.

## 4. Cold-turn measure

A cold turn is a turn where the engine did not reuse the cache that it needed to reuse. The proxy cannot see the engine cache. The proxy infers the cold turn from the response fields.

### 4.1 Definitions

- **First turn:** the key was not in the affinity table before this request.
- **Expected reuse:** the `prompt_tokens` value plus the completion tokens of the previous completed turn of the same conversation. The affinity table stores both values.
- **Expected new tokens:** the prompt tokens of this turn minus the expected reuse, or zero when the result is negative.
- **Recomputed tokens:** the prompt tokens of this turn minus `cached_tokens`. On llama-server this equals `timings.prompt_n`.
- **Cold turn:** a turn after the first turn where the recomputed tokens exceed the expected new tokens by more than the allowance. The allowance is 516 tokens on a hybrid llama-server node (PROVEN, spike 7). The checkpoint lies 516 tokens before the end. The allowance is PROPOSED on other nodes.
- **Warm turn:** a turn that is not a first turn and not cold.

[Spike 7](evidence/spikes/s7-hybrid-cache/README.md) supports this rule. It used Qwen3.5-2B on llama-server.

A pure append recomputed 201 tokens for 200 new tokens. A rewrite 194 tokens into a 6075-token history recomputed 5942 tokens. A rewrite at 9354 tokens in a 12281-token history recomputed 3877 tokens. That is 32 percent of the prompt and 16.5 s.

The earlier rule (cached tokens below half of the expected reuse) misses the last case. A turn with `prompt_n` far above the expected new tokens is cold. Spike 7 verified the non-stream shape. The streamed shape is NOT TESTED.

The cache class is `unknown` in three cases. The engine does not report the fields. The response failed before the usage data. The node is a hosted node.

### 4.2 Fields for each engine

| Engine | Cached tokens | Processed tokens | Total prompt | Status |
|---|---|---|---|---|
| llama-server | `timings.cache_n` | `timings.prompt_n` | `cache_n` plus `prompt_n` | From the brief. Do not use `tokens_cached`. |
| gufo | `timings.cache_n` | `timings.prompt_n` | `cache_n` plus `prompt_n` | From the brief. Do not use `tokens_cached`. |
| mlx-lm | `usage.prompt_tokens_details.cached_tokens` | total minus cached | `usage.prompt_tokens` | From the brief. |
| SGLang | `usage.prompt_tokens_details.cached_tokens` | total minus cached | `usage.prompt_tokens` | From the brief. |
| vLLM | `usage.prompt_tokens_details.cached_tokens` | total minus cached | `usage.prompt_tokens` | Only with `--enable-prompt-tokens-details`. Otherwise `unknown`. |
| Responses path (llama-server, vLLM, Ollama) | `usage.input_tokens_details.cached_tokens` in `response.completed` | total minus cached | `usage.input_tokens` | From R7 (source). The field is optional for Codex. Real Ollama values are NOT TESTED. Otherwise `unknown`. |
| Ollama | See [05-engine-behaviour.md](05-engine-behaviour.md) | See file 05 | See file 05 | ASSUMPTION. Spike round 1 reported cached tokens in usage for Ollama 0.35.1. The field name is not checked here. |
| Hosted | Not used in v1 | Not used | Not used | The class is `unknown`. |

Two facts limit the measure.

- **Silent zero hits.** An engine can report zero cached tokens and still work. The vLLM issue 45238 shows a silent zero hit on a hybrid model ([R2 vLLM notes](evidence/research/r2-vllm-20261007T022033Z.md)). The proxy reports the field as given. It does not claim to know the cause.
- **Compaction.** A compaction call rewrites the history and always misses ([R4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md)). pi sends compaction calls with no session header. The key then comes from the first messages. A compaction call can show as a first turn or a cold turn. This false positive exists.

### 4.3 Requirements

- **PRX-OBS-021** The proxy must read the cached token fields in the table from the node response. The proxy must not change any byte of the response.
- **PRX-OBS-022** The proxy must read the cached token fields from the final data chunk of a stream. For a non-stream response, the proxy must read the whole body.
- **PRX-OBS-023** The proxy must set `cache_class` to `unknown` when the needed field is absent, and must not guess a value.
- **PRX-OBS-024** The proxy must store the `prompt_tokens` value and the completion tokens of each completed turn in the affinity table entry of the conversation.
- **PRX-OBS-025** The proxy must write `cache_basis` so that a reader knows which field gave `cached_tokens`.
- **PRX-OBS-026** The proxy must make the cold allowance a configuration value. The default is 516 tokens for a hybrid llama-server node and is PROPOSED for other nodes. Changed 2026-10-07: the earlier default was one half of the expected reuse.


An OpenAI-style stream carries usage data only when the harness sets `stream_options.include_usage`. The proxy does not change harness requests. For an engine that needs it, a registry patch can add `stream_options` for every request to that node. The patch does not change the prompt tokens, so it does not break the cache.

Rules for patches are in [07-registry-and-configuration.md](07-registry-and-configuration.md). This use of a patch is PROPOSED. It is NOT TESTED.

## 5. Metrics

### 5.1 Principles

- **PRX-OBS-027** The admin process must serve all metrics in the Prometheus text format on the path `/metrics`. The same token rule as the admin API applies.
- **PRX-OBS-028** The proxy must not use a session reference, a request identifier or a path as a metric label. The number of these values has no limit.
- **PRX-OBS-029** The proxy must use base units in metric names: `seconds`, `bytes`, `tokens`, `celsius`, `watts`, and `ratio`.
- **PRX-OBS-030** The proxy must keep the label set to the labels in the table below.

### 5.2 Proposed metric set

All names below are PROPOSED. The label `harness` has a small fixed set of values from the route configuration. The label `class` is one of `first`, `warm`, `cold`, `unknown`.

| Name | Type | Unit | Labels | Meaning |
|---|---|---|---|---|
| `legatus_requests_total` | counter | requests | `alias`, `node`, `harness`, `protocol`, `outcome` | Requests by final outcome. |
| `legatus_request_duration_seconds` | histogram | seconds | `node`, `harness` | Time from arrival to the end of the response. |
| `legatus_ttfb_seconds` | histogram | seconds | `node`, `harness` | Time from dispatch to the first byte from the node. |
| `legatus_overhead_seconds` | histogram | seconds | none | Time that the proxy adds before dispatch, without queue wait. |
| `legatus_queue_wait_seconds` | histogram | seconds | `alias` | Time that a request waits in the hold queue. |
| `legatus_held_requests` | gauge | requests | `alias` | Requests in the hold queue now. |
| `legatus_holds_total` | counter | holds | `alias`, `result` | Holds by result: `served` or `hold_limit`. |
| `legatus_placements_total` | counter | placements | `alias`, `placement` | Placement decisions by kind. |
| `legatus_moves_total` | counter | moves | `reason` | Conversations that moved to another node. |
| `legatus_turns_total` | counter | turns | `node`, `harness`, `class` | Turns by cache class. |
| `legatus_prompt_tokens_total` | counter | tokens | `node`, `harness` | Prompt tokens reported. |
| `legatus_cached_tokens_total` | counter | tokens | `node`, `harness` | Cached tokens reported. |
| `legatus_recomputed_tokens_total` | counter | tokens | `node`, `harness` | Recomputed tokens (section 4). |
| `legatus_completion_tokens_total` | counter | tokens | `node`, `harness` | Completion tokens reported. |
| `legatus_node_up` | gauge | ratio | `node` | One when the health state is up. |
| `legatus_node_running_requests` | gauge | requests | `node` | Requests running now. |
| `legatus_node_cap_requests` | gauge | requests | `node` | Cap of the node. |
| `legatus_node_warm_conversations` | gauge | conversations | `node` | Conversations inside the protected window. |
| `legatus_conversations` | gauge | conversations | none | Entries in the affinity table. |
| `legatus_ready` | gauge | ratio | none | One after the proxy is ready. |
| `legatus_registry_ok` | gauge | ratio | none | Zero when the last reload failed. |
| `legatus_event_log_paused` | gauge | ratio | none | One while the disk guard pauses the log. |
| `legatus_event_log_free_bytes` | gauge | bytes | none | Free space on the log volume. |
| `legatus_event_log_dropped_total` | counter | events | `reason` | Events not written. |
| `legatus_machine_temperature_celsius` | gauge | celsius | `machine`, `sensor` | Reading from the node agent. |
| `legatus_machine_power_watts` | gauge | watts | `machine` | Reading from the node agent. |
| `legatus_machine_throttled` | gauge | ratio | `machine` | One when the node agent reports thermal throttling. |

The node agent reports the three `machine` values. The node agent protocol is outside this file: see [07-registry-and-configuration.md](07-registry-and-configuration.md). When a node has no agent, the proxy does not export the machine metrics for it.

### 5.3 OpenTelemetry GenAI conventions

PROPOSED. Web research on 2026-10-07 read the OpenTelemetry GenAI metrics page at [semantic-conventions-genai, gen-ai-metrics.md](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-metrics.md). All GenAI metrics on that page have the stability level Development. Names and attributes can still change. The proxy therefore keeps the Prometheus names above as the stable surface, and uses the table below for a later optional export.

| OpenTelemetry name | Unit | Source | Legatus metric | Fit |
|---|---|---|---|---|
| `gen_ai.client.operation.duration` | `s` | Read from the page | `legatus_request_duration_seconds` | Good. The proxy is the client of the node. |
| `gen_ai.server.request.duration` | `s` | Read from the page | `legatus_request_duration_seconds` | Good, from the harness side. |
| `gen_ai.server.time_to_first_token` | `s` | Read from the page | `legatus_ttfb_seconds` | Partial. The proxy measures the first byte, not the first token. The first chunk can hold no token. |
| `gen_ai.server.time_per_output_token` | `s` | Read from the page | none | The proxy does not parse tokens in v1. |
| `gen_ai.client.token.usage` | `{token}` | Named in search results only | `legatus_prompt_tokens_total`, `legatus_completion_tokens_total` | Partial. It is a histogram with an attribute `gen_ai.token.type`. The proxy uses counters. ASSUMPTION: check on the page before use. |

These attributes fit: `gen_ai.operation.name` (the value `chat`), `gen_ai.request.model` (the alias), `gen_ai.response.model` (the node model name) and `gen_ai.provider.name` (the engine name). Also `error.type` (the outcome) and `server.address` fit. The page marks `gen_ai.operation.name` as required and the others as required under conditions or recommended. Nobody checked the cache token attributes in the GenAI conventions: NOT TESTED. No OpenTelemetry convention exists for affinity, hold queues, cold turns or machine readings. Those keep the `legatus_` names.

- **PRX-OBS-031** The proxy must not need an OpenTelemetry collector or library to run. An OpenTelemetry export is out of v1 and is a later option.

## 6. Dashboard panels

The dashboard is a static page that calls the admin API (section 8). It shows these panels.

| Panel | Shows | Source |
|---|---|---|
| Nodes and load | Each node: health state, running requests, cap, engine, model, machine. | `/v1/nodes` |
| Conversations per node | Count of conversations in the table for each node. | `/v1/nodes` |
| Warm seats | Per node: seats in the protected window, free seats, cap. | `/v1/nodes` |
| Held requests | Count of held requests now, and the oldest wait. | `/v1/status` |
| Queue wait | Percentiles of `legatus_queue_wait_seconds` over the last hour. | `/metrics` |
| Cache hit by harness and node | Cached tokens divided by prompt tokens, by harness and node. | `/metrics` |
| Tokens recomputed | Recomputed tokens per turn, by harness and node. | `/metrics` |
| Cold-turn rate | Cold turns divided by non-first turns, by harness and node. | `/metrics` |
| Machine | Temperature, throttle flag and power for each machine. | `/metrics` |
| Recent decisions | The last events: time, harness, session reference, alias, node, placement, outcome, queue wait. | `/v1/decisions` |
| Proxy state | Ready flag, registry state, log state, free disk, uptime. | `/v1/status` |

- **PRX-OBS-032** The dashboard must show the cold-turn rate and the recomputed tokens for each harness and each node. The product claim depends on them.
- **PRX-OBS-033** The dashboard must show the log state in the first screen. The log state includes a paused log.
- **PRX-OBS-034** The dashboard must show only values from the admin API. It must not read the log files itself.

## 7. Disk guard and retention

- **PRX-OBS-035** The proxy must keep all events in v1. The proxy must not remove a log file and must not rotate log files.
- **PRX-OBS-036** The proxy must check the free space of the log volume at a fixed interval of 10 s. The interval is PROPOSED.
- **PRX-OBS-037** The proxy must pause event logging when the free space is below 1 GiB. The value is PROPOSED.
- **PRX-OBS-038** The proxy must resume event logging when the free space is above 2 GiB. The gap between 1 and 2 GiB stops fast changes between the two states. Both values are PROPOSED.
- **PRX-OBS-039** The proxy must keep serving requests while the disk guard pauses the log.
- **PRX-OBS-040** The proxy must say that it paused the log in four places. These are one line on standard error, the metric `legatus_event_log_paused`, the admin status and the dashboard.
- **PRX-OBS-041** The proxy must write a `log_paused` event before it stops, and a `log_resumed` event with the count of skipped events when it resumes.
- **PRX-OBS-042** The proxy must treat a log directory that it cannot write as a paused log, and must keep serving.

Size estimate: ASSUMPTION. An event has about 600 bytes. At 20 requests each minute the log grows by about 17 MB each day. Measure the real size in the first test run. An operator who needs retention can move old files, because the proxy only appends.

## 8. Admin API principles

All names below are PROPOSED.

- **PRX-OBS-043** The admin API must have a version in the path, with the prefix `/v1/`.
- **PRX-OBS-044** The admin API must require a bearer token on every path. This includes `/metrics`.
- **PRX-OBS-045** The admin API must compare the token in constant time, and must never accept the token in a URL.
- **PRX-OBS-046** The admin API must accept only the methods GET and HEAD, and must answer every other method with status 405.
- **PRX-OBS-047** The admin API must offer no control action in v1: no drain, no move, no table edit, no registry edit, no cache flush.
- **PRX-OBS-048** The admin API must never return a holder value. A holder value is a raw session value, a credential, a header value, a prompt or a completion.
- **PRX-OBS-049** The admin API must return lists in pages with a `limit` and an opaque `cursor`. The default limit is 100 and the maximum is 1000. Both numbers are PROPOSED.
- **PRX-OBS-050** The admin API must return the registry with secrets removed, and must show a secret as the text `redacted`.
- **PRX-OBS-051** The admin API must bind to the loopback address unless the operator sets another address.
- **PRX-OBS-052** The admin API must answer 401 when the token is absent or wrong, with no hint about the token.

| Path | Returns |
|---|---|
| `/v1/status` | Ready flag, versions, uptime, registry state, log state, held count. |
| `/v1/nodes` | Per node: state, load, cap, warm capacity, conversation count, machine readings. |
| `/v1/conversations` | Page of: session reference, harness, node, last seen, last class. |
| `/v1/decisions` | Page of recent `request` events, newest first. |
| `/v1/registry` | The active registry with secrets removed. |
| `/metrics` | Prometheus text format. |

## 9. Separate admin process

The admin API, the metrics endpoint and the dashboard run in one small process, apart from the proxy.

Reasons:

- A fault in the admin code cannot stop a request.
- The proxy has no admin listener and no admin code on the request path.
- The admin process has its own port, its own token file and its own bind address.
- The operator can leave the admin process off.

The admin process gets data from two sources. The first source is a read-only state interface of the proxy on a Unix socket in the log directory (PROPOSED). The second source is the event files, read from disk. The state interface carries no holder value (PRX-OBS-048).

- **PRX-OBS-053** The proxy must work when no admin process runs, and must not wait for one.
- **PRX-OBS-054** The proxy must expose its state only on a Unix socket with the mode 0600. The state must contain only values that the admin API can show.
- **PRX-OBS-055** The admin process must read its token from a file at start and when the file changes.
- **PRX-OBS-056** The admin process must stay off when the token file does not exist, has no content or cannot be read. It must say why on standard error. It must not use an old token. See [09-restart-and-failure.md](09-restart-and-failure.md).

## 10. Privacy summary

| Data | In the event log | In metrics | In the admin API |
|---|---|---|---|
| Prompt and completion text | Never | Never | Never |
| Credentials and the admin token | Never | Never | Never |
| Raw session value | Never | Never | Never |
| Session reference | Yes | No | Yes |
| Harness label | Yes | Yes | Yes |
| Token counts and timings | Yes | Yes | Yes |
| Node and alias names | Yes | Yes | Yes |
| Error body from a node | Never | Never | Never |

- **PRX-OBS-057** The proxy must remove inbound credentials and every `x-legatus-*` header before a request goes to a hosted node, as the key rules require. The rule is in [07-registry-and-configuration.md](07-registry-and-configuration.md). This file repeats the rule because the log must show no such header.

The log is metadata about work. It still shows who worked when, on which harness. The log directory must have access control that the operator sets. The proxy sets the file mode only.

## 11. Observability tests

Local labels. The fixture IDs belong to [13-test-fixtures-and-scenarios.md](13-test-fixtures-and-scenarios.md).

| Test | Check | Requirement |
|---|---|---|
| OBS-T01 | Send 100 requests with a canary string in the prompt, the reply, a header and the session value. Search every log, metric and admin response. The canary appears nowhere. | PRX-OBS-008 to 013 |
| OBS-T02 | Send a request with an unknown alias that holds a canary string. The log shows `unknown_alias`. | PRX-OBS-014 |
| OBS-T03 | Run one request to a stub of each engine kind. The log shows the expected `cached_tokens` and `cache_basis`. | PRX-OBS-021 to 025 |
| OBS-T04 | Run a stream with usage in the final chunk. The bytes that the harness receives equal the bytes that the node sent. | PRX-OBS-021 |
| OBS-T05 | Run turn 1, then turn 2 with a stub that reports zero cached tokens. Turn 2 is `cold` and `recomputed_tokens` equals the turn 2 prompt tokens. A stub that recomputes 3877 of 12281 tokens with 20 expected new tokens is also `cold`. | PRX-OBS-024, 026 |
| OBS-T06 | Restart the proxy. The same session value gives the same `session_ref`. Remove the salt. The reference changes. | PRX-OBS-017 to 018 |
| OBS-T07 | Fill a test volume below the pause value. The log pauses, requests continue, the four signals appear, and the log resumes above the resume value. | PRX-OBS-036 to 042 |
| OBS-T08 | Start two proxies with one log directory. The second one exits. | PRX-OBS-007 |
| OBS-T09 | Call the admin API with no token, a wrong token, a POST and a query token. Each call fails as specified. | PRX-OBS-044 to 046 |
| OBS-T10 | Read a list in pages. No entry is missing or repeated while new events arrive. | PRX-OBS-049 |
| OBS-T11 | Stop the admin process. All requests succeed. | PRX-OBS-053 |
| OBS-T12 | Run 10000 requests with a slow disk stub. The p99 added latency does not rise, and the drop counter matches the number of dropped events. | PRX-OBS-004 to 005 |

## Open items

- OPEN: the Ollama response field for cached tokens (file 05 must settle it).
- OPEN: the cold allowance on engines other than llama-server, and the streamed field shape. Spike 7 verified the rule on llama-server with non-stream responses only.
- OPEN: whether the proxy must parse the first content chunk to give a true time to first token. This costs a scan of the stream. v1 uses the first byte.
- OPEN: the node agent protocol for machine readings.

## Sources

- [Spike 5: pi restart and retry behaviour](evidence/spikes/s5-pi-restart-README.md)
- [R2 vLLM notes](evidence/research/r2-vllm-20261007T022033Z.md)
- [R4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md)
- [R7 Responses API](evidence/research/r7-responses-api-20261007T081115Z.md)
- [Spike 7 hybrid cache](evidence/spikes/s7-hybrid-cache/README.md)
- [Spike round 1 decisions](../decisions/2026-10-spike-decisions.md)
- OpenTelemetry GenAI metrics: https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-metrics.md (read 2026-10-07)
- OpenTelemetry metrics conventions: https://opentelemetry.io/docs/specs/semconv/general/metrics/ (listed in search results, not read)
