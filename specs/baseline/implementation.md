# Implementation languages

Shared, long-lived services and anything that runs on an inference node are written in Rust. Harness-side code stays in TypeScript, as a pi extension. The split follows two costs: resident footprint, and crossing a process boundary.

## Why Rust for services: footprint, not just latency

Inference dominates request latency, so speed alone does not justify Rust. Footprint does.

- **Memory.** The M5 Max and AMD 395 use unified memory. Every megabyte a sidecar holds is a megabyte unavailable for model weights or KV cache. An interpreted gateway with its dependencies can sit in the hundreds of megabytes; a Rust binary doing the same routing typically stays in the tens.
- **CPU.** On the 395 the CPU shares power and thermal headroom with the GPU, so idle polling loops in an interpreter are not free.
- **Disk and deployment.** A single static binary of a few megabytes, with nothing to install per node, versus a language runtime and environment of hundreds of megabytes.

## Why TypeScript inside the harness

pi extensions are TypeScript and run in pi's existing runtime. The adapter's few functions (acquire, release, session identity) cost almost nothing there, and calling out to a separate process for them would cost more.

## The rule

| Component | Language | Reason |
| --- | --- | --- |
| `legatus-router` on the client machine (includes the lease table and journal) | Rust | Always on; replaces an interpreted gateway; owns routing so engines stay swappable |
| `legatus-node` agent (health, load, machine readings) | Rust | Runs beside inference on every node, so footprint matters most |
| `legatus-admin` (dashboard and MCP) | Rust | Always on, small and stateless; a thin client of the router's admin API, kept out of the router so its footprint does not grow |
| `pi-legatus` adapter (`can_spawn`, `request_llm`, `release_llm`, config generation) | TypeScript | Runs in pi's process; calls the router's admin listener over HTTP |
| Adapter contract | TypeScript | `packages/adapter-contract` (`@legatus/adapter-contract`), the three-capability contract every adapter imports |
| Apple on-device node shim | Swift | Apple's on-device model is reachable only through Apple's own framework; the shim exposes an OpenAI-compatible endpoint. The spike used Hummingbird 2.27.0 |
| Calibration probe | Rust, once promoted | The spike probe is stdlib Python; see the exemption below |

There is no separate coordination service, no MCP server and no harness-side guard in v1; the earlier coordination crate, the TypeScript guard and the MCP surface are archived with their designs (see [archive](../archive/coordination-full-design.md)).

**Evidence for the rule.** In the spike the Rust router added 0.06 ms to first byte and 0.05 ms per streamed chunk, with 3 to 6 MB resident memory, and streamed byte-identical output. The numbers come from scripted fakes on one Apple M2, not from a loaded fleet.

**Exemption for verification tools.** On-demand verification tools (spikes, probes, conformance and compatibility checks) may be written in Python (run with `uv run`) or Node. They are not resident, do not run on inference nodes in normal operation, and carry none of the footprint cost. Anything that stays running follows the rule above.

## Workspace layout (PROPOSED)

| Kind | Members |
| --- | --- |
| Crates | `legatus-common` (shared types, the upstream tap trait, `ThermalBand`), `legatus-router`, `legatus-node` (also hosts the machine readings), `legatus-telemetry` (identity, behavioural record, energy, thermal, event stream), `legatus-eventlog` (the router event log), `legatus-admin`, `legatus-simcluster` (stub engines, virtual clock; layer `testkit`, depends on common only, other crates use it only as a dev-dependency), `legatus-simtests` (layer `verification`; depends on router, node, common and simcluster; `publish = false`; nothing depends on it) |
| Packages | `pi-legatus`, `adapter-contract`, `legatus-registry`, the admin contract package (routes, metrics and panels registries) |
| Tools | canary, calibrate, role-guide, fleet-check, smoke-test, verify-add-node, keyscan, measure, compat, chaos, adapter-conformance, the admin parity and browser test tools, apple-node (Swift, optional) |

Rules: the router may depend on telemetry only through one small trait in `legatus-common` (the upstream tap); `legatus-admin` may depend only on `legatus-common` and the admin contract package; package-to-package edges are allowed; the layout test asserts the required members and these rules, not a closed list, so members can be added. Capture crates are not members until the capture product starts. There is no coordination crate, no MCP crate in the router and no pi-subagents dependency.

## Candidate crates

`tokio`, `axum` for the OpenAI-compatible router and its admin listener, `reqwest` for node polling, `serde` for JSONL records. One `axum` version is used across the workspace: 0.8.9 PROPOSED, settled by the version pins task. There is **no embedded database**: the lease journal, the decision log, the event log, the behavioural record and the verdict files are JSONL or JSON. Adding a database crate would be a deliberate decision with a pinned version, not a default. The MCP server in `legatus-admin` is hand-written minimal JSON-RPC on these crates (PROPOSED), with a pinned library as the fallback if a spike against pi 1.0.3 says so; NOT TESTED.
