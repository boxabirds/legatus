# Architecture decisions from the October 2026 spike phase

**Status: the baseline docs in `../baseline/` have been updated to match this record (not committed). D-1 to D-5 are approved; P-1 to P-5 remain proposed and are marked as proposed in those docs. Ceetrix stories have not been changed.**

Method: each claim below comes from a spike that installed the real software and ran it against scripted fakes or small real models (Qwen3 1.7B on an Apple M2, 16 GB). Spike agents wrote the reports; the coordinator re-ran only the router regression suite (12 of 12 passed). Evidence lives in the Legatus session scratchpad under `spikes/`, `spike-pi-*`, `spike-dsh-*`. Small models prove plumbing and protocol conformance, not tool-calling quality, and nothing here was tested on CUDA, Vulkan/Strix Halo or models larger than 1.7B.

## 1. Decisions

### Approved by the owner
| # | Decision |
|---|---|
| D-1 | **pi first.** Target pi 1.0.3 (`@earendil-works/pi-coding-agent`; the `@mariozechner` package is deprecated) because its system prompt is very light. Claude Code, opencode and DeepSeek Harness come later as adapter stories. |
| D-2 | **One protocol per role, never mixed.** Local roles use OpenAI chat-completions. Frontier roles keep their own protocol as separate roles. The router does not translate for pi. |
| D-3 | **Lease scope: isolate first, lock only what cannot be isolated or merged.** One git worktree per writer, enforced as confinement by the guard (not a lease). A `release` lease. One generic named-resource lease (for example `gpu:rtx4090`, a shared test environment) claimed by `delegate` before spawn. Branch leases dropped. Path-glob leases deferred to an optional later phase. |
| D-4 | **Three-layer capability model.** Nodes are described by a *model card* (what the weights can do), a *server card* (what the inference engine exposes, its endpoints and limits) and a *measured profile* (latency, throughput, concurrency, calibrated). Roles are not a layer: they sit above and declare required capabilities. Roles also carry an **endpoint kind** (`chat`, `embeddings`, `transcription`, `speech`, `images`, `rerank`, `classify`, `decide`, `tool`) beside the protocol. Two role tiers: capability roles and work roles. Hosted pay-per-token nodes need the same credential custody and budget cap as the frontier. |
| D-5 | **Method.** Prove basic feasibility with real spikes before product engineering. Record the version beside every result. Freeze engines for spikes (Ollama auto-updates mid-test). |

### Proposed defaults, not yet confirmed
| # | Default |
|---|---|
| P-1 | Coord is served over a **Unix socket** to in-process extensions and **loopback streamable-HTTP MCP** to MCP clients (pi's MCP supports only stdio and streamable HTTP). |
| P-2 | `tester` is a writer. |
| P-3 | Footprint budgets live in the registry parameters. |
| P-4 | Foreground subagent coverage: pi-subagents, with the guard listed in every agent's `extensions:`, plus a startup self-check and a registration check. No in-process fork. |
| P-5 | Apple's on-device model is an optional node for narrow roles (section 3.7). |

## 2. Architecture as proven

```
 harness (pi 1.0.3)  --OpenAI chat or Messages-->  legatus-router (Rust)  -->  engines / hosted nodes
   |  guard extension (tool_call hook) ----------->  legatus-coord (Rust, SQLite event log)
   |  delegate tool  -> pi-subagents (async children) -> worktree + branch per writer
```

- **Router:** Rust axum proxy. Rewrites the role in `model` to the node's model id, streams SSE byte for byte, pins sessions on `x-session-affinity`, fails over only before the first byte, enforces first-byte and idle timeouts, keeps a per-node breaker, logs every decision.
- **Coord:** Rust with a rusqlite append-only event log. Tools: claim, renew, release, who_holds, transfer, claim_many, a minimal task board. Unix socket plus MCP over loopback HTTP (rmcp).
- **Guard:** TypeScript pi extension on `pi.on("tool_call")`. Confinement: a write is allowed only inside the agent's own worktree. Own deadline on every coord call, fail closed, startup self-check, registration with coord.
- **Delegate:** TypeScript pi extension. Async children, pointer-only results, idempotent completion.

## 3. Verified facts by area

### 3.1 pi 1.0.3 (spikes on the current package)
- **Hook:** `pi.on("tool_call")` returning `{block: true, reason}` blocks before execution, also for MCP tools and codemode inner calls. A throwing hook fails closed. There is **no hook timeout** (a hung call stalled 70 s+), so Legatus must set its own deadline.
- **Timers:** `setInterval` fires through a 10 s model wait and a 5 s tool run. A synchronous busy loop starves it. Timers and sockets must be `unref()`d or `pi -p` never exits.
- **Custom tools and connections:** `pi.registerTool` works. A handler held one persistent Unix-socket connection across calls.
- **MCP:** built in, stdio and streamable HTTP only (no Unix socket, no legacy SSE). Tools are declared to the model only with `exposure: "direct"` (default `codemode` hides them). `-ne` disables built-in MCP too. If coord is down at pi start, its tools are silently absent and never appear later.
- **Providers:** a role name works as the model id on both protocols. `compat.sendSessionAffinityHeaders: true` plus `sessionAffinityFormat: "openai-nosession"` sends `x-session-affinity` on OpenAI and Anthropic paths. Compaction and summary calls carry **no** session header. `models.json` is user-directory only (`PI_CODING_AGENT_DIR`). Project settings need trust (`-a`).
- **Retries and timeouts:** pi retries a failed call 3 times (2/4/8 s) and has a 300 s request timeout, so a hung backend stalls it and its retry returns to the same node.
- **pi-subagents 0.76.0:** foreground children run in the parent's process and do **not** load its extensions unless the agent lists them in `extensions:`. Async and workflow children are separate processes and do load the user-level guard. No per-run environment variables (only cwd and a 16 KiB `extensionBindings`). Completions arrive as new user messages and can loop a naive orchestrator. Spawn over RPC is async-only with file-only output; the structured API is foreground-only. `pi-acp` is an editor adapter with no ssh, so remote execution needs plan B (an ssh exec tool).

### 3.2 Router (S1, S3)
- Added latency 0.06 ms to first byte, 0.05 ms per SSE chunk, 3 to 6 MB RSS. Streams byte-identical.
- Failover works between turns and mid-stream. A mid-stream error needs wording pi's retry regex recognises, or an abrupt abort (no coupling; recommended default). Router timeouts are mandatory: with a 3 s first-byte timeout pi recovered in 3.4 s; without it, 300 s.
- A node that dies mid-stream while its breaker is closed must be avoided by that session's retry (bug found and fixed).
- Pins must persist across router restarts or sessions move nodes.
- The router serves **both** protocols per role-type with about 150 extra lines, validating one protocol per role at load.
- Messages path: a router 529 `overloaded_error` before the first byte, or an `overloaded_error` SSE event mid-stream, makes both pi and Claude Code retry; an `api_error` mid-stream makes Claude Code fall back to a non-streaming call.

### 3.3 Coord and guard (S5)
- Blocking, race safety (8 rounds, zero overlapping holds), fail-closed on outage, per-call overhead about 0.4 ms (socket) to 1 ms (HTTP), crash recovery from the event log, and `rmcp` interoperability with pi's MCP client all verified, with scripted models only.
- A lease is live only if **both** wall and monotonic clocks agree (a backward wall-clock jump extended leases under wall-only expiry; simulated in code, not by changing the OS clock).
- The guard cannot stop a child launched without it. A textual `-ne` check was evaded; a dispatcher-side watcher killed the unregistered process in 1.2 s; a static `sandbox-exec` profile denied the write. A wrong extension filename or a throw in `session_start` leaves pi running unguarded with exit code 0, so a startup self-check is mandatory.

### 3.4 Delegation (S6)
- Verified against a scripted model: delegate end to end, idempotent completion, confinement (absolute paths, `../`, symlink, bash redirects, `git -C`, branch and ref manipulation), named-resource contention (`blocked` with holder), release under a lease with a dependency gate, merge conflict giving `blocked: merge_conflict` with paths and branches intact, and clean failure paths (`router_down`, `coord_down`, child killed, child hung, orchestrator crash).
- A real Qwen3 1.7B delegated correctly but the child never created the file, and the board reported `done` with no changes. **Done cannot mean "process exited 0".**
- The per-worktree `sandbox-exec` profile costs 20 to 40 ms and is what lets the guard allow unclassifiable commands (`npm test`, `make`). pi-subagents has no option to wrap the child command; the spike used a PATH trick that depends on an undocumented fallback. One of about 9 parallel runs failed once and did not reproduce.

### 3.5 Engines (S7 and the Ollama re-run, versions recorded)
| | Ollama 0.35.1 | llama-server 0.5.0 | mlx_lm 0.32.0 |
|---|---|---|---|
| `/v1/messages` | yes (stream fixed vs 0.15.4) | yes | no |
| Thinking field | `reasoning` | `reasoning_content` | `reasoning` |
| Cached tokens in usage | yes | yes | yes |
| Context overflow | **silent truncation** (4096 default, cut to about 2050), HTTP 200 | explicit 400 `exceed_context_size_error` | none; memory grows |
| Concurrency | 1 slot, serial | 4 slots | continuous batching |
| Load signals | `/api/ps` only | `/health`, `/props`, `/metrics` (`/slots` wakes a sleeping server) | none |
Also: llama.cpp 0.5.0 returns 500 for malformed JSON (pi retries it); mlx_lm drops the connection on invalid requests; a cancelled MLX prefill is not honoured. brew llama.cpp is one release behind (v0.6.0 published 2026-10-05).

### 3.6 Claude Code and other harnesses
- **Claude Code** speaks only Anthropic Messages. Through the router it completed a real tool loop on llama-server in an isolated environment (default prompt: 3 turns, 452 s). It sends `x-claude-code-session-id` and `x-claude-code-agent-id`, accepts arbitrary model strings, sends about 11 beta headers and mid-conversation `system` messages, and tolerates unknown fields on both engines. Prefill of its roughly 15K-token prompt dominates latency on this hardware.
- **DeepSeek Harness (`dsh` 0.2.0-rc.2, developer preview):** blocks before execution and covers subagents automatically with one listener; per-agent env works; but no session header on its chat and Anthropic routes, delegation needs internal APIs, a guard plugin that fails to load fails open, and children share one process.
- **opencode:** not run. Research flagged that its block hook may not fire inside subagent sessions.

### 3.7 Apple on-device model (S9)
Real model, 4096-token context, about 0.5 s per classification call, serial concurrency, structured output via guided generation, emulated tool calling. Works behind our own OpenAI-compatible shim; existing open-source shims did not qualify. Needs a lean pi profile (pi's default prompt overflows it), a per-node max-output override (pi sends `max_completion_tokens: 1` for 4096-token windows), admission control, and refusal handling. Fits short summaries, tagging and classification, not coding or tool loops.

### 3.8 Capability research (S10)
A measured calibration probe (stdlib Python) ran against Ollama and llama-server. Declared context must come from the loaded context, not the trained one. **System One** models (TypeSafe's Jev, per vendor material, not independently tested: typed questions in, typed probabilities out, managed API only) map to an endpoint kind `decide`. Open equivalents (for example GLiNER2.5-Decide) were not run. One agent summary mentioned a `/v1/systemone` endpoint in llama.cpp 0.6.0; it was not corroborated and is ignored.

## 4. Corrections to the baseline specs (applied)
- `registry.md`: replace flat node fields with the three layers; add `requires`, endpoint kind and `exclusive` as a list; `max_ctx` is the loaded or effective context; correct the `sessionAffinityFormat` assumption: pi 1.0.3 sends `x-session-affinity` only when `compat.sendSessionAffinityHeaders: true` is set together with `sessionAffinityFormat: "openai-nosession"`.
- `dispatcher.md`: add a capability and endpoint-kind filter before health and context; router timeouts are mandatory; failover only before the first byte; protect against silent truncation; compaction calls are sessionless.
- `coordination.md`: leases are for worktree-confined writers' shared resources and `release`; path-glob leases deferred; add the dual-clock rule; the hook has no timeout so coord calls carry their own deadline.
- `architecture.md`: "one process per role" is false for foreground subagents; add registration check, startup self-check and sandbox as defence in depth; the Unix-socket-only coord transport changes to socket plus loopback HTTP.
- `roadmap.md`: the open question about progress while pi-subagents blocks is answered (async mode); phase 5 plan B before the `pi-acp` fork.
- `implementation.md`: spikes used Python and Node for tooling; add an exemption sentence for on-demand verification tools.

## 5. Story changes this implies (by Ceetrix story number)
- **Router (10 to 23, 51, 52):** per-node request patches (for example thinking suppression differs by engine); the Ollama `normalise_sse` rewriter is an off-by-default guard only; an explicit abort or a recognised error on mid-stream failure; compare streaming `message_start` and final input tokens to detect Ollama truncation; classify llama.cpp 500s on malformed JSON and MLX connection drops by body shape.
- **Pin and affinity (12, 52):** pin on `x-session-affinity` (OpenAI and Anthropic paths); accept sessionless compaction calls; persist pins.
- **Coord and guard (24 to 31, 46):** socket plus loopback HTTP; dual-clock rule; confinement replaces per-edit leases; the guard's `guard_registered` check and startup self-check are mandatory; `-ne` bypass needs watcher plus sandbox.
- **Delegation (39 to 43):** async children, idempotent completion, pointer-only results, an acceptance check so `done` means changed (or gated), allow-list the injected report path, kill the process group, orphan reconciliation.
- **Leases and worktrees (33 to 38):** only `release` and named resources are leases; abandon checkpoint-commits and keeps the branch; releaser merges via a structurally constrained guard.
- **Credentials (47, 53):** pi-subagents has no per-run env, so use the bindings channel or separate launches.
- **New work:** the capability registry (model, server, measured layers), a calibration probe story, per-capability smoke canaries, the `decide` endpoint kind, hosted-node custody and budget, and an Apple on-device node story (optional).
- **Pinning (50):** pin pi 1.0.3 and pi-subagents 0.76.0; turn the spike harnesses into a compatibility check; record engine versions and freeze engines in tests.

## 6. Not tested / open
- CUDA, Vulkan, Strix Halo, models above 1.7B; real-model tool-calling quality; real-engine failover and timeouts (fakes only); MLX through the router; opencode and DeepSeek Harness through the router.
- Interactive Claude Code; `count_tokens` on a real Claude Code path; whether mid-conversation system messages affect the model.
- Hook order across several extension files; hook failures other than `tool_call` and `context`; more than 2 parallel writers; concurrent releasers; Linux sandboxing; laptop sleep and real OS clock jumps.
- The one parallel-delegation flake; the MLX server wedge seen once under heavy load.
- Vendor claims for Jev and GLiNER2.5-Decide are unverified.

## 7. Version pins in force
pi 1.0.3, pi-subagents 0.76.0, pi-acp 0.0.34 (not used), Ollama 0.35.1 (frozen copy), llama.cpp 0.5.0 (brew; v0.6.0 exists), mlx-lm 0.32.0, `rmcp` 3.5.1, `dsh` 0.2.0-rc.2, Hummingbird 2.27.0 (Apple shim). Re-check every pin before the next spike or release.
