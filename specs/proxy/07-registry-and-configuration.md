# Registry and configuration

Status: draft for review. Written 2026-10-07. Requirement IDs: PRX-REG-001 to PRX-REG-049 and PRX-SEC-001 to PRX-SEC-016. Style: STE-style (not STE-compliant). Glossary: [15-glossary.md](15-glossary.md).

The registry is one YAML file. It is the only configuration of the proxy that describes nodes, machines and aliases. The proxy reads it at start and at reload. The proxy never writes it.

Engine facts that fill the fields are in [05-engine-behaviour.md](05-engine-behaviour.md). The older design is in the [baseline registry](../baseline/registry.md). This file replaces it for the proxy.

The baseline has a three-layer node card, roles, a `requires` field and capability tiers. None of them is in v1, because v1 parks the capability manager ([16-parked-capability-manager.md](16-parked-capability-manager.md)).

## 1. Principles

- One file, one source of truth for what the owner declares.
- Declared values are the ceiling. Measured values can lower a limit. They never raise it.
- Measured values live in a separate machine-written file. The proxy never edits the file of the owner.
- A secret is a reference. The file never holds a secret value.
- The proxy rejects a bad file with a precise error. It never starts on a half-valid file.
- All state derived from the file is soft state. It is rebuilt at start.

## 2. File structure

The file has these top-level keys. Unknown top-level keys are an error.

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `version` | integer | yes | Schema version. The value is 1. |
| `settings` | map | no | Proxy-wide parameters with defaults. Meaning is in files 03 and 04. |
| `machines` | map of machine | no | Physical hosts. |
| `nodes` | map of node | yes | Endpoints that serve a model. |
| `aliases` | map of alias | yes | Names that harnesses use in the `model` field. |

### 2.1 Settings

The `settings` map holds parameters that other files define. This file lists the keys and the types. It does not define the meaning. Files 03 and 04 write the parameters `window`, `hold_limit` and `table_ttl`. The registry key of a parameter in seconds ends in `_s`.

| Key | Type | Default | Owner file |
| --- | --- | --- | --- |
| `listen` | address | none, required if `settings` exists | [02-architecture.md](02-architecture.md) |
| `hold_limit_s` | integer | 250 PROPOSED. Start-up warning above 290 | [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md) |
| `protected_window_s` | integer | 180 PROPOSED | [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md) |
| `table_ttl_s` | integer | 600 PROPOSED, approved by the owner (DEC-049) | [03-affinity-and-keys.md](03-affinity-and-keys.md) (the parameter `table_ttl`) |
| `probation_window_s` | integer | 30 PROPOSED | [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md) (the parameter `probation_window`) |
| `mature_turns` | integer | 2 PROPOSED (DEC-067, pending owner confirmation) | [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md) |
| `max_held` | integer | 64 PROPOSED | [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md) |
| `hold_limit_status` | integer | 503 | [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md) |
| `key_text_limit_system` | integer | 32768 bytes | [03-affinity-and-keys.md](03-affinity-and-keys.md) |
| `key_text_limit_first` | integer | 8192 bytes | [03-affinity-and-keys.md](03-affinity-and-keys.md) |
| `body_limit_bytes` | integer | NOT SET | [06-protocols-and-harnesses.md](06-protocols-and-harnesses.md) (PRX-PROTO-065) |
| `admin` | map | token file and address | [08-observability-and-admin.md](08-observability-and-admin.md) |
| `client_tokens_ref` | secret reference | absent. Required when any hosted node exists | section 10.2 of this file |

### 2.2 Machine

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `host` | string | no | Host name or address for the node agent. |
| `mem_gb` | number | no | Total memory. Used by findings only. |
| `agent` | URL | no | Address of the node agent that reports health, load and machine readings. |

A machine has no behaviour in v1 beyond the admin read. The node agent reports temperature, thermal throttling and power. The proxy records them and does not act on them. This matches the [scope reset](../decisions/2026-10-scope-reset.md). NOT TESTED.

### 2.3 Node

| Field | Type | Required | Source | Meaning |
| --- | --- | --- | --- | --- |
| `engine.name` | enum | yes | declared | `llama-server`, `ollama`, `mlx_lm`, `vllm`, `sglang`, `gufo`, `lmstudio`, `openai-compatible`, `openai-hosted`, `anthropic-hosted`. |
| `engine.version` | string | yes for local engines | declared, compared | Version tested in [05-engine-behaviour.md](05-engine-behaviour.md). |
| `model` | string | yes | declared | The model name the node expects. The proxy writes it in the `model` field. |
| `quantisation` | string | no | declared | Descriptive. |
| `machine` | machine name | no | declared | Host of the node. |
| `endpoints` | list | yes | declared | Each entry has `protocol` (`openai-chat`, `openai-responses` or `anthropic-messages`) and `base_url`. An `openai-responses` entry needs `responses: true`. Engine support is in [05-engine-behaviour.md](05-engine-behaviour.md) section 14. |
| `responses` | boolean | no | declared | The node serves `/v1/responses`. The default is false. A pool with no such node answers 404 on the path (DEC-065, owner decision pending). |
| `stateful_responses` | boolean | no | declared | The engine keeps response state for `previous_response_id`. The proxy then records `response.id` for the node. The default is false. |
| `ignores_previous_response_id` | boolean | no | declared | The engine accepts and ignores `previous_response_id` (Ollama today). The default is false. |
| `warm_capacity` | integer or `auto` | no | declared, measured | Conversations that the node keeps warm. `auto` (the default) means the slot count, then the calibrated count where the engine has a host cache. See [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md) section 4.5. |
| `paths` | list | no | declared | Other OpenAI-style paths that the node serves, for example `/v1/embeddings`. The default is the empty list. |
| `slots` | integer or `auto` | no | declared, measured | Cap on running requests. `auto` reads the engine. Engines without a signal reject `auto`. |
| `context` | map | no | declared, measured | `per_slot` tokens and `on_overflow`: `error_400`, `silent_truncate`, `unbounded` or `unknown`. |
| `cache` | map | no | declared, measured | `kind`: `dense`, `hybrid` or `unknown`. `prompt_tokens_details`: boolean, for vLLM. |
| `engine_flags` | map | no | declared, compared | Flags the owner declares, such as `jinja`, `np`, `ctx_checkpoints`, `checkpoint_min_step`, `cache_ram_mib`, `num_parallel`, `keep_alive_s`, `kv_unified`, `prefix_caching`, `speculative_decoding`. Findings compare them with probe results. |
| `patch` | map | no | declared | Fixed request edits: `set` (map) and `remove` (list). See section 5. |
| `auth` | map | no | declared | `scheme` (`bearer`, `x-api-key`, `none`) and `key_ref`. Required for hosted engines. |
| `always_on` | boolean | no | declared | When false, a node can sleep and the proxy treats a refused connection as normal. |

The measured profile of a node is not in this file. See section 8.

### 2.4 Alias

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `nodes` | list of node names | yes | The pool for this alias. At least one entry. |
| `affinity` | map | no | `key_headers`: ordered list of header names. `hash_fallback`: boolean, default true. Rules in [03-affinity-and-keys.md](03-affinity-and-keys.md). |
| `description` | string | no | Free text for the admin read. |

The alias name is the value of the `model` field that a harness sends. The set of protocols of an alias is the union of the protocols of its nodes. The proxy sends a request only to a node that speaks the protocol of the request. The proxy computes the set and does not accept it from the file. Changed 2026-10-07: the earlier text used the intersection, which blocks a pool where only some nodes have the `responses` flag.

## 3. Example

The example is compact on purpose. Host names, versions and numbers are illustrative. The `measured` block is not part of the registry file. The proxy writes it to the profile file, and the example shows it merged into one view.

```yaml
version: 1
settings:
  listen: 0.0.0.0:8080
  hold_limit_s: 250
  protected_window_s: 180
  client_tokens_ref: file:/etc/legatus/clients.txt
machines:
  strix: { host: strix.local, mem_gb: 128, agent: "http://strix.local:9100" }
  mini:  { host: mini.local,  mem_gb: 16 }
nodes:
  strix-qwen:
    engine: { name: llama-server, version: b11146 }
    model: qwen3.8-27b-q4
    machine: strix
    endpoints: [ { protocol: openai-chat, base_url: "http://strix.local:8081" } ]
    slots: auto
    context: { per_slot: 65536, on_overflow: error_400 }
    cache: { kind: hybrid }
    responses: false
    engine_flags: { jinja: true, np: 4, ctx_checkpoints: 32, checkpoint_min_step: 8192, cache_ram_mib: 8192 }
    patch: { set: { chat_template_kwargs: { enable_thinking: false } } }
    measured:                       # written by the proxy, not by the owner
      probed_at: "2026-10-07T09:00:00Z"
      concurrency_useful: 4
      per_slot_context: 65536
      reuse_field: timings.cache_n
      checkpoint_distances: [516, 4]   # spike 7 values for Qwen3.5-2B, not measured for this model
      warm_capacity: 4                 # equals slots until a swap test shows more
  mini-ollama:
    engine: { name: ollama, version: 0.35.1 }
    model: qwen3:4b
    machine: mini
    endpoints: [ { protocol: openai-chat, base_url: "http://mini.local:11434" } ]
    slots: 1
    context: { per_slot: 8192, on_overflow: silent_truncate }
    patch: { set: { reasoning_effort: none } }
    always_on: false
  claude-hosted:
    engine: { name: anthropic-hosted }
    model: claude-example-model
    endpoints: [ { protocol: anthropic-messages, base_url: "https://api.anthropic.com" } ]
    auth: { scheme: x-api-key, key_ref: env:ANTHROPIC_API_KEY }
aliases:
  local-coder:
    nodes: [strix-qwen, mini-ollama]
    affinity: { key_headers: [x-session-affinity, x-claude-code-session-id] }
  frontier:
    nodes: [claude-hosted]
```

## 4. Aliases and names

- PRX-REG-001: The proxy must load the registry from the path given at start.
- PRX-REG-002: The proxy must reject a registry file that has no `version` or whose `version` is not 1.
- PRX-REG-003: The proxy must reject a field that the schema does not list, at every level.
- PRX-REG-004: The proxy must route a request by the exact value of its `model` field, matched to an alias name.
- PRX-REG-005: The proxy must answer a request with an unknown `model` value with the unknown-model error of [06-protocols-and-harnesses.md](06-protocols-and-harnesses.md).
- PRX-REG-006: The proxy must write the `model` value of the node into the request before it forwards the request.
- PRX-REG-007: The proxy must compute the protocols of an alias as the union of the protocols of its nodes. Changed 2026-10-07.
- PRX-REG-008: The proxy must reject an alias whose protocol set is empty.

Other OpenAI-style paths (embeddings, audio, images) pass through by path to nodes that serve them (DEC-004, approved). The proxy picks a node for such a path by the `model` field. The Responses API path `/v1/responses` is in v1 as pass-through to a node with the flag `responses` (DEC-062 and DEC-065).

## 5. Patches

A patch changes the request body per node. The registry fixes the patch. The patch applies to every request that goes to the node. This keeps the prompt bytes identical on every turn, because a changed prompt breaks the cache of the node ([05-engine-behaviour.md](05-engine-behaviour.md) section 13).

Operations:

- `set` writes a top-level key. The value can be a map. A map under `set` merges, key by key, into a map that the request already has.
- `remove` removes a top-level key.

Examples that the spikes proved: `chat_template_kwargs.enable_thinking` on llama-server, `reasoning_effort` on Ollama (S4). A cap on `max_tokens` is a PROPOSED patch.

- PRX-REG-009: The proxy must apply the `set` and `remove` operations of a node patch to every request to that node.
- PRX-REG-010: The proxy must apply a patch after it rewrites the `model` field.
- PRX-REG-011: The proxy must reject a patch that names `messages`, `tools`, `system`, `model`, `stream` or `functions`.
- PRX-REG-012: The proxy must not apply a patch to a path other than chat completions and Messages.
- PRX-REG-013: The proxy must apply the same patch result to a request regardless of the conversation.
- PRX-REG-038: The proxy must forward a request on another OpenAI-style path to a node of the alias that lists the path in `paths`. The proxy must change only the `model` value.
- PRX-REG-039: The proxy must answer a request on a path that no node of the alias lists with the 404 error of [06-protocols-and-harnesses.md](06-protocols-and-harnesses.md).

## 6. Validation

The proxy validates the whole file before it uses any part. It reports every error found, not only the first. An error has the form: `registry error <code> at <path>: <text>`. The path is a dotted path such as `nodes.mini-ollama.slots`.

| Code | Condition | Text |
| --- | --- | --- |
| `unknown_field` | Field not in the schema | Field is not part of the schema. |
| `bad_version` | `version` missing or not 1 | Schema version 1 is the only version. |
| `bad_type` | Wrong type or enum value | Expected `<type>`, found `<value kind>`. |
| `duplicate_name` | Two nodes, machines or aliases share a name | Two entries use this name. |
| `unknown_ref` | Alias names a missing node, or a node names a missing machine | Name does not exist. |
| `empty_alias` | Alias has no nodes | An alias needs at least one node. |
| `no_common_protocol` | Protocol set of an alias is empty | Nodes of this alias share no protocol. |
| `bad_url` | A `base_url` does not parse or has a scheme other than http or https | URL is not valid. |
| `bad_slots` | `slots` is below 1, or `auto` on an engine with no signal | Engine has no slot signal. Declare a number. |
| `context_missing` | `on_overflow` is `silent_truncate` and `per_slot` is absent | The truncation guard needs the loaded context. |
| `bad_patch` | A patch names a forbidden key | Patch must not change tokens of the prompt. |
| `inline_secret` | A field named `key`, `token`, `password` or `secret` holds a value | Use `key_ref`. |
| `bad_key_ref` | `key_ref` has an unknown form | Use `env:NAME` or `file:/path`. |
| `secret_unresolved` | The named variable or file does not exist or is empty | Secret reference cannot be read. |
| `secret_permissions` | A secret file is readable by group or others | Set mode 0600. |
| `hosted_needs_auth` | A hosted engine has no `auth.key_ref` | Hosted node needs a key reference. |
| `hosted_needs_client_tokens` | A hosted node exists and `client_tokens_ref` is absent | A hosted node needs a client token reference. |
| `bad_settings` | A setting is out of range | Value is outside the allowed range. |
| `bad_responses_flag` | An endpoint has the protocol `openai-responses` and the node has `responses` false, or a node sets `stateful_responses` and `ignores_previous_response_id` both true | Responses flags do not agree. |
| `bad_warm_capacity` | `warm_capacity` is below 1, or below the declared `slots` | Warm capacity cannot be lower than the slots. |

The error text never contains a secret value or a prompt.

Warnings do not stop the load. They appear in the admin read and in the log.

| Code | Condition |
| --- | --- |
| `engine_version_untested` | The engine version differs from the version in [05-engine-behaviour.md](05-engine-behaviour.md). |
| `flag_mismatch` | A declared flag differs from the probe result. |
| `slots_above_measured` | Declared slots are above measured useful concurrency. |
| `responses_engine_mismatch` | A node of engine `mlx_lm` has `responses` true. The engine answers 404 on `/v1/responses` ([05-engine-behaviour.md](05-engine-behaviour.md) section 14). |

- PRX-REG-014: The proxy must validate the whole registry file before it applies any part of it.
- PRX-REG-015: The proxy must report every validation error with a code, a path and a text.
- PRX-REG-016: The proxy must refuse to start when the registry has an error, and must exit with a non-zero status.
- PRX-REG-017: The proxy must keep error texts free of secret values and of prompt text.
- PRX-REG-018: The proxy must report each warning in the admin read and in the event log.

## 7. Reload

The proxy reloads the registry on the signal SIGHUP. PROPOSED. The admin API gives read access only, so it has no reload action. The proxy does not watch the file. A watch can read a half-written file.

Rules:

1. The proxy parses and validates the new file in full.
2. If the file has an error, the proxy keeps the old registry, reports the errors, and continues to serve.
3. If the file is valid, the proxy swaps the registry in one step.
4. A request in flight keeps the node record it started with.
5. A new request uses the new registry.

Effects of a change:

| Change | Effect |
| --- | --- |
| Node added | The node joins. The probe runs. The node takes requests when the probe ends, or as `uncalibrated` by declared values. |
| Node removed | The node takes no new request. Requests in flight finish. The proxy removes the affinity entries for the node. A conversation of that node moves at its next turn and pays a full prefill. |
| Alias changed | Takes effect for the next request. Affinity entries that point to nodes still in the alias stay. |
| Engine version, model, context, patch or flags changed | The probe runs again for that node. Old measured values stay in use until the probe ends. |
| Secret reference changed | The proxy reads the secret again. |
| `listen` changed | Needs a restart. The proxy reports `restart_required` and keeps the old address. |
| Machine changed | Takes effect at once. No probe. |

- PRX-REG-019: The proxy must reload the registry when it receives SIGHUP.
- PRX-REG-020: The proxy must keep the old registry and keep serving when a reloaded file has an error.
- PRX-REG-021: The proxy must swap the registry in one step after a successful validation.
- PRX-REG-022: The proxy must let a request in flight finish with the node record it started with.
- PRX-REG-023: The proxy must remove the affinity entries of a node that a reload removes.
- PRX-REG-024: The proxy must run the calibration probe again for a node after a reload. This applies when the reload changes the engine version, model, context, patch or declared flags of the node.
- PRX-REG-025: The proxy must report `restart_required` for a changed key that needs a restart.

## 8. Declared and measured values

The calibration probe ([05-engine-behaviour.md](05-engine-behaviour.md) section 12) measures a node at join and on change. The proxy writes the result to a profile file next to the registry. The file is JSON, one record per node, machine-written. The file is a cache. If it is absent, the proxy runs the probe. The proxy compares the stored engine version, model, context and patch with the registry, and discards a record that does not match.

The stricter value wins:

| Field | Declared | Measured | Value in use |
| --- | --- | --- | --- |
| Slots and concurrency | cap | useful concurrency | the smaller number |
| Context per slot | tokens | effective tokens | the smaller number |
| `on_overflow` | one of four | one of four | the more dangerous, by the order in the note below |
| `cache.kind` | `dense`, `hybrid`, `unknown` | observed reuse shape | `hybrid` wins over `dense` when they differ |
| Reuse field | none | found field | the found field |
| Warm capacity | integer or `auto` | calibrated count of warm conversations | the smaller number when declared. The calibrated count, within the state size bound, when `auto` |
| Speeds and times | none | measured | measured |

The measured values only lower a limit. A measured concurrency above the declared cap does not raise the cap. Declared slots are the ceiling that the owner accepts, as S3 found that declared slots are only a ceiling ([S3](evidence/spikes/s3-slots-README.md) section "RECOMMENDED effectiveSlots").

- PRX-REG-026: The proxy must use the smaller of the declared and the measured value for slots.
- PRX-REG-027: The proxy must use the smaller of the declared and the measured value for context per slot.
- PRX-REG-028: The proxy must use the more dangerous of the declared and the measured `on_overflow` value, in the order given below the table.
- PRX-REG-029: The proxy must not raise a declared limit because of a measured value.
- PRX-REG-030: The proxy must flag a node in the admin read when a measured value is stricter than the declared value.
- PRX-REG-031: The proxy must write measured values only to the profile file and must not edit the registry.
- PRX-REG-032: The proxy must discard a stored profile whose engine version, model, context or patch differs from the registry.
- PRX-REG-033: The proxy must set `cache.kind` to `hybrid` when the probe shows hybrid reuse behaviour, even if the registry says `dense`.
- PRX-REG-040: The proxy must read the node flags `responses`, `stateful_responses` and `ignores_previous_response_id`. Each default is false.
- PRX-REG-041: The proxy must reject an `openai-responses` endpoint on a node with `responses` false, and a node with `stateful_responses` and `ignores_previous_response_id` both true.
- PRX-REG-042: The proxy must read `warm_capacity` as an integer or `auto`, with the default `auto`, and must reject a value below the declared `slots`.
- PRX-REG-043: The proxy must use the smaller of the declared `warm_capacity` and the calibrated count when both exist. When the value is `auto`, it must use the slot count until the probe shows more warm conversations.
- PRX-REG-044: The proxy must warn for a node of engine `mlx_lm` with `responses` true.
- PRX-REG-045: The proxy must read the engine flag `prefix_caching` of a node. The values are `true`, `false` and `unknown`, and the default is `unknown`. The proxy must show the flag in the admin read.
- PRX-REG-046: The proxy must read the engine flag `speculative_decoding` of a node. The values are `true`, `false` and `unknown`, and the default is `unknown`. The proxy must show the flag in the admin read.
- PRX-REG-047: The proxy must report a warning at load for each vLLM node that has no `cache.prompt_tokens_details` value.
- PRX-REG-048: The proxy must take the idle time of an affinity table entry only from the setting `table_ttl_s`. It must hold no other constant for it.
- PRX-REG-049: The proxy must ignore `slots` and `warm_capacity` on a hosted node. It must report a warning at load when a hosted node sets either field.

The order of danger for `on_overflow` is `silent_truncate`, `unbounded`, `unknown`, `error_400`. A silent cut is first, because it gives a wrong answer with no error. The proxy applies this order.

## 9. Evidence for calibration thresholds: the smoke result of spike 4

The proxy is not a capability manager, so it does not accept or refuse a node for a task. The calibration can still record how reliably a node makes tool calls. This section uses the spike result only as evidence for the thresholds of that record. Every value has the label PROPOSED. The record is informational and never removes a node from an alias.

Evidence ([S4](evidence/spikes/s4-smoke-README.md)), real models on an Apple M2 with 16 GB, 20 trials per case:

| Fact | Value |
| --- | --- |
| Specified pass rate before the spike | 0.95 |
| Smallest tested model that cleared 0.90 reliably | qwen3:4b on Ollama, 0.94 and 0.95 on the common five cases |
| qwen3:1.7b with thinking on | 0.88 to 0.96, depends on temperature and engine, so not stable |
| Same GGUF, thinking off, Ollama versus llama-server | 0.70 versus 0.82 on the common five cases |
| Effect of thinking, qwen3:1.7b on Ollama | 0.70 to 0.96, at 4 times the time |
| Pass chance at a 0.95 threshold with 8 calls per behaviour and a true rate of 0.95 | 54 percent |
| Pass chance at a 0.90 threshold for the same true rate | 96 percent |
| Pass chance at a 0.90 threshold when the true rate is 0.88 | 44 percent |

What the evidence implies for a record of this kind (PROPOSED):

- Use a threshold of 0.90 and at least 20 calls per behaviour.
- Report a Wilson interval. Mark a result inside the interval of the threshold as `borderline`.
- Add a per-behaviour floor of 0.70.
- Test schema-valid arguments, with a floor of 1.0.
- Test a two-step chain, where the second call needs the result of the first. Single-call tests did not expose a 0 of 20 chain result.
- Judge the content of the answer. Thinking can leak into `content` with HTTP 200.
- Key the record by engine, thinking mode, template setting, context and sampling. The same weights gave different results on two engines.

NOT TESTED: MLX, vLLM, SGLang, models above 4 billion parameters, large argument copy for the 4B model.

- PRX-REG-034: The proxy must key a stored smoke record by engine, version, model, thinking setting, template setting, context and sampling.
- PRX-REG-035: The proxy must use at least 20 calls per behaviour in a smoke record. PROPOSED.
- PRX-REG-036: The proxy must mark a smoke result as `borderline` when the Wilson interval contains the threshold. PROPOSED.
- PRX-REG-037: The proxy must not remove a node from an alias because of a smoke result.

## 10. Secrets and key custody

The key of a hosted node stays in the proxy. The harness holds no key for a hosted node. This follows the decision on key custody (see [01-decisions.md](01-decisions.md)).

A secret reference has one of two forms:

- `env:NAME` reads an environment variable of the proxy process.
- `file:/absolute/path` reads a file, trims one newline at the end, and requires mode 0600 or stricter.

The proxy reads references at start and at reload. It keeps the value in memory only. It never writes the value to a log, a profile file, an error text or the admin read.

### 10.1 Headers before a node

The proxy removes inbound headers before it sends a request to any node. The headers are `authorization`, `x-api-key`, `api-key`, `proxy-authorization` and `cookie`. The proxy also removes every header whose name starts with `x-legatus-`. For a node with an `auth` field, the proxy then adds the node credential. A hosted node must have an `auth` field.

The proxy also removes the session headers that the alias lists in `key_headers`, and `x-claude-code-agent-id`, before a hosted node. PROPOSED. The reason is that a session identifier of a person goes to a third party if the proxy forwards it. The proxy needs these headers for its own affinity and does not need them at the node. Other headers pass unchanged, because Claude Code sends about 11 beta headers that Anthropic needs (round 1, [decisions](../decisions/2026-10-spike-decisions.md) section 3.6).

### 10.2 Who can spend the key

A hosted key behind a proxy on a local network lets any program that reaches the proxy spend the key. The owner decided on 2026-10-07 to close this gap (approved).

The registry holds `client_tokens_ref`, a reference to a file with one accepted client token per line. When the registry defines a hosted node, the registry must have `client_tokens_ref`. A registry with a hosted node and no `client_tokens_ref` is an error (`hosted_needs_client_tokens`). The proxy then requires a client token on every request to the model port, for local and hosted aliases alike.

The token has the form of a bearer credential. The harness already sends it as its API key, in the `authorization` header or the `x-api-key` header. The proxy removes this header before it sends the request to a node (section 10.1). A harness that cannot send a configured token cannot reach the proxy.

A registry with local nodes only needs no token. Such a proxy stays open on the trusted network. NOT TESTED: a harness with no key setting. The token rule does not apply to the admin API, which has its own token (see [08](08-observability-and-admin.md)).

### 10.3 Requirements

- PRX-SEC-001: The proxy must accept a secret only as a reference of the form `env:NAME` or `file:/absolute/path`.
- PRX-SEC-002: The proxy must reject a registry that holds a secret value inline.
- PRX-SEC-003: The proxy must require mode 0600 or stricter on a secret file.
- PRX-SEC-004: The proxy must keep secret values in memory only.
- PRX-SEC-005: The proxy must not write a secret value to a log, an error text, a profile file or the admin read.
- PRX-SEC-006: The proxy must remove the inbound headers `authorization`, `x-api-key`, `api-key`, `proxy-authorization` and `cookie` before it forwards a request to any node.
- PRX-SEC-007: The proxy must remove every header that starts with `x-legatus-` before it forwards a request to any node.
- PRX-SEC-008: The proxy must add the credential of the node from `auth.key_ref` to each request to a node that has an `auth` field.
- PRX-SEC-009: The proxy must refuse to load a hosted node without `auth.key_ref`.
- PRX-SEC-010: The proxy must remove the session headers of the alias and `x-claude-code-agent-id` before it forwards a request to a hosted node. PROPOSED.
- PRX-SEC-011: The proxy must not forward a response header or body to a harness that contains the key of a node.
- PRX-SEC-012: The proxy must read secrets again at reload.
- PRX-SEC-013: The proxy must require a client token on every request to the model port when the registry defines a hosted node.
- PRX-SEC-014: The proxy must show in the admin read the name of each secret reference and whether it resolved, and never the value.
- PRX-SEC-015: The proxy must refuse to load a registry that defines a hosted node and has no `client_tokens_ref`, with the error code `hosted_needs_client_tokens`.
- PRX-SEC-016: The proxy must accept requests with no client token when the registry defines no hosted node.

## 11. Open items and risks

- RISK: key custody lets any program that reaches the proxy spend the key. The client token rule (PRX-SEC-013 to PRX-SEC-016) limits who spends the key. Any holder of a token can still spend the whole key, because v1 has no budget (DEC-008).
- OPEN: the form of keychain references on macOS (`keychain:service`). Not in v1.
- OPEN: whether SIGHUP is enough for the target users, or the proxy needs a watch.
- OPEN: where the profile file lives and how a restart treats a stale record. This file says discard on mismatch.
- OPEN: the path list for the passthrough of other OpenAI-style paths. See OPEN-004 in [14-open-questions-and-risks.md](14-open-questions-and-risks.md).
- OPEN: whether the proxy forwards the harness session headers to hosted nodes. PRX-SEC-010 is a proposal. A hosted provider can use a session header for its own cache routing. NOT TESTED.

## Sources

- [Baseline registry](../baseline/registry.md) (older design, partly reused)
- [Spike decisions](../decisions/2026-10-spike-decisions.md) sections 3.5 to 3.8, 4
- [Scope reset](../decisions/2026-10-scope-reset.md)
- [S3 slots](evidence/spikes/s3-slots-README.md), [S4 smoke](evidence/spikes/s4-smoke-README.md)
- [05-engine-behaviour.md](05-engine-behaviour.md), [R4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md)
