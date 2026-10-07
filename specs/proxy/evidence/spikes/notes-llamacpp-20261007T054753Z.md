# llama.cpp server (HEAD b11460 file refs; installed b11146 has identical logic at shifted lines, e.g. create_checkpoint 2309, restore ~3349-3380, do_checkpoint 3460/3625)
All READ-FROM-CODE unless marked (inferred).
## Defaults
- --ctx-checkpoints (-ctxcp, alias --swa-checkpoints) = 32 per slot: common/common.h:635, arg.cpp:1701. 
- --checkpoint-min-step (-cms) = 8192 tokens: common.h:637, arg.cpp:1709. (no --checkpoint-every-n-tokens flag exists)
- --cache-ram = 8192 MiB: common.h:638, arg.cpp:1719. --slot-prompt-similarity 0.1: common.h:701. cache-idle-slots default on (arg.cpp:1737). n_ubatch 512: common.h:457.
- No 64-token minimum in this code (grep `>= 64` empty). Issue 22384 (closed) described one at an older commit.
## When checkpoints are made (tools/server/server-context.cpp)
- Only for completion tasks, only if seq_rm type is FULL/RS or n_swa>0 (3708-3721). Hybrid GDN = FULL (common.cpp:1553-1585; n_rs_seq default 0).
- Created BEFORE llama_decode of a batch, so a checkpoint at n_tokens_start holds state after tokens [0,n_tokens_start) (3901-3904, comment).
- Batch is split (3819-3850) at: (a) user-message starts (needs message spans from chat parser; qwen3-coder parser defines them: common/parsers/qwen3-coder.cpp:32-39, "<|im_start|>user"; tool responses are role TOOL) if it is the last user message OR > last_cp + 8192; (b) at 4+n_ubatch=516 and 4 tokens before the prompt end.
- Mid-prompt checkpoints skipped unless batch starts a user msg or near end (3875-3878); spacing rule (3894-3898): last user msg, near-end, or > last+min_step.
- Net per prompt (L tokens): checkpoints at {L-516, L-4} + user-start positions (>=8192 apart, plus last user msg). NO checkpoint is taken after generation (create_checkpoint has one call site, 3904). Mid-gen state L+G exists only as the live slot state (or saved whole into --cache-ram).
- create_checkpoint (2513-2577): when list full(>=32): erase older ones within min_step of a predecessor (unless same task), then FIFO-evict front; same-n_tokens duplicate superseded. Data = recurrent state only (LLAMA_STATE_SEQ_FLAGS_PARTIAL_ONLY); size reported in log.
## Choosing on a new prompt (3495-3640)
1. n_past = common prefix of slot.prompt.tokens (L+G stored incl. generated) and new tokens (3442).
2. pos_next = n_past; pos_min_thold = n_past - n_swa(0) - (has_new_tokens?0:1). If recurrent state pos_min (=L+G-1, one cell) >= thold (i.e. state is at/after n_past: cannot roll back) -> search checkpoints newest->oldest: accept first with pos_max <= pos_next and (pos_min < thold or pos_min==0) (3580-3592). For recurrent cell pos=n-1, so condition = checkpoint n_tokens <= D.  [pos semantic: inferred from llama-memory-recurrent.cpp:390 + hybrid max-of-mins llama-memory-hybrid.cpp:172]
3. Hit: load recurrent state, n_past = ckpt.n_tokens (3598-3607). Miss: "forcing full prompt re-processing", n_past=0 (3611-3615).
4. All checkpoints with pos_max > pos_next erased (3619-3628) -> after a divergence at D, later checkpoints are GONE.
5. Then seq_rm(slot, p0=n_past, -1) (3665). For recurrent memory a partial seq_rm with p0<=state pos DESTROYS the state (tail_id=-1) and still returns true unless rollback via n_rs_seq (llama-memory-recurrent.cpp:195-224); hence the checkpoint step must precede it. If n_past==L+G exactly (state==live) nothing is needed (pos_min < thold): 0 recompute (+1 token if n_past==N: 3635-3640).
6. If n_past == N, n_past-- (one token reprocessed) (3636-3640).
## Slot / host cache
- Slot choice: LCP similarity > 0.1 (1692-1740), else LRU; if f_keep<0.5 or LRU, save slot to prompt cache (1770-1786).
- --cache-ram: server_prompt_cache (server-task.cpp:1700-1920). prompt_save stores WHOLE seq state (LLAMA_STATE_SEQ_FLAGS_NONE, incl recurrent+KV at L+G) plus the prompt's checkpoint list (server-context.cpp:308-331; alloc 1733-1793). load picks entry maximizing f_keep (lcp/stored_len, must be >=0.25) and f_sim (lcp/new_len) vs current slot (1807-1855); restored prompt carries its checkpoints, then steps 1-6 apply. Entries evicted oldest-first by byte/token limit (1760-1790, 1893-1905). Entry contained in the new prompt is dropped (1750-1760).
## Closed form
Let N=new prompt len, D=common prefix with stored seq (len L+G), C=checkpoint n_tokens set of that slot/entry. 
 c*(D) = max{c in C: c <= D}, or 0 if none; if D == L+G (live state intact) c* = L+G.
 recompute(D) = N - c*(D)   (+1 if c*==N).
 Typical agent turn (history replayed, template keeps generated text): D=L+G -> recompute = N-(L+G).
 Template strips reasoning/ rewrites assistant turn: D in [L-4, L+G) -> c*=L-4: recompute = N-L+4  (loses G tokens).
 D in [L-516, L-4) -> N-L+516. Below that -> latest user-start checkpoint <= D (>=8192 spacing), else N (full).
## Observable
- timings.cache_n = n_past used (slot.stats.n_prompt_cached, 3639), timings.prompt_n = tokens actually prefilled (n_prompt_processed), prompt_ms, predicted_n... (server-common.cpp:84-96). cache_n+prompt_n ~ N (cache_n is the post-checkpoint n_past, so recompute = prompt_n; checkpoint restore => cache_n = c*). Non-stream result also has tokens_evaluated (=N), tokens_cached = slot.prompt.n_tokens() (server-context.cpp:2250-2251; server-task.cpp:349-356) = N+G, NOT the reused count. Stream: return_progress gives prompt_progress{total,cache,processed}. n_prompt_tokens_cache also in /slots (server-context.cpp:740).
## 18000 vs 17999 (default flags)
Step function (staircase), flat between checkpoints; the 1-token shift is a cliff only if a checkpoint (or L+G) sits exactly at 18000 (then jump = distance to the previous checkpoint: 4, 512, up to >=8192, or whole prompt if none). Not a sawtooth. Biggest realistic cliff: D=L+G (full replay) vs D<L+G (any edit in generated text): loses G+4 tokens.
