# Spike C: conversation key stability in real harness traffic

All traffic below is REAL harness behaviour against a SCRIPTED FAKE server (`fake/rfake.mjs`, records raw body and headers to timestamped JSONL). No real model, no lock. Model replies (tool calls, text, reasoning/thinking fields) are fake. Versions: `VERSIONS-20261007T065702Z.txt`. Evidence: `runs/run-<UTC>-<label>/` (marks.jsonl = commands, scenario.json, *-requests-*.jsonl, ENV.txt); runs with `INVALID.txt` are kept but not used (port collisions, a sed bug, runaway loops). Per-run key tables: `analysis-20261007T065922Z.txt`. Code: `tools/` (`analyze.py`, `summary.py`, `retries.py`, `volatile.py`, `keyv1.py`, `make_vectors.py`, `*_run.sh`). Vectors: `test-vectors-keyv1-20261007T065515Z.json`.

## Verdict on the gate question: GO-WITH-CHANGES

The derived key K1 (first system + first non-system message) is byte-stable across all turns of one conversation in every harness tested (turns up to 14, tool loops, reasoning/thinking fields, retries, resume). It breaks in exactly one case: compaction (all four harnesses that compact). It does NOT discriminate in two cases: identical first prompts, and Codex (Responses API), where it collides across every conversation in one directory. The "first 512 bytes" rule (K4, Olla) is unusable. A header route exists for pi (with compat flag), Claude Code, opencode, Codex; DSH and plain SDK clients have none, so only the derived key serves them.

## Tested / not tested

| Item | Version (latest) | Status |
|---|---|---|
| pi (OpenAI + Anthropic route) | 1.0.3 (1.0.4) | tested |
| pi-subagents fg, async, parallel workflow | 0.76.0 (0.76.1) | tested |
| Claude Code `-p` and TUI via pty | 2.1.291 (2.1.292) | tested |
| opencode | 1.18.35 (same; brew 1.0.134 stale, unused) | tested |
| DeepSeek Harness | 0.2.0-rc.2 (same) | tested |
| Codex (Responses API) | 0.160.1 (same) | tested: plain, 2-session, resume only. No compaction, subagent, retry, long loop |
| OpenAI node 7.30.0 / python 3.26.0, Anthropic node 0.131.0 / python 1.11.0 | latest | tested (python Anthropic and node Anthropic both ran; only chat shape recorded) |
| Open WebUI, real models, Claude Code interactive subagents, pi `/compact` command | | NOT TESTED (pi compaction was automatic, set via settings) |

## Stability per conversation (within one session, K = same value on every turn)

| Harness | Session header(s) | K1 | K3 | K4 (first 512 B) | K5 (keyv1) | Break |
|---|---|---|---|---|---|---|
| pi chat, 4 and 14 turns, reasoning_content | `x-session-affinity` only with the compat flag; none otherwise | yes | yes | yes | yes | compaction |
| pi Anthropic route | same | yes | yes | NO from turn 2 (`cache_control` moves) | yes | compaction |
| Claude Code `-p`, 4 and 14 turns | `x-claude-code-session-id` (also `metadata.user_id.session_id`) | yes | yes | yes | yes | compaction |
| Claude Code TUI | same | yes | yes | yes | yes | compaction |
| opencode | `x-session-affinity` = `x-session-id` = `ses_...` | yes | yes | yes | yes | compaction |
| DSH | none | yes | yes | yes | yes | compaction |
| Codex | `session-id`, `thread-id`, `x-codex-window-id` (`<id>:0`), body `prompt_cache_key` = session id | yes | yes | yes | yes | not tested |
| OpenAI/Anthropic SDK chat | none | yes | yes | NO (changes every turn while chat is under 512 B) | yes | n/a |

## Distinctness

- Different prompts: K1, K3, K5 differ in pi, Claude Code, opencode, DSH. Same first prompt in two sessions: K1 collides (all harnesses), headers differ.
- K4 constant across ALL conversations of pi (`26a27f52`), Claude Code (`28500fcf`), DSH (`ef162f84`), opencode (`07164748`): the system prompt fills 512 bytes. Rejected.
- Codex: K1, K3 identical for sessions A, B, A (same directory) because the first non-system input item is `<environment_context>`; the real prompt is item 2. Fixed in K5.
- Size: system text is 2.7 KB (DSH), 3.1 KB (pi), 6 KB (Claude Code), 9.7 KB (opencode), 19.6 KB (Codex). The working directory sits at char 8663 in opencode and 17198 in Codex, beyond the proposed 8192 limit.

## Compaction (K1 breaks in all four)

| Harness | Compaction request | First message after |
|---|---|---|
| pi (auto, settings) | side call, no session header, own system "context summarization assistant" | "The conversation history before this point was compacted..." (session header unchanged) |
| Claude Code (`/compact`, TUI and `-p`) | same system, tools, K1 and header as the main thread (prefix fork) | "This session is being continued from a previous conversation..." |
| opencode (auto) | side call, own system, session headers present | "What did we do so far?" |
| DSH (auto) | same system and first message as main (K1 equal) | "This is an automatically generated checkpoint..." |

Header route: key unchanged. Derived route: a new key, so a new table entry. Old cache is useless anyway, so the cost is small.

## Subagents, side requests, resume, retries

- pi-subagents (fg, async, parallel): each child has its own `x-session-affinity`, own system ("You are a child subagent, not the parent"), own first message. Two children with the same task collide on K1, differ on header.
- Claude Code: child keeps the parent's `x-claude-code-session-id` and adds `x-claude-code-agent-id` (distinct per child); a child of a child also sends `x-claude-code-parent-agent-id`. Background child runs interleaved with the parent. K1 differs (child prompt is the first message). Spec 3.3 (session + agent id) is confirmed; its ASSUMPTION is now PROVEN.
- opencode: child has its own `ses_` id. DSH: no header, child K1 differs.
- Side requests: Claude Code TUI sends a title call per user prompt (no tools, own system, same session header; real use goes to a small-model alias); opencode and DSH one title call at start (own system; opencode with headers, DSH without); pi sends none except the compaction call (no header); `-p` Claude Code sends none.
- Resume (`--continue`, `--session-id`, `resume --last`): pi, Claude Code, opencode, DSH, Codex keep header and K1. Claude Code re-sends the stored first message, so changed git status between runs does not change K1 (run `cc-s10`).
- Retries after scripted 503: body byte-identical and same session header in pi (2 s, 4 s), Claude Code (0.6 s, 1.1 s), opencode (2.3 s, 4.4 s), DSH, both SDKs (`x-stainless-retry-count` 0,1,2 differs, not part of any key). `x-client-request-id` equals the session id in pi, not a per-request id.

## Volatile content near the start

| Field | Harness | Effect |
|---|---|---|
| `x-anthropic-billing-header: cc_version=...<3 hex>; cc_entrypoint=...` first system block | Claude Code | hex is a function of the first prompt; differs per conversation, child and after compaction; stable within a conversation. `cch=` not seen in 2.1.291 (risk on newer versions) |
| messages[1] `role:"system"` (environment, date, skills) and later `<total_tokens>` system messages | Claude Code | mid-array, after the first user message; ignored by the rule; content shape flips string vs blocks with `cache_control` between turns |
| memory path with config dir and cwd in system prompt | Claude Code, pi, opencode, DSH | per user and directory, stable per conversation |
| `<environment_context>` first user item; `msg_<uuid>` ids on items | Codex | stable per session; ids per item |
| `Current runtime context` user message (3rd) | DSH | stable in tests |
| `<system-reminder>` blocks (CLAUDE.md, gitStatus, attribution) in first user message | Claude Code | stable across turns and resume. Stripping them changed no stability result and would merge sessions in different repos |
| date, time | none in the first messages of any harness except the Claude Code env message (position 2) | not observed to change |

No volatile field broke K1 within a conversation. K5 matters for distinctness (Codex), shape flips and billing-hash robustness, not for the observed turn stability.

## Recommended canonicalisation (`tools/keyv1.py`, 19 passing vectors)

1. System text: top-level `system` (Anthropic) minus blocks starting `x-anthropic-billing-header:`; `instructions` (Responses); plus the leading run of `system`/`developer` messages. Joined with one space.
2. First part: scan non-system messages in order; collect text; stop at the first message that has text outside wrapper blocks. Wrapper-only messages (text starting `<environment_context>`, `<system-reminder>`, `<user_instructions>`, `<skills_instructions>`) are kept and joined in front. Later `system` messages and tool items are skipped.
3. Text: text parts joined with one space; other part types become `[type]`; thinking, tool_use, tool_result, ids and `cache_control` are ignored; NFC; whitespace collapsed; trimmed; per-part limit.
4. Canonical string `v1 US S<system> US F<role> US <first>` (US = U+001F); the proxy hashes it with HMAC.

Vectors V01 to V17 cover whitespace, parts versus string, developer role, NFC, billing hash, mid-array system messages, thinking blocks, compaction (key changes), Codex A/B, and the limit.

## Design consequences for `03-affinity-and-keys.md`

1. Keep header-first; keep body hash as the fallback. Section 4 default is validated for turn stability.
2. Section 4.5: add that K4 (first 512 bytes) collides across all conversations of four harnesses and is unstable on short chats; do not offer it.
3. Section 4.3: raise `key_text_limit` for the system part to at least 32768 (or hash whole text up to 256 KiB); 8192 drops the cwd in opencode and Codex. Keep a small limit for the first part.
4. Section 4.1/4.2: add the wrapper-only skip rule and the billing-header drop; add `/v1/responses` (`instructions`, `input`) to the part table.
5. Section 3.1: add `session-id` and `thread-id` (Codex), body field `prompt_cache_key` (Codex), `x-claude-code-parent-agent-id`; note `x-client-request-id` is NOT per request; `x-session-affinity` is already sent by opencode and pi.
6. Compaction: say that the derived key changes at compaction by design; pi and opencode compaction calls differ in key (pi has no header), Claude Code and DSH compaction calls share the main key.
7. DSH and SDK clients have no header: affinity there relies on the derived key; same-prompt sessions collide. State it as a known limit.
8. 3.3: mark the agent-id ASSUMPTION PROVEN (distinct per child, parent id header for nesting).

## Quirks and mistakes (honest)

- Ports 18920 to 18922 were used by three Claude Code runs (outside the 18900 to 18919 range; each run ended). Early pi batches hit one hardcoded port (INVALID). Two compaction runs looped until timeout (kept gzipped, marked). Some early pi `marks.jsonl` have second precision, so labels in `pi-s3*` and `pi-s6` runs are approximate; session headers are authoritative.
- Fake usage numbers are not real; pi, opencode and DSH compaction thresholds were forced with settings and fake token counts.
- Disk: runs/ about 1 GB.
