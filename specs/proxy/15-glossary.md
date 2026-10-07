# Glossary

Status: draft, 2026-10-07. Owner of this file: writer W-A. The final pass merged the terms of the other files.

Use each term in one meaning. Do not use the forbidden synonyms. This glossary allows a technical name that ends in -ing (for example "routing") because it defines the name.

## 1. Terms

| Term | Definition | Do not write |
|---|---|---|
| admin API | The read-only HTTP API of the admin process. It has a version in the path and needs a token. | none |
| admin process | The small process, separate from the proxy, that serves the admin API and the dashboard. | none |
| admin read | An answer of the admin API. | none |
| admission control | The rule that limits the running requests on a node to its cap. | throttling (except thermal throttling), rate limiting, load shedding |
| affinity | The rule that keeps a conversation on one node. | stickiness (except in "strict stickiness"), pinning, session binding |
| affinity key | The value that identifies a conversation. It comes from a header or from a hash of the first messages. | session id, conversation id, pin key |
| affinity table | The in-memory map from an affinity key to a node and a last-seen time. | pin table, session map, routing table |
| alias | The name in the `model` field of a request. It names a pool of nodes. | role, virtual model, model group |
| `--cache-ram` | The llama-server setting for the memory budget of the host cache. | none |
| calibration | The steps that measure a node at join and at each change. The calibration probe runs them. | none |
| cap | The largest number of running requests that the proxy allows on one node. | limit, concurrency limit, max slots |
| cap place | One unit of the cap of a node. A running request holds one cap place from its start until its end. The proxy frees the cap place when the request ends for any reason. | slot (in this meaning) |
| capability manager | The parked layer with a custom protocol, leases and capability discovery. It is not in v1 (see file 16). | none |
| checkpoint | A recurrent state that an engine saves at one token position, so that a later request can restart from it. | none |
| client token | A secret value that a harness sends to the proxy to show that it can use the proxy. Required when the registry defines a hosted node. | client key, API key (in this meaning) |
| cluster | All the machines and nodes that one proxy serves. It is not a pool. | none |
| cold prefill | The prefill of the whole prompt, because the node has no reusable cache for it. | none |
| cold turn | A turn after the first turn of a conversation where the node reuses little or none of the cached prompt. | cache miss, re-prefill |
| cold-turn rate | The share of turns that are cold turns. | none |
| compaction | A call of a harness that replaces the history of a conversation with a summary. | none |
| conversation | A series of requests that share one affinity key. | session, chat, thread |
| dashboard | The page of the admin process. It shows values from the admin API. | none |
| de facto standard | A format that many products use and that no standards body owns. | standard (alone) |
| engine | The program that runs the model on a node. Examples are llama-server, Ollama and mlx_lm. | server, backend, runtime |
| engine eviction | The removal of the cache state of a conversation by an engine. | none |
| event log | The files of JSON lines that hold one event for each request. The events hold metadata only. | none |
| evict | To take the seat of an idle conversation and give it to another conversation. | none |
| failover | The send of the same request again to another node after a failure. It is not in v1. It is not a move. | none |
| finding | A report in the admin read that a declared value and an observed value differ. A finding is advisory. | none |
| fixture | A defined test input or scenario. The ID has the form FIX-NNN. | none |
| harness | A program that calls the proxy for a person. Examples are pi and Claude Code. | client, agent, tool |
| harness label | A short name of a harness, for example `pi`, that the proxy writes in each event. | none |
| hold | The state of a request after the proxy accepts it and before the proxy sends it to a node. | queue (as a verb), park, buffer |
| hold limit | The longest time that the proxy holds one request. | queue timeout, wait limit |
| hold queue | The set of held requests. | waiting room, backlog |
| hold-limit error | The error that the proxy sends when a request waits for the hold limit. | none |
| host cache | The memory of the host in which an engine saves the state of an idle slot. | none |
| hosted node | A node that is a remote paid API. The proxy keeps its key. | remote node, cloud node, frontier node |
| hybrid model | A model with both recurrent layers and full attention layers. | linear model, SSM model |
| in-flight count | The number of requests that the proxy sent to a node and that the node did not finish. | active count, load |
| key class | The class of an affinity key. STRONG comes from a header. DERIVED comes from two message parts. WEAK comes from one part or from the credential. | none |
| key map | The per-route setting that names the headers that give the affinity key. | header config |
| local node | A node that runs an engine on a machine of the owner. | self-hosted node, on-prem node |
| machine reading | A value that a node agent reports about a machine. Examples are temperature, thermal throttling and power. | none |
| measured profile | The values that the proxy measures for a node in a calibration. | benchmark result, node card |
| move | The change of the node of a conversation. | migration, rebalance, failover (as a word for a move) |
| node | One endpoint that serves one model, with one engine on one machine. | backend, upstream, worker, server |
| node agent | A small program on a machine that reports health, load and machine readings to the proxy. | none |
| passthrough | The forwarding of a request and its response with no change except the `model` field and the headers that this specification names. | proxying (alone), relay |
| patch | A fixed change that the proxy applies to the request for one node. | override, transform |
| placement | The choice that the proxy makes for a request, with its kind. The event log records the kind. | none |
| pool | The set of nodes that one alias names. | cluster (as a word for a pool), group |
| prefill | The work of an engine to read the prompt tokens before it writes the first output token. | prompt processing, ingest |
| probation window | The short protected window for a conversation with fewer than two successful requests and for every key of class WEAK. | grace period |
| probe | A request that the proxy sends to a node for a reason other than a harness request. A calibration probe measures the node. A health probe checks that the node is up. | none |
| prompt cache | The saved state that lets an engine skip the prefill of a prompt prefix that it processed before. | KV cache (for the feature), prefix cache |
| protected window | The time after which an idle conversation can lose its seat. | grace period, idle timeout |
| proxy | The Legatus program. Use "the proxy" for it. | router, gateway, dispatcher, balancer |
| queue wait | The time from the arrival of a request to its dispatch to a node. Written `queue_wait_ms`. It is zero when the request is not held. | none |
| recomputed tokens | The prompt tokens that an engine reads again in one turn, because it did not reuse them. | none |
| registry | The file that lists nodes, aliases and patches. | config, node table, inventory |
| request | One HTTP call from a harness to the proxy. | call, query, message |
| Responses API | The OpenAI API at `POST /v1/responses`. Codex uses it. It is in v1 as pass-through only, with no translation (DEC-062, DEC-065). | none |
| `responses` flag | The node setting that says the node serves `/v1/responses`. The default is false. | none |
| reuse | The count of prompt tokens that an engine did not read again. The response fields of the node report it. | none |
| rolling reuse ratio | The ratio of reused tokens to prompt tokens over a window of recent turns of one node. | none |
| route | A rule that matches a request path and gives it a key map and a pool. | endpoint (for a rule), mapping |
| running request | A request that a node is working on. | active request, busy request |
| seat | The reservation of one warm conversation on a node. A node has `warm_capacity` seats. | slot (in this meaning) |
| session header | A request header that a harness sends to name its session. It is the first source of an affinity key. | none |
| sibling spill | The send of a request with a DERIVED key to another node. It happens when its node is at the cap and another request of the same key is in flight. | none |
| side request | A request in a conversation that the harness sends for another purpose. Examples are a title and a summary. | auxiliary request, helper call |
| slot | A unit of concurrency in an engine. One slot serves one running request. | lane, worker, thread |
| smoke test | A short set of tool-call requests that checks the tool-call behaviour of a node. | none |
| soft state | State that the proxy loses at restart and that traffic rebuilds. | cache (for state), ephemeral state |
| spike 7 | The measurement of the hybrid cache on this machine with llama-server and Qwen3.5-2B (2026-10-07). Also written S7. | none |
| spike B | The script that the owner runs on the Strix Halo and on the M5 Max (DEC-060). | none |
| spike C | The measurement of key stability in real harness traffic against a fake server (2026-10-07). | none |
| spike D | The measurement of harness behaviour at the hold limit, with statuses, retries and errors, against a fake server (2026-10-07). | none |
| spike F | The test with real Codex sessions on real engines. The owner decision is pending. | none |
| strict stickiness | The rule that a conversation moves only when its node is unavailable. | hard affinity, pinning |
| stub engine | A program of the test kit that copies the HTTP behaviour of one engine. | none |
| table entry | One row of the affinity table. | none |
| thermal throttling | The slowing of a machine because of heat. A node agent reports it as a machine reading. It is not admission control. | none |
| time to first token | The time from the arrival of a request to the first output token. | first token latency, TTFT metric |
| time to head | The time from admission until the response head leaves the node. Written `t_head`. | none |
| turn | One request and its response in a conversation. | round, step |
| virtual time | Time that a test moves with a driver and not with the clock of the machine. | none |
| warm | A conversation is warm on a node when the node still holds its prompt cache. | none |
| warm capacity | The count of conversations that a node keeps warm at one time. It is the registry value `warm_capacity`. The default is the slot count. It is larger when the engine keeps idle state in host memory and the probe shows it. | cache size, capacity (alone) |
| warm slot | A slot that holds the prompt cache of a conversation. A conversation can also stay warm in host memory (see warm capacity). | hot slot, reserved slot |
| warm swap | The exchange on one engine slot of the saved state of two conversations through the host cache, so that both conversations stay warm. | none |
| working directory | The directory in which a harness runs. Its path can be in the system text. | none |
| wrapper-only message | A message whose text parts all start with a wrapper tag such as `<environment_context>`. The key rule joins it to the next message. | none |

## 2. Labels

| Label | Meaning |
|---|---|
| PROVEN | Measured, or read from source, with a reference. |
| PROPOSED | A design choice or default. Nobody measured it. |
| ASSUMPTION | Believed. Nobody checked it. |
| NOT TESTED | A claim that needs a test. Nobody ran a test. |
| NOT MEASURED | A value that nobody measured. |
| NOT SET | A value that the owner must choose. |
| NOT TRANSFERABLE | A measured value that does not carry to another model, engine or machine. Speeds and sizes of spike 7 have this label for the large models. |
| OPEN | A question that nobody decided. |

## 3. Status words in the decision log

| Status | Meaning |
|---|---|
| decided by owner | A recorded owner text supports the decision. |
| proposed by coordinator | The coordinator chose the decision. The owner did not approve it yet. |
| pending owner confirmation | A default that the text applies until the owner approves or changes it. |
| open | Nobody decided. |

## 4. ID prefixes

| Prefix | Meaning | Owner file |
|---|---|---|
| DEC-NNN | A decision | [01-decisions.md](01-decisions.md) |
| VIS-NNN | A measurable claim of the vision | [00-vision-and-scope.md](00-vision-and-scope.md) |
| PRX-AREA-NNN | A requirement. AREA is SCOPE, PROTO, ROUTE, KEY, AFF, ADM, ENG, REG, OBS, REST, SEC, TEST or PERF. | The file of the area |
| EDGE-AREA-NNN | An edge case | [12-edge-cases.md](12-edge-cases.md) |
| FIX-NNN | A test fixture or scenario | [13-test-fixtures-and-scenarios.md](13-test-fixtures-and-scenarios.md) |
| LES-NNN | A lesson | [11-lessons-learned.md](11-lessons-learned.md) |
| RISK-NNN, OPEN-NNN | A risk or an open question | [14-open-questions-and-risks.md](14-open-questions-and-risks.md) |

## 5. Abbreviations

| Abbreviation | Meaning |
|---|---|
| API | Application programming interface |
| ASD-STE100 | The Simplified Technical English specification of the AeroSpace and Defence Industries Association of Europe |
| HTTP | Hypertext Transfer Protocol |
| KV | Key and value. The attention state that an engine saves for each token. |
| MTP | Multi-token prediction |
| OTel | OpenTelemetry |
| RSS | Resident set size. The physical memory that a process uses. |
| SSE | Server-sent events. The format of a streamed response. |
| STE | Simplified Technical English |
| TTL | Time to live |
| TTFT | Time to first token. Write the full term in prose. |

## 6. Technical names allowed with an -ing ending

This specification defines these words as terms: affinity, caching, checkpointing, logging, queueing, routing, scheduling, streaming, batching, prefilling, decoding, thinking, reasoning, serving, loading.

It also defines these names: finding, rolling, sliding window, working directory and thermal throttling.

| Name | Definition |
|---|---|
| routing | The choice of a node for a request. |
| caching | The saving of prompt state by an engine. |
| streaming | The sending of a response in parts as SSE. |
| checkpointing | The saving of a recurrent state at a token position, so that a later request can restart from it. |
| queueing | The waiting of a request in a queue of an engine or of the proxy. |
| thinking | The output of a model before its answer, in a reasoning model. |
| finding | A report in the admin read (see the term). |
| rolling | The word in "rolling reuse ratio". It means that the window moves with each turn. |
| sliding window | A layer type of a model that reads only a fixed number of recent tokens. |
| working directory | The directory in which a harness runs (see the term). |
| thermal throttling | The slowing of a machine because of heat (see the term). |

## Sources

- [Vision and scope](00-vision-and-scope.md).
- [Decision log](01-decisions.md).
- [r1 models](evidence/research/r1-models-20261007T022035Z.md) for the hybrid model terms.
- [r4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md) for the engine terms.
