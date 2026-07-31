# G8 Finance — Thai tax source/read-model reconciliation

## Bounded gap closed

The authenticated Thai VAT/WHT report already had an authoritative Finance
fact write boundary, append-only outbox event, rebuildable tenant-read
projection, MFA/RBAC scope checks, API response validation, and a read-only
Dioxus surface. It did not verify that the source event feed and read model
were exactly aligned before serving a statutory report.

This packet adds that missing fail-closed boundary without changing the shared
protobuf or migration ownership:

- `SqlxFinanceThaiTaxStore::reconcile_projection` compares the complete,
  bounded holding source sequence with the complete read sequence using fact
  identity, source event identity, aggregate version, and event type.
- The comparison reads source data only from the tenant-write database and
  projection data only from the tenant-read database. It accepts at most the
  existing one-million-fact bound and reports missing, extra, reordered, or
  replaced rows as drift.
- The Thai tax worker requires reconciliation after the final incremental page
  and after a rebuild. Drift is retryable and never acknowledged as a healthy
  completed projection.
- `GetThaiTaxReport` routes both physical database sides and refuses to return
  a report when the source/read sequences are not reconciled. Existing MFA,
  `finance.read`, customer-scope, holding/company, and audit boundaries remain
  authoritative.
- The Finance UI states that source/read reconciliation is required and shows
  no stale, drifted, or client-derived totals when the server rejects the
  report.

## Files

- `crates/infrastructure/src/finance_tax_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `apps/worker/src/finance_tax_projection.rs`
- `apps/server/src/main.rs`
- `apps/console/src/main.rs`
- `crates/infrastructure/tests/finance_thai_tax_projection.rs`

## Verification

The ignored SQLx acceptance creates and migrates disposable tenant write/read
databases, projects one authoritative tax event, verifies exact reconciliation,
verifies the report, deletes only the disposable projected row, and verifies
that reconciliation reports drift. Worker unit coverage verifies that a
completed projection rejects a count/version/event mismatch.

Run from `erpx/`:

```text
cargo test -p erpx-worker --lib finance_tax_projection --offline
cargo test -p erpx-infrastructure --test finance_thai_tax_projection --offline -- --ignored --test-threads=1
```

No production directory activation, provider activation, Folio write
retirement, command-owner switch, or production cutover is part of this
packet.

## Executed evidence — 2026-07-28

- PASS: `cargo fmt --all --check`.
- PASS: `cargo test -p erpx-worker --lib finance_tax_projection --offline` —
  2/2, including completed-page drift rejection.
- PASS: disposable PostgreSQL SQLx acceptance — 1/1. It applied tenant-write
  and tenant-read migrations, projected one source event, verified exact
  reconciliation and report totals, removed the disposable read row, and
  verified non-reconciled drift.
- PASS: `cargo test -p erpx-console --offline -- --test-threads=1` — 108/108.
- QUEUED: server transport test rerun is currently blocked by a concurrent
  unrelated Operations compile error in `operations_persistence.rs`; no
  Finance or shared file was changed to bypass that error.
