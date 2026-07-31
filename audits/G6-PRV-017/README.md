# G6-PRV-017 — durable incremental projection-status checkpoints

## Delivered behavior

The control-read scheduler-status projection now has a durable source cursor
ordered by `(updated_at, job_id)`. The publisher reads bounded source pages,
upserts the payload-free status DTOs, and advances the cursor in the same
control-read transaction. A crash before commit replays the page safely; a
committed page cannot move the cursor backwards. Rebuilds clear both the read
rows and the checkpoint before copying the authoritative control-write source.

The cursor rejects a nil job identity, page sizes are bounded, and the
projection writer continues to validate status counters, references, versions,
and kind/state combinations before persistence. No provider credentials,
tenant connection details, claim payloads, or worker secrets cross the read
projection boundary.

## Files changed

- `crates/migration/migrations_read/0006_tenant_projection_status_checkpoint.sql`
- `crates/migration/src/lib.rs`
- `crates/infrastructure/src/tenant_projection_scheduler.rs`
- `crates/infrastructure/src/tenant_projection_status_projection.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/tenant_projection_status.rs`
- `crates/infrastructure/tests/tenant_projection_status_incremental.rs`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

Migration `0005_tenant_projection_freshness.sql` is the preceding freshness
packet and is not reimplemented here.

## Verification

From `erpx/`:

```text
cargo fmt --all --check — PASS
cargo test -p erpx-migration --lib — PASS
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings — PASS
cargo clippy -p erpx-worker --all-targets -- -D warnings — PASS
cargo test -p erpx-infrastructure --lib --test tenant_projection_status --test tenant_projection_status_read — PASS
cargo test -p erpx-worker — PASS
```

Direct disposable-database evidence:

```text
cargo test -p erpx-infrastructure --test tenant_projection_status_incremental -- --ignored --exact incremental_status_publication_advances_a_durable_checkpoint_idempotently — PASS: 1 test
```

The disposable write/read databases were explicitly removed after the test.

## Open requirements and rollback

This packet does not provide source-event lag for every projection, physical
candidate database rebuild execution, replacement-read activation transport,
backup/PITR execution, or Platform Operations actions. Rollback removes only
the additive checkpoint migration, cursor/publisher path, focused tests, and
audit entry after checkpoint state is no longer required or the read model has
been rebuilt from the authoritative source.
