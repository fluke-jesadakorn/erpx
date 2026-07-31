# G10-MKT-078 — Marketing delivery read-model integrity fence

Date: 2026-07-28

## Delivered behavior

The Marketing delivery projector now reconciles the disposable read model
against the authoritative tenant-write outbox before the worker claims
provider work after a bounded page is drained.

- Reconciliation validates source envelope identity, aggregate identity and
  version, latest source payload, read-row payload, and read-row version.
- Reconciliation reports stale, ahead, payload-mismatched, orphaned, and
  missing deliveries as unhealthy.
- The durable cursor is checked for the expected final source event, timestamp,
  and event count for the holding. Cursor updates cannot overwrite another
  holding's checkpoint, and reset deletes only the selected holding's cursor.
- The worker fails closed before provider claims when the caught-up read model
  or cursor is unhealthy.
- Clearing and replaying the disposable read model leaves the tenant-write
  delivery aggregate and outbox unchanged.

## Exact files

- `crates/infrastructure/src/marketing_delivery_persistence.rs`
- `crates/infrastructure/tests/marketing_delivery_persistence.rs`
- `apps/worker/src/main.rs`
- `audits/G10-MKT-078/README.md`

## Verification

- Focused Rust formatting for the three changed Rust files — PASS.
- `cargo check -p erpx-infrastructure --test marketing_delivery_persistence` —
  blocked by the pre-existing unrelated compile error in
  `crates/domain/src/quality.rs:373` (`Option<Uuid>::is_some_and(Uuid::is_nil)`
  has the wrong function signature under Rust 1.91).
- `cargo test -p erpx-infrastructure --test marketing_delivery_persistence
  -- --list` — blocked by the same unrelated compile error; no test result is
  claimed.
- The focused disposable SQLx test covers exact source/read payload
  reconciliation, checkpoint drift detection, holding-scoped clear, source
  outbox preservation, and replay rebuild. It requires the repository's two
  disposable tenant PostgreSQL URLs and was not represented as an external
  provider certification claim.

## Rollback

Revert the three implementation/test files and this audit together. If the
worker change is reverted after deployment, stop using the new reconciliation
fence and retain the existing tenant-write delivery aggregate/outbox and
disposable read rows. No migration or authoritative write data changed.
