# G8 Finance consolidated trial balance — 2026-07-28

## Delivered

The Finance read path now exposes a holding-level consolidated trial balance
over explicitly supplied company chart snapshots. The domain aggregator:

- validates one holding and currency across every chart;
- verifies every posted journal belongs to one supplied company chart;
- aggregates immutable posted-journal lines by account code;
- rejects inconsistent account metadata, missing charts, duplicate companies,
  overflow, and unbalanced totals; and
- never infers intercompany elimination. Elimination must be an explicit posted
  adjustment journal and remains visible in the authoritative source.

The SQLx adapter walks the bounded tenant-read journal projection, carries the
maximum source version, and delegates all arithmetic to the domain. The server
requires an explicit holding-contained Finance read scope, entitlement/RBAC,
tenant scope membership, and an auditable correlation context. Before reading
the report it compares each supplied chart against the authoritative Finance
chart read projection. The Dioxus Finance surface displays typed server lines
and totals only; it performs no client aggregation.

Existing Finance journal persistence, outbox, projection, worker scheduling,
and rebuild path remain the source of truth, so this read feature does not
introduce a parallel source or schema. Intercompany elimination, FX
translation, ownership percentages, statutory consolidation packs, and live
two-database execution remain explicit follow-up requirements.

## Evidence

- `cargo test -p erpx-domain finance::tests::consolidated_trial_balance
  --offline`: 2 passed.
- `cargo check -p erpx-proto -p erpx-server --offline`: passed.
- `cargo check -p erpx-console --no-default-features --features desktop
  --offline`: passed before the final formatter pass; re-run in the current
  verification wave.
- Focused SQLx acceptance compiles and is correctly ignored without disposable
  tenant URLs. Explicit execution fails closed at the missing
  `ERPX_TEST_TENANT_WRITE_DATABASE_URL` resource; no database work ran.

## Rollback

Rollback is additive: remove the consolidated domain types/function, the
SQLx read method, the protobuf RPC, server mapper, console read panel, focused
acceptance test, and this audit. Existing Finance journal rows, outbox events,
read projections, and migration history are unchanged.

G8 remains open for broader AP/AR close, banking reconciliation, tax filing
export, FX/consolidation eliminations, production approval, and disposable
database certification. No activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator refresh — 2026-07-28

The real ignored acceptance
`live_consolidated_trial_balance_replays_and_aggregates_two_companies` passed
against a fresh tenant write/read PostgreSQL pair. The arrangement persists
both authoritative company chart snapshots, posts immutable journals for two
companies, projects the source events, and reads a balanced holding-level
report with total debit/credit 150 and source-version evidence. No database
or production state remains from the refresh.
