# Parked: the capability manager

Status: draft, 2026-10-07. Owner of this file: writer W-A. The decision is [DEC-005](01-decisions.md).

This file records what the owner parked, why, what the proxy keeps so that the work can restart, and when to restart it. Nothing here is a requirement of v1.

## 1. What the parked manager is

The capability manager is the earlier design of Legatus. It linked a subagent task to an LLM node by the semantic needs of the task. It had these parts:

| Part | What it did |
|---|---|
| Custom protocol | A Legatus API on top of the proxy, with versioned routes under `/legatus/v1/`. |
| Capability discovery | A three-layer node description: model card, server card and measured profile. |
| Negotiation | A caller asked for a role or for capabilities. Legatus answered with a node or a refusal. |
| Explicit lease | A caller held a node for the length of a task. A lease limited the slots of a node. |
| `can_spawn` | A cheap advisory query of the free capacity before a spawn. |
| `request_llm` | The call that acquired a lease. A matching call released it. |
| Pool view | A read of the total and used slots of each node. |

The full text is in [LLM pool allocation](../baseline/coordination.md) and in the [archive](../archive/coordination-full-design.md). The owner's reduced scope is in [the scope reset](../decisions/2026-10-scope-reset.md).

## 2. Why the owner parked it

1. **The vision excludes it.** The owner defined v1 as a reverse proxy that needs no change in the harness. A lease call is a change in the harness.
2. **The key mechanism has the label NOT TESTED.** The explicit lease path needs a pi extension that sets a header after an acquire call. Nobody tested this on pi 1.0.3 ([scope reset, open items](../decisions/2026-10-scope-reset.md)).
3. **A harness can forget.** A harness that does not call `request_llm` holds nothing and has no limit. The lease only limits callers that take part.
4. **The proxy can limit load without it.** The admission cap and the hold queue limit the load on a node for every harness. They need no cooperation ([DEC-023](01-decisions.md), [DEC-026](01-decisions.md)).
5. **It adds surface.** The manager needs a control API, a token scope, a journal, a restart rule and a lease-only token. The proxy v1 has a read-only admin side ([DEC-037](01-decisions.md)).
6. **The proxy needs proof first.** The claim that affinity pays has partial proof only. Spike 7 showed it for append-only traffic on one small model and one engine. Task-level selection has no value if the base routing does not work.

The parked state has a real cost. Without leases, the proxy chooses a node from the alias that the harness names. The proxy cannot see the task. A harness cannot ask for "a node with a context of 128k". It must choose an alias that the registry maps to such nodes.

## 3. What the proxy keeps as seams

A seam is a place where the manager can attach later with no rewrite. The proxy v1 keeps these seams. Each seam is a design rule, not a feature.

| Seam | Rule | Why it helps |
|---|---|---|
| One node call site | The proxy sends every request to a node from one place in the code. | A timeout, a retry or a lease rule can attach in one place. |
| One recorded outcome per request | The proxy writes one event for each request. It does not act on the event in v1. | A later manager and a later resilience layer have data. |
| Alias indirection | A request names an alias. The registry maps it to nodes. | A role or a capability filter can replace the alias lookup. |
| Reserved header namespace | The proxy reserves every `x-legatus-*` header. It removes them before a hosted node. | A lease header such as `x-legatus-lease` has a safe place. |
| Request id header | The proxy reads and writes `x-legatus-request-id`. | A caller can match a lease event to a request event. This is a PROPOSED name from the earlier design. |
| Key map per route | A route sets how the affinity key forms. The map covers the three protocol paths: chat, Messages and Responses (DEC-062). | An adapter can send a key of its own, for example `session#agent`. |
| Cap per node | The admission control counts running requests against a cap. | A lease can claim part of the cap, and an exclusive role can claim all of it. |
| Registry fields | The registry holds engine, model, context, slots, machine, protocol and endpoints. | The three-layer description needs few new fields. |
| Read-only admin API with a version prefix | The admin API has a version prefix and needs a token. | Control routes can join with a new scope. |

Spike C changed one fact that matters for the trigger below.

Subagent identity is already on the wire without a lease. A Claude Code subagent keeps the session id and sends its own agent id. A pi-subagents child and an opencode child send their own session header. DeepSeek Harness children send no header and differ by their first message. Codex sends `session-id` and `thread-id` ([spike C](evidence/spikes/sC-key-stability/README.md), PROVEN). The proxy can therefore tell most children from their parents with no lease.

This lowers the value of the manager for the case "two subagents of one task".

These seams have the label PROPOSED. The architecture file must turn each seam into a requirement or drop it. See [the architecture](02-architecture.md).

## 4. What the proxy does not keep

- No lease table, no journal and no lease routes.
- No role layer. An alias is a pool name and nothing more.
- No `can_spawn` and no `request_llm`.
- No placeholder routes. A route appears only when its feature exists.

## 5. Trigger to restart

Restart the manager work when all of these conditions are true. The values have the label PROPOSED. The owner must approve them.

1. Spike B (or a later measurement) shows that affinity pays on at least one engine and model that the owner uses. Spike 7 showed it for llama-server and Qwen3.5-2B only.
2. The benchmark in [the vision](00-vision-and-scope.md) shows that the proxy meets its claims.
3. A real harness ran through the proxy for a period that the owner sets, with no unplanned change in the design.
4. The event log shows a problem that the admission cap and the key rules of [03](03-affinity-and-keys.md) cannot solve. An example is two subagents of one task that need different nodes, or a deadlock between a parent and its children.
5. A pi extension can set a header after an acquire call. This needs a spike first.

If condition 4 never occurs, the manager stays parked.

## 6. Risks of the parked state

- **Drift.** The old baseline documents describe the manager as if it were in v1. A reader can confuse them with this specification. The final pass must mark them as parked. See [the baseline index](../baseline/vision.md).
- **Loss of knowledge.** The spike evidence for leases stays in [the spike record](../decisions/2026-10-spike-decisions.md) and in the archive. The spikes gathered it on fakes and a 1.7B model.
- **A second product.** The manager can become a product on top of the proxy. A clean split lets it use the proxy as it is.

## Sources

- [Decision log](01-decisions.md).
- [Vision and scope](00-vision-and-scope.md).
- [LLM pool allocation, baseline](../baseline/coordination.md).
- [Coordination full design, archive](../archive/coordination-full-design.md).
- [Scope reset, 2026-10-06](../decisions/2026-10-scope-reset.md).
- [Spike decisions, October 2026](../decisions/2026-10-spike-decisions.md).
- [Spike 1b](evidence/spikes/s1b-pi-lease-gaps-README.md).
- [Spike C, key stability](evidence/spikes/sC-key-stability/README.md): subagent identity in harness headers.
