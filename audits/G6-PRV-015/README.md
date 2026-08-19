# G6-PRV-015 — worker scheduler-status publisher

## Delivered behavior

Added an opt-in worker handler that continuously publishes the typed,
metadata-only scheduler status projection from the authoritative control-write
pool to the physically separate control-read pool. Publication uses the
existing version-ordered idempotent upsert, is bounded by the infrastructure
page size, and does not claim scheduler jobs or open holding database routes.

The worker composition exposes the handler by an explicit registry method and
the worker binary selects it only when
`ERPX_WORKER_HANDLER=holding_projection_status_projection`. The default registry
does not install it. A separate full rebuild/clear operation remains available
on the infrastructure projection store for reviewed recovery workflows.

## Files changed

- `apps/worker/src/holding_projection_status_projection.rs`
- `apps/worker/src/lib.rs`
- `apps/worker/src/main.rs`
- `crates/infrastructure/src/holding_projection_status_projection.rs`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`
- `audits/G6-PRV-015/README.md`

No protobuf, gRPC, holding migration, Folio application, credential, or runtime
environment file changed. No production provider or external network call was
added.

## Interfaces

- `HoldingProjectionStatusPublisher` — injected async publication boundary.
- `HoldingProjectionStatusProjectionHandler<P>` — named worker handler with
  redacted failures and stable report counts.
- `WorkerRuntime::handler_registry_with_control_plane_holding_projection_status_projection`
  — explicit physical control-pool composition.
- `HOLDING_PROJECTION_STATUS_PROJECTION_HANDLER_NAME` — selectable handler
  name.
- `SqlxHoldingProjectionStatusProjectionStore::publish_read_model` — steady
  state upsert pass; `rebuild_read_model` remains the explicit clear-and-copy
  recovery seam.

## Security and data boundaries

Only validated scheduler status metadata is copied. Job payloads, claim IDs,
worker IDs, credentials, database URLs, tokens, and storage references are not
part of the DTO or SQL projection. The read adapter receives only the
control-read pool; the writer receives the control-write source and separate
read pool through the existing control-plane composition. Persistence failures
are logged as a safe category and returned without provider/database details.
The handler is not silently enabled, and it never claims or executes holding
work.

## Verification

From `erpx/`:

```text
cargo fmt --all --check — PASS
cargo check -p erpx-worker --bin erpx-worker — PASS
cargo clippy -p erpx-worker --all-targets -- -D warnings — PASS
cargo test -p erpx-worker — PASS: 47 passed, 1 ignored
cargo test -p erpx-infrastructure --test holding_projection_status_read --test holding_projection_status --lib — PASS: 99 library tests (95 passed, 4 ignored), 5 status tests, 2 read tests
```

## Unfinished work

This packet does not establish a default deployment cadence, live freshness
producer, replacement-read database switching, scheduler-to-status event
notifications, Platform Operations rebuild/action RPCs, or complete backup /
restore workflows. It does not claim those requirements are complete.

## Migration effects and rollback

No database migration was added and no data was deleted. Rollback removes the
worker publisher module/export, worker composition branch, and the additive
steady-state `publish_read_model` method. Retain the existing rebuildable
status table and G6-PRV-014 writer until a reviewed retention decision.

