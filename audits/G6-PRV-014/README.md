# G6-PRV-014 — scheduler status read-model projection writer

## Delivered behavior

Added `SqlxTenantProjectionStatusProjectionStore`, an explicit control-plane
projection writer that copies only validated scheduler status metadata from the
control-write pool into the separate rebuildable control-read table. It
supports idempotent version-ordered upserts, bounded/redacted error material,
read-model clearing, and a deterministic full rebuild seam. Invalid counters,
references, kind/state combinations, and secret-shaped error material fail
closed before persistence or are redacted.

The packet also includes focused infrastructure tests for invalid status
rejection and safe DTO shape. It does not silently register a worker or claim
scheduler jobs; production scheduling/refresh cadence remains an explicit
composition-root dependency.

## Files changed

- `crates/infrastructure/src/tenant_projection_status_projection.rs`
- `crates/infrastructure/src/tenant_projection_scheduler.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/tenant_projection_status_read.rs`
- `audits/G6-PRV-014/README.md`

## Verification

From `erpx/`:

```text
cargo fmt --all — PASS
cargo test -p erpx-infrastructure --test tenant_projection_status_read — PASS: 2 tests
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings — PASS
```

The control-read migration and migration assertions are independently covered
by G6-PRV-013. The full integration-wave verification is recorded in the
execution ledger.

## Unfinished work and rollback

Worker registration/cadence, incremental scheduler-change publishing,
projection freshness, replacement-read switching, and Platform Operations
rebuild/action controls remain open. Rollback removes only the additive writer,
exports, focused test, and audit; retain the rebuildable read schema if it has
been populated until a reviewed retention decision is made.
