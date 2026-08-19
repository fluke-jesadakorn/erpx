# G8-FIN-015 — Finance banking read-list and Platform Operations detail transport

## Delivered behavior

The replaceable Finance banking read model now has a bounded keyset page API:

- `FinanceBankingReadCursor` validates an allow-listed aggregate type and
  non-nil aggregate ID.
- `SqlxFinanceBankingStore::list_read_rows` reads only the routed holding-read
  database, orders by `(aggregate_type, aggregate_id)`, uses an exclusive
  continuation cursor, and limits each page to 100 rows.
- Holding, company, and branch scope predicates are applied in SQL before
  pagination. Company scope includes both company- and branch-scoped records;
  branch scope is exact. Platform and composite subtree scopes fail closed.
- Persisted row identity, source version, aggregate payload, and holding/
  company columns are cross-validated before a row leaves the repository.
- `GetFinanceBankingReadRow` now uses the read-only route and the same typed
  scope boundary. `ListFinanceBankingReadRows` adds optional aggregate/company
  filters, bounded page size, opaque cursor transport, customer-scope checks,
  Finance role authorization, and customer-visible audit events.
- Platform Operations now implements the already-declared `GetHolding` RPC with
  safe registry/readiness metadata and platform audit attribution. Database
  keys, storage namespaces, credential references, and backup references do
  not cross the transport.

## Files changed

- `crates/infrastructure/src/finance_banking_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/finance_banking_persistence.rs`
- `crates/proto/proto/erpx/platform/v1/platform.proto`
- `apps/server/src/main.rs`
- `erpx/audits/G8-FIN-015/README.md`

No new migration or database effect was introduced. The existing G8-FIN-012
read migration supplies the read table and forced-RLS policy. Folio, runtime
environment, worker scheduler, Stripe, and provider SDK files were untouched.

## Verification

Focused checks from `erpx/`:

```text
cargo check -p erpx-infrastructure -p erpx-server
cargo test -p erpx-infrastructure --lib
cargo test -p erpx-infrastructure --test finance_banking_persistence
cargo test -p erpx-server --bin erpx-server
cargo test -p erpx-infrastructure --test finance_banking_provider
```

Results:

- Infrastructure check: PASS.
- Infrastructure unit tests: 94 passed, 4 documented PostgreSQL tests ignored.
- Banking persistence tests: 2 active passed, 2 disposable-database tests ignored.
- Server tests: 60 passed.
- Provider adapter tests: 6 passed.
- Direct rustfmt checks passed for all changed Rust files.

The integration-wide commands were also run. Dioxus web and desktop checks,
Folio lint, and TypeScript checking passed; Folio lint retained only the four
pre-existing `start-n8n.js` warnings. The adjacent scheduler issue observed in
the earlier wave was repaired by G6-PRV-011; the fresh post-repair wave passed
workspace formatting, strict clippy, and active workspace tests.

Independent live evidence ran
`live_bank_read_projection_is_idempotent_and_rebuildable` against explicitly
named disposable write/read PostgreSQL databases. The projection rebuilt,
the bounded list returned a 2-row first page and exclusive 1-row continuation,
and the mismatched company scope returned no rows. Both databases were
dropped; a catalog query confirmed no matching databases remained.

## Security review

- Read-list and single-row reads use the read pool; the write database is not
  required for read execution.
- The holding setting is installed by the scoped read transaction, preserving
  forced RLS.
- SQL scope predicates run before `LIMIT` and cursor calculation, preventing
  unauthorized rows from affecting page shape or continuation state.
- The server repeats aggregate scope containment after repository decoding and
  records actor, customer, purpose, action, and correlation for every read.
- Cursors contain only an allow-listed aggregate type and UUID, encoded as an
  opaque bounded transport value.
- Platform holding detail returns only readiness state strings and registry
  metadata; credentials and physical keys remain server-side.

## Unfinished work

Provider-specific network adapters, provider credential vault integration,
bank-to-GL matching/posting, Finance budgets and approval/SoD enforcement,
consolidation, Thai tax, statements/drill-down, migration reconciliation, and
full Platform Operations actions remain open. The provider-neutral G8-FIN-013
sidecar remains a typed, non-network boundary and is not production provider
connectivity.

## Integration order

1. Keep all Finance reads on the typed read projection and route through the
   authoritative role/customer-scope checks.
2. Add client/UI adapters for the new list/detail RPCs with explicit loading,
   stale, denied, and read-only states.
3. Integrate a provider implementation by translating only into the existing
   G8-FIN-013 DTOs, then map complete facts to G8-FIN-011 write commands.
4. Add bank-to-GL matching and reconciliation workflows only after the
   provider/import command path has independent entitlement and SoD evidence.

## Rollback

Remove only the additive list cursor/page repository API, protobuf messages and
RPC, server handlers, tests, and this audit. Retain the existing read migration
and previously integrated banking write/projection data; no database rollback
is required.
