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
| `pi-legatus` adapter (acquire, release, config generation) | TypeScript | Runs in pi's process; calls the router's admin listener over HTTP |
| Apple on-device node shim | Swift | Apple's on-device model is reachable only through Apple's own framework; the shim exposes an OpenAI-compatible endpoint. The spike used Hummingbird 2.27.0 |
| Calibration probe | Rust, once promoted | The spike probe is stdlib Python; see the exemption below |

There is no separate coordination service, no MCP server and no harness-side guard in v1; the earlier coordination crate, the TypeScript guard and the MCP surface are archived with their designs (see [archive](../archive/coordination-full-design.md)).

**Evidence for the rule.** In the spike the Rust router added 0.06 ms to first byte and 0.05 ms per streamed chunk, with 3 to 6 MB resident memory, and streamed byte-identical output. The numbers come from scripted fakes on one Apple M2, not from a loaded fleet.

**Exemption for verification tools.** On-demand verification tools (spikes, probes, conformance and compatibility checks) may be written in Python (run with `uv run`) or Node. They are not resident, do not run on inference nodes in normal operation, and carry none of the footprint cost. Anything that stays running follows the rule above.

## Candidate crates

`tokio`, `axum` for the OpenAI-compatible router and its admin listener, `reqwest` for node polling, `serde` for the JSONL lease journal.
