# Pinned toolchain and libraries

Every pin has its evidence and a status. Status values: PROVEN here (built in the scratch check of story 128 task 1), PROVEN on a prototype (spike s2 only), NOT TESTED, FAILED. A pin without evidence is invalid.

Check run: 2026-10-07 on macOS / Darwin 25.6.0 arm64 and on Linux aarch64 (Ubuntu 24.04 VM through colima, container image rust:1.94.0), both rustc 1.94.0 (4a4ef493e 2026-03-02). A scratch crate with all pins as exact (`=`) requirements and the features the proxy will use resolved and built clean with `cargo build` (37 s on macOS, 63 s on Linux, no override, no patch).

| Pin | Version | Evidence | macOS arm64 | Linux |
|---|---|---|---|---|
| rustc | 1.94.0 | spike s2 README; installed toolchain | PROVEN here (builds all pins) | PROVEN here (builds all pins) |
| tokio | 1.53.2 | spike s2 README (deterministic paused time); scratch build | PROVEN here (build only; time behaviour PROVEN on a prototype) | PROVEN here (build) |
| hyper | 1.12.0 | spike s2 README; scratch build | PROVEN here (build) | PROVEN here (build) |
| hyper-util | 0.1.21 | spike s2 README; scratch build | PROVEN here (build) | PROVEN here (build) |
| tower | 0.5.3 | spike s2 README; scratch build | PROVEN here (build) | PROVEN here (build) |
| axum | 0.7.9 | spike s2 README; scratch build | PROVEN here (build) | PROVEN here (build) |
| reqwest (test kit only) | 0.13.5 | spike s2 README; scratch build | PROVEN here (build) | PROVEN here (build) |
| yaml_serde (registry file parser, story 125) | 0.10.7 | story 125 task 2 prototype: nested paths kept, every unknown key reported, empty and comments-only file is null, duplicate keys and tabs are parse errors (tests registry_schema_test, registry_file_test); fork of serde_yaml maintained by the YAML organisation | PROVEN here (build and tests) | NOT TESTED |

Not exercised: axum 0.8.9. It stays unpinned until a separate change re-runs the spike s2 tests (owner decision pending).

Limit: Linux was checked on aarch64 only; x86_64 Linux is NOT TESTED.
