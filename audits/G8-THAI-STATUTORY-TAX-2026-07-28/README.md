# G8 Finance — Thai statutory tax fact/report packet

Date: 2026-07-28

This packet adds one authoritative G8 slice: explicit Thai output VAT, input
VAT, and withholding-payable facts linked to immutable posted journal lines.
Tax is never inferred from a journal description or accepted as a client-side
report total.

## Implemented boundary

- `crates/domain/src/finance_tax.rs` validates tax-period, currency, amount,
  rate, company scope, and immutable journal-line linkage semantics and
  deterministically aggregates a report.
- Tenant-write migration `0052_finance_thai_tax.sql` stores append-only facts,
  idempotency, row-level security, and the authoritative outbox path.
- `SqlxFinanceThaiTaxStore` persists the fact atomically with its snapshot and
  outbox event, verifies the referenced posted journal/line, projects only
  bounded report rows, and supports idempotent event replay.
- Tenant-read migration `0041_finance_thai_tax_projection.sql` stores only the
  rebuildable typed report rows and projection checkpoints.
- `finance_thai_tax_projection` is registered as a durable multi-tenant worker
  projection with bounded incremental/rebuild jobs.
- `RecordThaiTaxFact` is an authenticated, entitled, holding/company-bound
  command. `GetThaiTaxReport` is an authenticated `finance.read` query that
  requires a fresh MFA step-up and emits only typed report facts/totals.
- The Dioxus Finance surface exposes period/company/currency/MFA inputs and
  renders only server-returned tax facts and totals.
- `crates/infrastructure/tests/finance_thai_tax_projection.rs` is a real
  PostgreSQL acceptance test covering source event projection, idempotent
  replay, and report scope. It is intentionally ignored pending disposable
  tenant write/read database URLs.

## Verification

PASS: domain tax aggregation tests (2/2).

PASS: worker tax projection bounds test and proto package tests.

PASS: targeted warnings-denied Clippy for domain, infrastructure, worker,
server, and console; `cargo fmt --all --check`; desktop Dioxus console check.

QUEUED: the new live PostgreSQL acceptance failed closed because
`ERPX_TEST_TENANT_WRITE_DATABASE_URL` is not configured. This is a resource
blocker, not an application assertion failure. The full ignored sweep has the
same disposable SQLx resource gate.

## Scope remaining

This packet does not claim G8 complete. Thai Revenue Department filing/export
certification, provider-backed bank-to-GL, consolidation, broader AP/AR close
acceptance, and production migration/cutover remain open. No production
activation, command-owner change, Folio write retirement, or cutover occurred.
