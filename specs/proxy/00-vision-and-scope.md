# Vision and scope

Status: draft, 2026-10-07. Owner of this file: writer W-A. Language: STE-style (see [README](README.md)).

This file states the vision of Legatus v1, tests it, and bounds it. The vision is the owner's. The critique, the measurable claims and the bounds are the coordinator's proposals. Every unproven statement carries a label: PROVEN, PROPOSED, ASSUMPTION or NOT TESTED.

## 1. The vision (owner's words)

"Do one thing exceptionally well."

1. An extremely efficient LLM router and proxy that does better than anything else at routing to other endpoints, local or remote.
2. Standards-based integration: it works with most things by using existing standards.
3. Optimised specialisation: where extra tuning is possible, build it in, so that LLM endpoints are configured optimally.

The vision is Legatus v1. Legatus v1 is a reverse proxy. It is not a capability manager (see [DEC-002](01-decisions.md) and [the parked manager](16-parked-capability-manager.md)).

## 2. What v1 is

Legatus v1 is one program. It receives requests from current harnesses. It sends each request to one LLM endpoint. It sends the response back.

The harness needs no change. The harness sends an OpenAI chat completions request, an OpenAI Responses request (Codex) or an Anthropic Messages request. The owner decided on 2026-10-07 that v1 supports Codex ([DEC-062](01-decisions.md)). The proxy passes the Responses path through with no translation. A node needs the flag `responses` ([DEC-065](01-decisions.md), owner decision pending).

The `model` field names a pool of nodes, called an alias. The proxy picks one node from the pool.

The proxy has three jobs that other proxies do not do together. It keeps a conversation on one node, so that the prompt cache of the node stays warm. It limits the running requests on each node, so that a node works at its efficient concurrency. It holds a request when no seat is free. It does not refuse the request.

These three jobs are the product. Section 3 tests whether they are worth the cost.

## 3. Critique of the vision

The vision has three weak points. Each weak point has a fix in this file. The fix is not a change of the vision. The fix is a rule for how to read the vision, so that each part can fail a test.

### 3.1 "Better than anything else" is not a claim

"Better" has no metric. "Anything else" has no list. A statement with no metric and no list cannot fail. A statement that cannot fail does not guide design.

The fix is a set of measurable claims. Each claim names one metric, one baseline set and one method. The claims are in section 5. The benchmark method is in section 6. The scenarios and fixtures are in [the test fixtures file](13-test-fixtures-and-scenarios.md).

Nothing in this folder is a benchmark result. All claims in section 5 have the status NOT TESTED. The only measured numbers come from the earlier router prototype. It added 0.06 ms to the first byte and 0.05 ms per SSE chunk. It used 3 to 6 MB resident memory (PROVEN for the prototype, see [spike record, section 3.2](../decisions/2026-10-spike-decisions.md)). The prototype had no hold queue and no hash key.

The numbers do not carry over to this proxy without a new measurement.

A limit on "better": a stub engine measures the routing logic, not the engine. A claim about the time to first token or about cold turns is valid only with a real engine. Spike 7 is the first real measurement. It ran llama-server on a 2B hybrid model and a dense control on one machine ([spike 7](evidence/spikes/s7-hybrid-cache/README.md)). It supports the cost model. It does not measure the proxy or the large models.

### 3.2 "Locally or remote" means local first

Hosted endpoints need things that a local router does not need. They need price tracking, budget caps, rate limit handling, retries and failover to another provider. The owner removed price and budget and deferred retries and failover ([DEC-008](01-decisions.md), [DEC-009](01-decisions.md)).

So v1 reads "locally or remote" as follows. Local engines come first. A remote endpoint is plain passthrough with key custody in the proxy. The proxy has no budget, no failover and no retry for a remote endpoint. A remote error reaches the harness unchanged.

This meaning has the label PROPOSED. It is a smaller claim than the vision. The vision text does not change. The claim for remote endpoints in section 5 has the same bound.

### 3.3 "Standards-based" means de facto standards

There is no standard for the LLM request itself. OpenAI chat completions, OpenAI Responses and Anthropic Messages are vendor APIs. Each vendor can change them without notice. Other servers copy them in part, so a "compatible" server often differs in small ways (see [LES entries](11-lessons-learned.md)).

There is also no standard for affinity. Each product uses its own header. Table 1 lists the headers that the research found.

| Header or field | Who sends or reads it | Status | Source |
|---|---|---|---|
| `x-session-affinity` | pi 1.0.3 sends it with two compat flags. opencode 1.18.35 sends it | PROVEN | [spike 1b](evidence/spikes/s1b-pi-lease-gaps-README.md), [spike C](evidence/spikes/sC-key-stability/README.md) |
| `x-claude-code-session-id` | Claude Code sends it, with an agent id header. A subagent keeps the session id and adds its own agent id | PROVEN | [spike decisions](../decisions/2026-10-spike-decisions.md), [spike C](evidence/spikes/sC-key-stability/README.md) |
| `x-session-id` | OpenRouter reads it. The vLLM router and the vLLM semantic router read it. opencode 1.18.35 sends it | PROVEN (docs read, spike C) | [r5](evidence/research/r5-gateways-20261007T055410Z.md), [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md) |
| `X-OpenWebUI-Chat-Id` | Open WebUI can forward it | ASSUMPTION (not run) | [Open WebUI horizon note](../../docs/horizon/openwebui.md) |
| `x-litellm-session-id` | LiteLLM reads it | PROVEN (source read) | [r5](evidence/research/r5-gateways-20261007T055410Z.md) |
| `X-Olla-Session-ID` | Olla reads it | PROVEN (source read) | [r6](evidence/research/r6-routers-20261007T055357Z.md) |
| `X-SMG-Routing-Key` | The SGLang gateway reads it | PROVEN (source read) | [r6](evidence/research/r6-routers-20261007T055357Z.md) |
| `prompt_cache_key` (body field) | OpenAI hosted API reads it. Codex 0.160.1 sends it, equal to its session id | PROVEN (docs read, spike C) | [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md), [spike C](evidence/spikes/sC-key-stability/README.md) |
| `session-id`, `thread-id` | Codex 0.160.1 sends both | PROVEN | [spike C](evidence/spikes/sC-key-stability/README.md) |

No header in Table 1 is a standard. The proxy reads several of them, and the key map is configurable per route ([DEC-019](01-decisions.md)). The meaning of "standards-based" is therefore this: the proxy follows the habits that exist and does not invent a new one.

Two real standards exist for the observability side. Both have the label PROPOSED for v1:

- Prometheus text format for metrics. The format is a de facto standard with an open specification.
- OpenTelemetry GenAI semantic conventions for names of fields and metrics, where a name exists for the thing the proxy measures. The conventions had the status "in development" at the date of this file. This statement is an ASSUMPTION. Re-check it before you fix the names in [the observability file](08-observability-and-admin.md).

### 3.4 The bound on "optimised specialisation"

"Where extra tuning is possible, build it in" has no limit. Without a limit, the proxy grows into an engine manager. That is a different product with a different failure profile.

The bound in v1 has three parts:

1. Request-level tuning. The proxy can apply a fixed patch per node to the request. Examples are a thinking switch and a limit on `max_tokens`. The patch is identical on every turn of a conversation, because a changed prompt breaks the cache.
2. Endpoint configuration checks. The proxy compares the declared setting with the observed setting. Examples are the loaded context of a node and the slot count of a node.
3. Recommendations from probes. A calibration probe measures a node. The proxy reports a recommendation to the person. The person changes the engine.

The proxy does not start, stop, restart, load, unload or configure an engine. The proxy does not change an engine setting. See [DEC-011](01-decisions.md).

### 3.5 The differentiator rests on an unproven claim

The core claim is this: if the proxy keeps a conversation on one node, the placement pays. It pays only if the engine keeps the prompt cache between turns, and only if the cache hit saves more time than the placement costs.

This claim has the label NOT TESTED on this hardware. The evidence for it points both ways:

- Hosted providers and routers report gains from sticky routing. The numbers are vendor claims for other workloads (see [r3](evidence/research/r3-sglang-routing-20261007T022127Z.md)).
- Hybrid models, such as the Qwen 3.8 family, use a recurrent state in 3 of every 4 layers ([r1](evidence/research/r1-models-20261007T022035Z.md)). Their caches are fragile. Research found a silent zero-hit bug in vLLM (issue 45238, see [r2](evidence/research/r2-vllm-20261007T022033Z.md)) and reports of cold prefill on each turn in llama.cpp ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)).
- The cost of a moved conversation was 47 s on gufo and 230 s on llama.cpp at 60k tokens on Strix Halo. These numbers come from the owner's other repository, as read by research agents ([r4](evidence/research/r4-local-engines-20261007T022141Z.md)). They are not re-measured here.

Spike 7 measured the hybrid cache on this machine. It used llama-server b11459 and 0.5.0, Qwen3.5-2B Q4_K_M and a dense control. The results are PROVEN for that model.

Affinity pays on append-only traffic. A turn recomputes 201 tokens for 200 new tokens. A hybrid node recomputes the whole prompt when the history changes more than 516 tokens before the end. That costs 15 to 17 s at 6k and 34 to 38 s at 12k on this M2 under load. A second conversation stays warm on one slot only when `--cache-ram` is on.

The large models have the label NOT TESTED. Speeds are NOT TRANSFERABLE (spike B, DEC-060). The product has two possible shapes. The result depends on how often harnesses rewrite history:

| If affinity pays | If affinity does not pay |
|---|---|
| The proxy is a cache-aware router with admission control. | The proxy is a hold queue with a per-node cap. |
| The key rules are the main work. | The key rules are a small feature that the proxy disables per node. |
| A gain over a plain round robin is likely on cold-turn rate and on tokens recomputed. | HAProxy with `maxconn` and a queue timeout covers most of the value ([r5](evidence/research/r5-gateways-20261007T055410Z.md)). |

The second column is the worse case. If affinity does not pay on the engines of the owner, a configured HAProxy or an extended Olla can match the product. The build-or-adopt choice then favours adoption. The benchmark in section 6 is the test of this. The owner must accept that the benchmark can end the project.

### 3.6 Other blind spots

- **Adoption cost.** The research estimate for an Olla extension is 1 to 2 weeks of Go. The estimate for a new build is 3 to 5 weeks. Both are the estimates of one agent (ASSUMPTION, see [r6](evidence/research/r6-routers-20261007T055357Z.md)). A new build needs a reason that a benchmark can show.
- **Soft state.** The proxy loses the affinity table at a restart. After a restart, each conversation pays a cold turn. The earlier prototype wrote pins to disk ([spike record](../decisions/2026-10-spike-decisions.md)). The reversal is deliberate (see [DEC-021](01-decisions.md)). Its cost has the label NOT MEASURED.
- **No retries and no failover.** A node failure reaches the harness. pi retries a failed request 3 times in about 14 s (spike 5). The retry can return to the same node. The user sees the failure. This is a product limit, not a defect.
- **Key stability.** The hash key must stay stable across turns. This has the label NOT TESTED for each harness. A harness that edits the first message breaks the key.
- **Evidence base.** Most spikes ran on one Apple M2 with a 1.7B model and fakes. The owner targets hybrid models of 27B and larger on other hardware. The spikes prove that the parts connect. They do not prove performance.
- **Few users.** The target is 3 people. A benchmark on this scale can give a result that does not hold at a larger scale. The proxy does not claim a larger scale.

## 4. Target and scale ceiling

The target is a small cluster. The values below are the owner's target. They have the label PROPOSED as design limits.

| Item | Value | Label |
|---|---|---|
| Machines | about 5 | PROPOSED |
| People | 3 | PROPOSED |
| Running conversations per node | about 4 | PROPOSED |
| Running requests in total | about 20 | PROPOSED (5 times 4) |
| Conversations in the table | up to a few hundred | PROPOSED |
| Proxy processes | 1 | PROPOSED |

The scale ceiling is the limit above which the design makes no claim. The owner approved the ceiling on 2026-10-07 (DEC-052). The label PROPOSED stays until a test covers it.

The proxy runs as one process on one machine. It has no cluster mode and no shared state. It has no high availability. It has no tenant isolation.

A node count above 20 and a total above 100 concurrent conversations are outside the design. The proxy can run above the ceiling. No test covers that case and no claim covers it.

The proxy keeps all state in memory. The ceiling is therefore a memory ceiling and a single-thread coordination ceiling. Nobody measured either. See [the architecture file](02-architecture.md) for the resource targets.

## 5. Measurable claims

Each claim replaces a part of "better than anything else". Pass values are not numbers yet. The owner decided on 2026-10-07 that the numbers follow the benchmark. The rule in the next paragraphs fixes the pass test before the run, so that nobody can choose a threshold after the result.

| ID | Claim | Metric | Baselines | Status |
|---|---|---|---|---|
| VIS-001 | The proxy adds little latency. | Added latency to the first byte and per SSE chunk, in ms, at the 50th and 99th percentile | Direct request, Olla, LiteLLM, HAProxy, SGLang router, round robin | NOT TESTED |
| VIS-002 | The proxy uses little memory. | Resident memory in MB at idle and at 20 running streams | Olla, LiteLLM, HAProxy, SGLang router, round robin | NOT TESTED |
| VIS-003 | The proxy keeps the time to first token low under load. | Time to first token at the 50th and 99th percentile, with 20 running conversations | Same as VIS-001 | NOT TESTED |
| VIS-004 | The proxy has fewer cold turns. | Share of turns after the first with cache reuse below a set limit | Same as VIS-001 | NOT TESTED |
| VIS-005 | The proxy recomputes fewer tokens. | Mean prompt tokens recomputed per turn | Same as VIS-001 | NOT TESTED |
| VIS-006 | The proxy keeps the queue wait low. | Time from arrival to dispatch at the 50th and 99th percentile, when demand is above the slots | Same as VIS-001, only those with a queue | NOT TESTED |
| VIS-007 | The proxy does not change the bytes. | Byte difference of the response stream against the direct response | Same as VIS-001 | PROVEN for the prototype, NOT TESTED for this proxy |
| VIS-008 | The proxy works with the named harnesses without a harness change. | Pass or fail of a recorded conversation from each harness | None | NOT TESTED (pi partly PROVEN in spikes) |
| VIS-009 | The proxy forwards a hosted endpoint unchanged and keeps the key. | Byte difference, and a check that no inbound credential and no `x-legatus-*` header reaches the node | LiteLLM | NOT TESTED |

The baselines for the first six claims are Olla, LiteLLM, HAProxy with a stated configuration, the SGLang router and a plain round robin. HAProxy is the strongest baseline for the queue and the sticky table. A plain round robin is the lowest baseline. If the proxy loses to the round robin on a metric, the claim for that metric fails.

A tie is a result. A tie means that the proxy gives no gain on that metric.

The benchmark rule (decided by the owner, 2026-10-07):

1. Run the baselines first. Record the result of each baseline.
2. The proxy must beat every baseline on the cold-turn rate (VIS-004) and on the queue wait (VIS-006).
3. The proxy must stay within 2 times the best baseline on the added latency (VIS-001) and on the resident memory (VIS-002).
4. The pass numbers follow the benchmark. The owner sets them after the baseline results exist.

The rule gives no pass test for VIS-003 and VIS-005. The benchmark reports both. VIS-007 to VIS-009 are pass or fail checks. A tie on the cold-turn rate or on the queue wait fails the claim.

## 6. Benchmark method

The full method, the stub engines and the workloads are in [the test fixtures file](13-test-fixtures-and-scenarios.md). This section gives the rules that keep the result valid.

1. Pin the version of every baseline and every engine. Write the version in the result.
2. Give each baseline the best configuration that the owner of the baseline documents. Publish the configuration files.
3. Use the same hardware, the same models and the same workload for every baseline. Run the baselines one after the other, not at the same time.
4. Use recorded harness traffic as the workload. Record pi and Claude Code conversations. Replay them with the original timing between turns.
5. Run each case at least 5 times (PROPOSED). Report the median and the spread. Do not report only the best run.
6. Measure added latency with a stub engine that answers with no delay. Then the difference to a direct request is the cost of the proxy alone.
7. Measure the time to first token, the cold-turn rate and the tokens recomputed with real engines. Read the cache fields from the response (see [the engine file](05-engine-behaviour.md)).
8. Measure memory as resident set size of the process, at idle and at 20 running streams.
9. Measure the queue wait with demand above the slot count. A baseline with no queue must show how it fails, for example a refusal.
10. Record every case where a baseline cannot run the scenario. A refusal is a result, not a gap.

HAProxy needs an explicit configuration because it has no LLM features. The sketch below is the PROPOSED baseline. It has the label NOT TESTED. The person who runs the benchmark must check the syntax against the HAProxy version in use.

```
backend llm_pool
    balance leastconn
    stick-table type string len 64 size 10k expire 10m
    stick on req.hdr(x-session-affinity)
    timeout queue 250s
    server n1 10.0.0.1:8080 maxconn 4 check
    server n2 10.0.0.2:8080 maxconn 4 check
```

This baseline reads one header. It has no body hash for harnesses without a header. A body hash in HAProxy is fragile ([r5](evidence/research/r5-gateways-20261007T055410Z.md)). A fair baseline run reports both the header case and the no-header case.

Three threats to the validity of the benchmark:

- The author of the proxy also writes the benchmark. The workload must come from recorded traffic and not from the proxy design.
- Stub engines copy measured behaviour. They can hide real behaviour. Recorded captures from real engines must check the stubs.
- Spike 7 measured one engine and one small model. Claims VIS-003 to VIS-005 have no valid result for the proxy before the benchmark and spike B.

## 7. Non-goals

The proxy v1 does not do the items below. Each item has a decision in [the decision log](01-decisions.md).

| Non-goal | Decision | Where it goes |
|---|---|---|
| Capability manager: custom protocol, capability discovery, negotiation, explicit leases, `can_spawn`, `request_llm` | DEC-005 | [Parked](16-parked-capability-manager.md) |
| Harness adapters and harness instructions | DEC-006 | Later |
| Subagent orchestration, worktrees, guard, sandbox, remote execution | DEC-006 | Out |
| Conversation storage and capture | DEC-007 | A separate product |
| Price and budget tracking | DEC-008 | Out |
| Retries, failover, circuit breakers in the proxy | DEC-009 | Version 2 or later |
| Timeouts, other than the hold limit | DEC-010 | Version 2 or later |
| Engine start and engine management | DEC-011 | Out |
| An AppleScript API | DEC-012 | Out |
| Control actions in the admin API | DEC-037 | Later |
| Protocol translation between OpenAI and Messages | DEC-045 | Not decided for later |
| Splitting one model across machines | [vision baseline](../baseline/vision.md) | Out |
| Multi-user cloud scale and Kubernetes | DEC-052 | Out |

## 8. Open points for the owner

- Set the pass numbers for the claims in section 5 after the baseline results exist (see [DEC-051](01-decisions.md)). The owner decided the rule.
- Accept or reject the meaning of "locally or remote" in section 3.2.
- Accept or reject the bound on "optimised specialisation" in section 3.4.
- Decide what happens to the project if affinity does not pay (section 3.5).

## Sources

- Owner's vision text: the brief for this specification, 2026-10-07 (not a file in this folder).
- [Spike decisions, October 2026](../decisions/2026-10-spike-decisions.md).
- [Scope reset, 2026-10-06](../decisions/2026-10-scope-reset.md).
- [Baseline vision](../baseline/vision.md).
- [r1 models](evidence/research/r1-models-20261007T022035Z.md), [r2 vLLM](evidence/research/r2-vllm-20261007T022033Z.md), [r3 SGLang and routing](evidence/research/r3-sglang-routing-20261007T022127Z.md), [r4 local engines](evidence/research/r4-local-engines-20261007T022141Z.md), [r5 gateways](evidence/research/r5-gateways-20261007T055410Z.md), [r6 routers](evidence/research/r6-routers-20261007T055357Z.md).
- [Spike 1b](evidence/spikes/s1b-pi-lease-gaps-README.md), [spike 5](evidence/spikes/s5-pi-restart-README.md).
- [Open WebUI horizon note](../../docs/horizon/openwebui.md).
