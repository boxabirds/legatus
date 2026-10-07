# Spike 2: can the real router run deterministically under tokio paused time?

Verdict: **GO-WITH-CHANGES** for virtual mode. 27 tests pass (`cargo test -- --test-threads=1 --nocapture`; log `logs/full-run.txt`).
Versions (installed = latest unless noted): tokio 1.53.2, hyper 1.12.0, hyper-util 0.1.21, reqwest 0.13.5, tower 0.5.3, axum 0.7.9 (latest 0.8.9; kept at the prototype's 0.7), rustc 1.94.0.
Everything is fake upstream/in-process; no real model, no lock taken. Code: `src/` = copy of s1 router (`router.rs`, `state.rs`, `config.rs`, originals in `orig/`) with seams `clock.rs`, `transport.rs`, plus toy `lease.rs`, `journal.rs`, `fake.rs`, `sim.rs`. Tests: `tests/f1_f3_router.rs`, `f2_f6_f7_io.rs`, `f4_f5_lease.rs`, `f8_scaled.rs`.

## Results

| Probe | Status | Evidence (key output) |
|---|---|---|
| F1 sleep/timeout/interval via tower `oneshot`, in-memory upstream | VERIFIED | The real copied `chat` handler: first_byte timeout 1500 ms fails over to n2, response at exactly 1600 ms, chunks at 1600/1700; breaker Open then HalfOpen after the 10 s cooldown; mid-stream idle timeout emits the SSE error at exactly 600 ms; serial node queues 3 requests to 10/20/30 s. `[F1 router/oneshot] virtual=15.1s real=3.3ms`, `[F1 queue] virtual=30s real=1.9ms` |
| F2 real sockets break paused time | VERIFIED (hazard) | Real TCP server thread needing 200 ms real: client `timeout(30s)` fires, `virtual=30s real=1.9ms`. Worse: server and client in the SAME runtime over loopback with zero think time: clock advanced in 300/300 requests, 290-294/300 hit a 1 s timeout (3 runs). Real sockets are unusable under pause. |
| F2b in-memory incoming side | VERIFIED | Real axum router + hyper http1 over `tokio::io::duplex`: chunks at 7000/9000/11000 ms virtual, 1.2 ms real. Real HTTP parsing is fine; only the OS socket is the problem. |
| F3 SSE proxy, per-chunk delays | VERIFIED | 6 chunks 250 ms apart: byte-identical, client timestamps exactly 250..1500, two fresh runtimes give identical bytes, timestamps and upstream log. Slow reader (1 s/chunk): upstream emission `[10,20,30,1020,2020,3020]`, i.e. end-to-end backpressure is deterministic. |
| F4 sweeper + lease timers | VERIFIED with 3 caveats | Idle TTL 1800 s ended by a 5 s sweeper at exactly 1 800 000 ms, 1801 virtual s in 13-20 ms real; max-life 14400 s with 600 s touches over 18000 virtual s in 128-198 ms. Same-instant order is FIFO by timer registration, identical over 20 fresh runtimes. Release at 1800 s-1 ms and at 1800 s exactly wins; at +1 ms the sweeper wins (`lease_ended`). Caveats below. |
| F5 gates + seeded schedules | VERIFIED | `SimPoints` (arm / hold / `wait_held` / release by index) forces: request landing in the sweeper's found-due gap (buggy `recheck=false` ends the lease under an in-flight request, `recheck=true` survives); release-first vs expiry-first; two HTTP acquires for the last slot in both orders (A wins / B wins). 300 seeded schedules: winners `{A:117,B:116,C:67}` in 65 ms; seeds replay identically; an unreleased gate becomes `Stalled`, not a hang. |
| F6 std Instant/SystemTime/thread::sleep | VERIFIED | Not virtual: `sleep(3600s)` shows <1 s on `std::Instant`; `thread::sleep(50ms)` leaves the tokio clock at 0. The prototype's `(std Instant + d).into()` deadline fires immediately after a virtual jump (`timeout_at(std+1s, sleep(500ms))` timed out). |
| F7 journal in memory vs files | VERIFIED | Memory: 1000 lines, 500 virtual s, 84 ms. Tempdir file with fsync per line, written inline: deterministic, same logic, but 2.7 s real. Via `spawn_blocking`: 3-5.7 s real, and the last write was still in flight when the driver finished (999 vs 1000 lines) until another yield. |
| F5b spawn_blocking | VERIFIED (better than feared) | Tokio 1.53 does NOT auto-advance while a blocking task is pending: 10 s virtual timeout did not fire, virtual 0, real 101 ms. Safe, but the test pays real time. |
| F8 scaled mode over real sockets | VERIFIED for what it covers | Same router + lease code, real sockets and clock, 1800 s idle TTL as 300 ms (scale 6000), sweep clamped to 10 ms (5 s/6000 = 0.8 ms is below usable timer jitter): 15 trials, expiry lateness min -0.1, p50 6.8, p99 11.1 ms, test took 4.6 s. SSE through real sockets byte-identical, chunk times 47/87/130/171/214 ms for a nominal 40/80/..200. |

## Findings that change the design

1. **`tokio::time::advance(d)` is ONE jump.** Everything due fires at the final `now`. A lease touched every 600 s with idle TTL 1800 s is wrongly expired after `advance(3600s)` (`live=false`) but alive when the driver does `sleep(3600s)` (`live=true`, stepwise auto-advance). Story 89's `advance()`/`step_to_next_event()` must be built on a driver `sleep` (and `run_until` on sleeps in small steps), not on `time::advance`. Tokio has no public "next timer deadline" API.
2. **Timer resolution is 1 ms; deadlines round UP.** Story 90's `SUBTICK_NS_MAX` and "seeded sub-tick delays of simulated nanoseconds" are wrong: offsets under 1 ms do not give a controllable order (offsets 0..50 ns reversed gave `[5,0,1,2,3,4]`). Use whole-millisecond offsets (we used 0..7 ms: 3 actors all win across seeds). Same-instant order otherwise is FIFO by timer registration, deterministic.
3. An armed point holds EVERY task reaching it, including earlier sweeps (my first gap test passed vacuously for that reason). `PointController` needs arm-after-N-reaches or an arm-once mode.
4. Real-socket hazard is worse than "can": it is nearly always on loopback (F2). The no-socket rule is mandatory, not hygiene.
5. Monotonic clock: if all router code uses `tokio::time::Instant`, no `Clock` trait is needed for it (the s1 change to `state.rs` was one import line). Only wall time needs a trait.

## Seams the real router (story 10, and 12/24/30/52) must expose

- `UpstreamTransport`: one method `send(url, headers, body) -> Future<Result<{status, headers, Stream<Result<Bytes,_>>}, _>>`; the call site in `chat` is the only place that changes (done in `src/router.rs`, ~10 lines). Production impl wraps reqwest.
- Monotonic time: use `tokio::time::Instant` everywhere (forbid `std::time::Instant` by clippy `disallowed-types`); deadlines via `timeout_at` / `sleep_until` of tokio Instants only; no `std::thread::sleep`.
- `WallClock` trait (log stamps, max-lifetime wall rule, journal stamps); `SimWall` here derives from the paused clock plus a jump offset.
- `Journal` trait (append of one line) fed by a channel to a writer task; the writer must be an in-task call in virtual mode (no `spawn_blocking`, no `tokio::fs`).
- The incoming side is just `axum::Router` driven by `oneshot` (or hyper over `duplex` when wire behaviour matters); the router must expose `build(app) -> Router`, not own the listener.
- Background tasks (sweeper, journal writer, poller) spawned on the current runtime and returned as handles; sweeper deterministic order (sort ids; `HashMap` iteration order is random per process).
- `sim_point!` annotation in: sweeper after due, admin acquire before table, begin_request, end_request, journal append. Cost verified: one registered name plus an await that is a no-op when unarmed.
- Process startup (`main`) must not read files/bind sockets inside the library path used by the harness (the s1 `State::new` reads `pin_store` with `std::fs`; keep it behind the journal trait).

## What each mode can and cannot prove

Virtual (paused, in-memory): exact instants and boundaries (release at TTL-1 ms / TTL / TTL+1 ms), same-instant ordering, forced interleavings at named gaps, thousands of seeds in milliseconds, replay by seed, 5 virtual hours in 0.2 s. Cannot prove: real sockets, half-close/RST/backpressure of the OS, fsync/rename/torn writes, real SIGKILL, thread-based code, anything inside a gap that has no `sim_point`, sub-millisecond ordering, production-value behaviour in real time.
Scaled (real sockets, TTL 300 ms): the real wire path and real file I/O; bands only (lateness 0..11 ms here, quantised by a 10 ms sweep); same-instant race `release at TTL` split 25 released / 15 lease_ended over 40 trials, so races are luck. Cannot prove: exact ordering, boundaries, parameters below timer jitter (5 s sweep compressed beyond ~10 ms), production values.

## Not tested

The real story-10 router (does not exist; only the s1 prototype copy with a toy lease table), `tokio::fs` in virtual mode, multi-thread runtimes under pause (paused time requires current_thread; the F8 scaled runs use multi_thread), axum 0.8, `tokio::select!` fairness (`Builder::rng_seed` exists for it; not exercised), process kill/restart replay (modelled only by MemJournal; no restart test written), macOS only.
