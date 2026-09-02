# ERPX warm Rust UI loop acceptance — 2026-08-10

## Scope

- Host: Apple M3 Max development workstation.
- Acceptance: median warm `cargo xtask check-affected` time is at most 15 seconds for a leaf UI edit.
- Exclusions: first/cold compilation and root manifest, lockfile, toolchain, protobuf, migration, or shared-contract escalation.
- Each recorded total includes selector, changed-file rustfmt, Cargo, and snapshot update overhead.

## Baseline and cache migration

- Before the split, a warm Console Web check was approximately 8.8 seconds when its graph was already valid; the retained cold Dioxus build observation was approximately 149 seconds.
- `cargo xtask cache prune-debug --dry-run` selected only the legacy root `target/debug` group, then the one-time authorized cleanup removed 12.6 GiB. Dioxus, native runtime, and n8n state were not removed.
- The first post-cutover global `check-affected` correctly escalated to the workspace and completed in 31.02 seconds (`selector=0.44s`, `rustfmt=0.63s`, `cargo=28.98s`). This is not the quick-path acceptance case.

## Three-run warm measurements

| Edit boundary | Run 1 | Run 2 | Run 3 | Median | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| Finance feature view | 9.46s | 1.54s | 1.59s | 1.59s | Pass |
| Support presenter | 2.35s | 1.61s | 1.57s | 1.61s | Pass |
| Shared UI-kit button | 4.09s | 2.90s | 2.83s | 2.90s | Pass |

Each run changed a harmless source comment in the owning Rust file so Cargo observed a real leaf edit. The quick selector chose exactly one package library target and did not link tests, scan architecture, or traverse reverse dependencies.

## Post-measurement cache evidence

`cargo xtask cache status` reported:

- 32.1 GiB across 11 managed groups under the 128 GiB budget.
- 396.8 GiB filesystem free space, above the 64 GiB floor.
- Root `workspace--debug`: 14.8 GiB and retained as `warm-validation`.
- Dioxus Web, target-specific Web, hot-patch, and native runtime groups retained as `active` pins.

The result confirms that active Dioxus/runtime pins and the warm root validation pin coexist without forcing validation-cache eviction.

## Final handoff evidence

- `cargo xtask test-affected` passed and advanced the thorough-lane snapshot after exercising the global escalation, workspace tests, mapped domain harnesses, and architecture scan.
- `cargo xtask verify` passed after the refactor, including warnings-denied Clippy, full workspace tests, Web/Server Console checks, the Dioxus Web build, SQLx acceptance, and runtime smoke checks.
- After the final gate and warm runtime restart, cache usage was 49.4 GiB across 14 groups with 379.9 GiB free. The 24.6 GiB root validation cache remained `warm-validation`, while Dioxus and runtime groups remained `active`.
- `cargo xtask status` reported the server, Web hot-patch process, infrastructure, and all 59 required workers running; ERPX remained available on port 3004.
