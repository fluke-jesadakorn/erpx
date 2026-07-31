# G9-SCM-005 — projection checkpoint integrity

## Delivered behavior

The SCM read boundary now treats the aggregate row and its projection
checkpoint as one integrity pair:

- reconciliation reads and validates projected aggregate payloads, detects
  projected versions ahead of source, and reports missing, stale, or orphan
  checkpoints instead of returning a false green result;
- `SqlxScmStore::get` requires a matching checkpoint and validates the decoded
  aggregate before returning a read row;
- projection replay rejects an unpaired aggregate/checkpoint, a same-version
  payload conflict, and a same-version event-ID conflict; exact retries remain
  no-ops;
- clear/replay remains the only repair path and preserves the authoritative
  write/outbox source.

## Files changed

- `crates/domain/src/procurement_inventory.rs`
- `crates/infrastructure/src/scm_persistence.rs`
- `crates/infrastructure/tests/scm_persistence.rs`
- `audits/G9-SCM-005-READ-SAFETY/README.md`

No protobuf, server, worker registration, migration, manifest, console shell,
or shared ledger file was changed.

## Focused verification

Run from `erpx/`:

- `cargo test --offline -p erpx-domain --lib procurement_inventory --target-dir /tmp/folio-erpx-scm-target` — PASS, 5 passed.
- `cargo test --offline -p erpx-infrastructure --lib scm_persistence --target-dir /tmp/folio-erpx-scm-target` — PASS, 5 passed.
- `cargo clippy --offline -p erpx-infrastructure --lib --target-dir /tmp/folio-erpx-scm-target -- -D warnings` — PASS.
- `cargo test --offline -p erpx-infrastructure --test scm_persistence --no-run --target-dir /tmp/folio-erpx-scm-target` — PASS; the ignored acceptance binary compiles.
- `cargo test --offline -p erpx-infrastructure --test scm_persistence --target-dir /tmp/folio-erpx-scm-target -- --ignored --nocapture --test-threads=1` — **BLOCKED** at the required `ERPX_TEST_TENANT_WRITE_DATABASE_URL` lookup (`NotPresent`).

The ignored acceptance test now exercises same-version event conflict,
checkpoint deletion detection, fail-closed reads, projector refusal to
silently repair a damaged read model, and clear/replay recovery. It requires
`ERPX_TEST_TENANT_WRITE_DATABASE_URL` and
`ERPX_TEST_TENANT_READ_DATABASE_URL`; those external routed database URLs were
not configured in this run, so no live SQL acceptance is claimed.

## Rollback boundary

The change is read-model-only. Removing the three implementation/test paths
leaves authoritative SCM aggregates, snapshots, idempotency receipts, and
append-only outbox events untouched. A damaged disposable projection is
recovered by `clear_projection` followed by source-outbox replay; source rows
are never deleted or rewritten by this packet.
