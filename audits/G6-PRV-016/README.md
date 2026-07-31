# G6-PRV-016 — projection publication freshness

## Delivered behavior

The control-read projection now records `published_at` when the validated
scheduler-status writer upserts a row. The control-write source adapter remains
backward-compatible by treating source `updated_at` as its pre-publication
transport timestamp; the read-side writer replaces that with the actual read
model publication time. Platform Operations exposes both timestamps and a
single response-wide freshness classification:

- `fresh`: publication age is at most 300 seconds;
- `stale`: publication age is greater than 300 seconds;
- `unknown`: publication time is in the future, indicating clock skew or bad
  metadata.

The classification is computed once per response, uses an inclusive exact
boundary, and does not expose source payload, claims, workers, credentials, or
tenant connection details.

## Files changed

- `crates/migration/migrations_read/0005_tenant_projection_freshness.sql`
- `crates/migration/src/lib.rs`
- `crates/infrastructure/src/tenant_projection_scheduler.rs`
- `crates/infrastructure/src/tenant_projection_status_projection.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/tenant_projection_status.rs`
- `crates/infrastructure/tests/tenant_projection_status_read.rs`
- `crates/proto/proto/erpx/platform/v1/platform.proto`
- `apps/server/src/main.rs`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Verification

From `erpx/`:

```text
cargo fmt --all — PASS
cargo test -p erpx-migration — PASS: 6 tests
cargo test -p erpx-infrastructure --test tenant_projection_status --test tenant_projection_status_read — PASS: 7 tests
cargo test -p erpx-server --bin erpx-server — PASS: 62 tests
cargo test -p erpx-worker — PASS: 47 passed, 1 ignored
```

The full integration-wave verification remains mandatory before the next
packet: formatting, workspace clippy/tests, both Dioxus console target checks,
Folio lint/typecheck, runtime preservation, and disposable-database cleanup.

## Open requirements and rollback

This packet measures read-model publication freshness; it does not establish
durable incremental source checkpoints, source-event lag producers, physical
replacement-read switching, backup/restore execution, or Platform Operations
rebuild/action controls. Rollback removes migration `0005`, the additive DTO
fields, and freshness mapping after all rows written with `published_at` have
been migrated or the read projection has been rebuilt from an earlier schema.
