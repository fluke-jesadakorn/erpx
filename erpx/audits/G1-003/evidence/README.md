# G1-003 worker runtime evidence

This packet is a partial G1-003 delivery, not a G1 gate pass.

## Direct evidence

- `erpx/apps/worker/src/lib.rs` provides environment configuration, distinct-pool startup migration, runtime loop/rebuild APIs, and the fail-closed handler guard.
- `Customer360Projection` and `Customer360OutboxHandler` provide one concrete read-model projector with receipt/checkpoint-aware rebuild and event handling.
- `erpx/apps/worker/src/main.rs` provides the executable `run` entry point.
- `cargo check -p erpx-worker` passed.
- `cargo test -p erpx-worker` passed.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed.
- The ignored SQLx integration test passed against explicitly disposable, physically distinct PostgreSQL databases and the databases were dropped afterward.
- A fresh two-database ERPX server boot returned healthy, ready, and metrics responses; the process and disposable databases were removed afterward.

## Explicit exclusions

Remaining business-context handlers/projectors, external publishing, migration jobs, and physical read-database replacement/switching remain open. Unsupported executable handler selection fails before claiming events.
