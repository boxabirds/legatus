# Legatus proxy specification

Status: draft, 2026-10-07. Owner of this file: writer W-A. The final consistency pass ran on 2026-10-07.

Legatus v1 is a reverse proxy. It routes requests from current harnesses to LLM endpoints, local first. This folder is the baseline specification of the proxy. Ceetrix stories will come from it, so each requirement is precise, testable and traceable.

The vision is "Do one thing exceptionally well". Read [the vision and scope](00-vision-and-scope.md) first. It includes the critique.

## Reading order

1. [Vision and scope](00-vision-and-scope.md). The vision, its critique, the measurable claims, the non-goals and the scale ceiling.
2. [Decision log](01-decisions.md). Every decision with its status and evidence.
3. [Architecture](02-architecture.md). Components, request flow and process model.
4. [Affinity and keys](03-affinity-and-keys.md).
5. [Admission control and queueing](04-admission-control-and-queueing.md).
6. [Engine behaviour](05-engine-behaviour.md).
7. [Protocols and harnesses](06-protocols-and-harnesses.md).
8. [Registry and configuration](07-registry-and-configuration.md).
9. [Observability and admin](08-observability-and-admin.md).
10. [Restart and failure](09-restart-and-failure.md).
11. [Research and prior art](10-research-and-prior-art.md) and [lessons learned](11-lessons-learned.md).
12. [Edge cases](12-edge-cases.md), [test fixtures and scenarios](13-test-fixtures-and-scenarios.md), [open questions and risks](14-open-questions-and-risks.md).
13. [Glossary](15-glossary.md) and [the parked capability manager](16-parked-capability-manager.md).

## Files

| File | Content | ID ranges (IDs that the file defines) |
|---|---|---|
| [00-vision-and-scope.md](00-vision-and-scope.md) | Vision, critique, measurable claims, non-goals | VIS-001 to 009 |
| [01-decisions.md](01-decisions.md) | Decision log | DEC-001 to 067 |
| [02-architecture.md](02-architecture.md) | Components, flow, process model, resource targets | PRX-SCOPE-001 to 033, PRX-PERF-001 to 007 |
| [03-affinity-and-keys.md](03-affinity-and-keys.md) | Key rules, table, placement, moves | PRX-AFF-001 to 034, PRX-KEY-001 to 058 |
| [04-admission-control-and-queueing.md](04-admission-control-and-queueing.md) | Caps, protected window, hold queue | PRX-ADM-001 to 053 |
| [05-engine-behaviour.md](05-engine-behaviour.md) | Per engine facts, hybrid models, probes | PRX-ENG-001 to 075 |
| [06-protocols-and-harnesses.md](06-protocols-and-harnesses.md) | OpenAI and Messages paths, harness matrix | PRX-PROTO-001 to 065 |
| [07-registry-and-configuration.md](07-registry-and-configuration.md) | Registry file, patches, profile, secrets | PRX-REG-001 to 049, PRX-SEC-001 to 016 |
| [08-observability-and-admin.md](08-observability-and-admin.md) | Event log, metrics, admin API | PRX-OBS-001 to 058 |
| [09-restart-and-failure.md](09-restart-and-failure.md) | Restart, node failure, version 2 seams | PRX-REST-001 to 053 |
| [10-research-and-prior-art.md](10-research-and-prior-art.md) | Research and spike synthesis, build or adopt | none |
| [11-lessons-learned.md](11-lessons-learned.md) | Lessons from others and from spikes | LES-001 to 075 |
| [12-edge-cases.md](12-edge-cases.md) | Edge case catalogue | EDGE-ADM-001 to 040. EDGE-AFF-001 to 025. EDGE-CAC-001 to 027. EDGE-ENG-001 to 018. EDGE-HAR-001 to 030. EDGE-HST-001 to 018. EDGE-KEY-001 to 039. EDGE-OBS-001 to 017. EDGE-PRO-001 to 041. EDGE-RST-001 to 015. EDGE-STR-001 to 023. |
| [13-test-fixtures-and-scenarios.md](13-test-fixtures-and-scenarios.md) | Stubs, scenarios, benchmark method | FIX-001 to 339, PRX-TEST-001 to 087 |
| [14-open-questions-and-risks.md](14-open-questions-and-risks.md) | Open questions and risks | OPEN-001 to 045, RISK-001 to 042 |
| [15-glossary.md](15-glossary.md) | Terms, labels, ID prefixes | none |
| [16-parked-capability-manager.md](16-parked-capability-manager.md) | The parked manager and its seams | none |

## Status of the specification

| Item | State |
|---|---|
| Vision | Decided by owner. The critique and the measurable claims are proposals. |
| Benchmark of the claims | NOT TESTED. No result exists. The owner decided the rule. Baselines run first. The proxy must beat every baseline on cold-turn rate and queue wait. It must stay within 2 times the best result on added latency and memory. The pass numbers follow the benchmark. |
| Affinity pays on the target engines | PROVEN for llama-server on Qwen3.5-2B with append-only traffic (spike 7). NOT TESTED on other engines and on Flash-Next and 27B. |
| Hybrid cache numbers | PROVEN for Qwen3.5-2B on llama-server b11459 and 0.5.0 (spike 7). NOT TRANSFERABLE in speed. Spike B measures the owner machines. |
| Responses API on local engines | Read from source in r7 ([05](05-engine-behaviour.md) section 14). No engine ran with a live Codex conversation. Spike F is an owner decision pending. |
| Key stability and hold tolerance | PROVEN on fake servers in six harnesses and four SDKs (spike C and spike D, 2026-10-07). Open WebUI and real engines are NOT TESTED. |
| Decisions | See the status column in [the decision log](01-decisions.md). The owner decided the open items on 2026-10-07. Spike D closes DEC-027 (DEC-064). DEC-062 reverses DEC-057. Still open: the pass numbers of DEC-051. Pending owner confirmation: DEC-063, DEC-064 (with `Retry-After` 30 s) and DEC-067. Owner decision pending: DEC-065 (Responses pass-through only), DEC-066 (warm capacity) and spike F. |
| Owner decisions of 2026-10-07 | Hold queue in v1. Client token when a hosted node exists. Messages mid-stream default `overloaded_error`. No persistent table, no timeouts in the proxy except the hold limit, no leases. Table expiry 600 s. No hosted cap. Ceiling 20 nodes and 100 concurrent conversations. Other OpenAI paths pass through. v1 supports Codex, so the Responses API is in scope (DEC-062, reverses DEC-057). |

## Counts

The final pass computed these counts by script from the IDs that the files define. Each ID counts once. A REMOVED row keeps its ID and counts in the total, not in the active count. A superseded row (DEC-057, superseded by DEC-062) stays in the log and has its own column.

| Item | Total | REMOVED | Superseded | Active |
|---|---|---|---|---|
| Requirements `PRX-*` | 569 | 5 | 0 | 564 |
| Decisions `DEC-*` | 67 | 0 | 1 | 66 |
| Edge cases `EDGE-*` | 293 | 2 | 0 | 291 |
| Fixtures `FIX-*` | 223 | 2 | 0 | 221 |
| Lessons `LES-*` | 75 | 0 | 0 | 75 |
| Risks `RISK-*` | 42 | 0 | 0 | 42 |
| Open questions `OPEN-*` | 45 | 0 | 0 | 45 (6 of them marked CLOSED) |
| Claims `VIS-*` | 9 | 0 | 0 | 9 |

The removed requirements are PRX-ADM-031 to PRX-ADM-034 and PRX-PROTO-042. The removed edge cases are EDGE-HST-011 and EDGE-PRO-019. The removed fixtures are FIX-162 and FIX-224.

Requirements by area:

| Area | Total | REMOVED | Active |
|---|---|---|---|
| `PRX-ADM` | 53 | 4 | 49 |
| `PRX-AFF` | 34 | 0 | 34 |
| `PRX-ENG` | 75 | 0 | 75 |
| `PRX-KEY` | 58 | 0 | 58 |
| `PRX-OBS` | 58 | 0 | 58 |
| `PRX-PERF` | 7 | 0 | 7 |
| `PRX-PROTO` | 65 | 1 | 64 |
| `PRX-REG` | 49 | 0 | 49 |
| `PRX-REST` | 53 | 0 | 53 |
| `PRX-SCOPE` | 33 | 0 | 33 |
| `PRX-SEC` | 16 | 0 | 16 |
| `PRX-TEST` | 68 | 0 | 68 |

Some numbers between the first and the last ID of FIX and of PRX-TEST are not in use.

## Conventions

### Language

The files use an STE-style language. The official ASD-STE100 specification and dictionary was not available, so the text is STE-style and not STE-compliant. The rules come from a secondary summary of Issue 9 (January 2025). The evidence folder is not in this style.

Two linters check the text. Both are heuristic.

- `specs/proxy/tools/ste_lint.py` checks sentence length, banned words, dashes, semicolons and contractions. Errors must be zero.
- `/Users/julian/.claude/skills/asd-ste100/scripts/ste-lint.py` is the linter of the asd-ste100 skill. Hard violations must not increase.

### Labels

Mark each unproven statement with PROVEN, PROPOSED, ASSUMPTION or NOT TESTED. See [the glossary](15-glossary.md).

### Requirements

A requirement has the form "The proxy must verb object condition." It has one ID of the form `PRX-AREA-NNN`. AREA is one of SCOPE, PROTO, ROUTE, KEY, AFF, ADM, ENG, REG, OBS, REST, SEC, TEST or PERF. Each file owns its ID range and does not reuse the IDs of another file.

### Evidence

A claim links to a file in the `evidence/` folder. Each file ends with a list of sources.

## Evidence folder

| Folder | Content |
|---|---|
| [evidence/research](evidence/research/r1-models-20261007T022035Z.md) | Six research notes from web research on 2026-10-07. The notes label each claim documented, inferred or unverified. Re-check numbers before you rely on them. |
| [evidence/spikes](evidence/spikes/s5-pi-restart-README.md) | Spike notes. They include [spike C](evidence/spikes/sC-key-stability/README.md) (key stability), [spike D](evidence/spikes/sD-hold-tolerance/README.md) (hold tolerance) and [spike 7](evidence/spikes/s7-hybrid-cache/README.md) (hybrid cache measurement, 2026-10-07). The row also names [research r7](evidence/research/r7-responses-api-20261007T081115Z.md) (Responses API and Codex). |

Earlier material: [the spike decisions](../decisions/2026-10-spike-decisions.md), [the scope reset](../decisions/2026-10-scope-reset.md), [the baseline](../baseline/vision.md) and [the archive](../archive/coordination-full-design.md). The baseline and the archive describe a larger product. Where they disagree with this folder, this folder wins.

## Check the style

Run the linter on each file. The linter is heuristic. Errors must be zero.

```
uv run specs/proxy/tools/ste_lint.py specs/proxy/00-vision-and-scope.md
```

## Sources

- [Vision and scope](00-vision-and-scope.md).
- [Decision log](01-decisions.md).
