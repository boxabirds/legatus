# Implementation languages

Shared, long-lived services and anything that runs on an inference node are written in Rust. Logic that runs on every tool call inside an agent stays in TypeScript, as pi extensions. The split follows two costs: resident footprint, and crossing a process boundary.

## Why Rust for services: footprint, not just latency

Inference dominates request latency, so speed alone does not justify Rust. Footprint does.

- **Memory.** The M5 Max and AMD 395 use unified memory. Every megabyte a sidecar holds is a megabyte unavailable for model weights or KV cache. An interpreted gateway with its dependencies can sit in the hundreds of megabytes; a Rust binary doing the same routing typically stays in the tens.
- **CPU.** On the 395 the CPU shares power and thermal headroom with the GPU, so idle polling loops in an interpreter are not free.
- **Disk and deployment.** A single static binary of a few megabytes, with nothing to install per node, versus a language runtime and environment of hundreds of megabytes.

## Why TypeScript inside the harness: avoid process boundaries

The case against Rust is interoperability. pi extensions are TypeScript and run in pi's existing runtime. A few extra functions there cost almost nothing. Calling out to a separate process for every edit or commit check costs far more, and spawning a process per hook is worse still. Code that runs per tool call therefore stays in-process.

## The rule

| Component | Language | Reason |
| --- | --- | --- |
| `legatus-coord` (leases, task board, inboxes) | Rust | Shared by every agent across machines; long-lived; one persistent connection per agent |
| `legatus-router` on the client machine | Rust | Always on; replaces an interpreted gateway; owns routing so engines stay swappable |
| `legatus-node` health agent (optional) | Rust | Runs beside inference on every node, so footprint matters most |
| `delegate` tool | TypeScript (`pi-legatus`) | Called by the orchestrator in-process; forwards to the router |
| Lease-enforcement hook | TypeScript (`pi-legatus`) | Runs on every edit and commit; caches its own agent's leases so most checks never leave the process |
| Role definitions and registry lookup at spawn | TypeScript (`pi-legatus`) | Read once per spawn inside pi |

Rust services are started once and reached over a persistent connection (a Unix socket for in-process extensions, loopback MCP over HTTP for MCP clients), so each call costs a message, not a process spawn.

| Component | Language | Reason |
| --- | --- | --- |
| Apple on-device node shim | Swift | Apple's on-device model is reachable only through Apple's own framework; the shim exposes an OpenAI-compatible endpoint. The spike used Hummingbird 2.27.0 |
| Calibration probe | Rust, once promoted | The spike probe is stdlib Python; see the exemption below |

**Evidence for the rule.** In the spike the Rust router added 0.06 ms to first byte and 0.05 ms per streamed chunk, with 3 to 6 MB resident memory, and streamed byte-identical output. The harness-side guard and `delegate` ran in TypeScript inside pi with no process spawn per tool call. The numbers come from scripted fakes on one Apple M2, not from a loaded fleet.

**Exemption for verification tools.** On-demand verification tools (spikes, probes, conformance and compatibility checks) may be written in Python (run with `uv run`) or Node. They are not resident, do not run on inference nodes in normal operation, and carry none of the footprint cost. Anything that stays running, or runs per tool call, follows the rule above.

## Escape hatch

If a hot path inside pi later needs Rust, napi-rs compiles it into a native Node addon loaded in-process. Use it sparingly: it adds a build step per platform.

## Candidate crates

`rmcp` (official Rust MCP SDK), `tokio`, `axum` for the OpenAI-compatible router, `reqwest` for node polling, `rusqlite` for coordination state.
