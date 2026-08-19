# G8 Finance authenticated journal, subledger, and close-review read surface

## Scope

This packet connects the existing Finance `ListPostedJournals`,
`ListFinanceSubledgerReadRows`, `ListFinanceBankingReadRows`, and
`GetFinanceCloseReview` RPCs to the Dioxus `/finance` product route. The route
requires an explicit opaque holding reference, holding route reference, and
UUID holding scope; close review additionally requires company and period
UUIDs. It never infers a holding from platform data and never uses a
platform-scope query as a Finance read.

## Authoritative path

- The console sends an authenticated, audited holding-scoped `QueryContext` to
  `FinanceService::ListPostedJournals`.
- The server performs `finance.read` authorization, holding write/read routing,
  bounded pagination, and reads immutable posted-journal persistence. Existing
  Finance SQLx event/outbox/projector/reconciliation behavior remains the
  source of truth.
- The client retains only journal UUID, company/period references, source
  version, and posted timestamp. Raw `payload_json` is discarded before the
  display model is created.
- Close review retains only allow-listed status/version, balance/control
  totals, bounded account totals, freshness, fact counts, and correlation
  metadata. Scope mismatches, non-read-only metadata, unknown statuses, and
  inconsistent control/fact counts fail closed.
- The AP/AR subledger adapter retains only aggregate type/UUID, company/branch,
  source version, pagination metadata, and correlation. Optional aggregate
  type/company filters are validated before the authenticated holding-scoped
  request; raw aggregate JSON is discarded.
- The banking adapter retains only aggregate type/UUID, holding/company,
  source version, pagination metadata, and correlation. Holding identity and
  optional company/type filters are checked before display; raw banking JSON
  is discarded.

## Fail-closed boundary

`FinanceReadModel::from_wire` rejects oversized pages, missing correlation
IDs, inconsistent cursors, non-UUID journal IDs, unsafe/unbounded scalars, and
malformed timestamps/references. The subledger adapter rejects oversized
pages/payloads, missing or contradictory cursors, invalid aggregate/company/
branch identities, filter escapes, zero source versions, and missing
correlation. The close adapter rejects scope mismatches, missing projections,
non-read-only metadata, unknown statuses, oversized account reports, and
inconsistent fact/control counts. The UI renders explicit scope,
loading/empty/denied/offline/error states, bounded 25-row
journal/subledger/banking pages, close statement/control/account metadata, and
read-only boundary notes. Posting, period close, AP/AR, banking, and other
Finance commands remain server-owned and are not fabricated by this route.

## Evidence

- `cargo check -p erpx-console --offline`: PASS.
- `cargo test -p erpx-console --offline`: 71/71 PASS, including Finance
  journal/subledger/banking scope and cursor validation, close-review
  scope/read-only validation, payload redaction, and safe error categories.
- Existing Finance SQLx persistence and server acceptance suites remain the
  authoritative domain/event/worker/read evidence.

No production activation, command-owner switch, Folio write retirement, or
cutover occurred. External vendor certification, measured directory import/
revocation evidence, and live production migration evidence remain queued.
