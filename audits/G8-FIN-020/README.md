# G8-FIN-020 — Finance close-control evidence source adapter

## Scope

This packet adds a separate SQLx write adapter for authoritative Finance source
contexts to publish and update rows in the migration-0035
`erpx_tenant.finance_close_control_evidence` table. Main integration adds
migration 0037 for the positive source version; the Finance close-review
adapter and domain contracts remain unchanged.

## Delivered behavior

- Validates bounded control, source-type, evidence-reference, and JSON payload
  sizes before opening a transaction.
- Rejects nil holding, company, period, or source-aggregate identifiers.
- Optionally verifies source identity metadata included in the payload.
- Starts every operation with `begin_scoped`, then locks the control key with a
  transaction-local advisory lock.
- Reloads the authoritative Finance period and rejects missing or mismatched
  holding/company identity.
- Rejects period status `closed` and an already closed close-review row before
  writing. The migration trigger remains the final concurrent-close defense.
- Preserves a control's source type and source aggregate identity across
  updates.
- Treats an exact existing row and source version as a deterministic
  idempotent replay; stale versions and same-version changed payloads fail
  closed, while strictly newer evidence updates the current row until close.
- Writes only the migration-0035 evidence table. Source-context outbox
  publication remains a separate follow-up packet.

## Files

- `crates/infrastructure/src/finance_close_control_evidence.rs`
- `crates/infrastructure/tests/finance_close_control_evidence.rs`

## Verification

The source file contains unit coverage for reference, payload, and source
identity validation. The disposable integration test is `#[ignore]`; run:

```text
cargo fmt --all --check
cargo test -p erpx-infrastructure --test finance_close_control_evidence
ERPX_TEST_TENANT_WRITE_DATABASE_URL=... cargo test -p erpx-infrastructure --test finance_close_control_evidence -- --ignored --nocapture
```

The test includes the adapter source by path and uses a local test-only copy of
the scoped transaction helper. The main agent independently exported the
adapter's three public types and reran the focused checks plus the disposable
PostgreSQL integration test.

## Known boundary and integration order

Migration `0037` adds a positive `source_version` column for ordered source
publication. The adapter serializes each control key under an advisory lock,
rejects stale versions, and rejects changed payloads at an already-used
version; this prevents delayed source events from overwriting newer evidence.
The live test also covers the same-version conflict path.

Integration order: source-context outbox publication remains future
integration work. This packet is not evidence that G8 Finance or the ERPX
program is complete.
