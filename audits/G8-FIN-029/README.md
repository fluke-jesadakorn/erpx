# G8-FIN-029 — bounded Finance close-review/report adapter

## Packet contract

- **Objective:** provide a read-only infrastructure adapter for one Finance
  close review and its bounded report/control summary.
- **Dependencies:** G8-FIN-018 close/reporting domain rules, G8-FIN-019
  close persistence, and G8-FIN-028 authenticated close-review command
  transport.
- **Allowed paths:** `crates/infrastructure/src/finance_close_persistence.rs`,
  the new `crates/infrastructure/src/finance_close_read.rs`, the infrastructure
  export, this audit, and append-only plan/ledger records.
- **Forbidden paths respected:** no protobuf, server, worker, UI, domain,
  Folio, scheduler, provider, command-owner, or schema migration changes.

## Delivered behavior

- An internal validated close snapshot carries aggregate state and
  version/idempotency/update metadata for adapter composition; it is not
  exported as a public raw-domain escape hatch and the persisted JSONB is
  never returned.
- `SqlxFinanceCloseReadAdapter` composes an already-routed
  `SqlxFinanceCloseStore` with the existing tenant Finance read pool and
  performs no data writes.
- DTOs preserve holding/company/period identity, explicit `open`/`review`/
  `closed` status, version, idempotency key, and bounded actor metadata.
- Statement totals, deterministic account rows, and resolved/unresolved
  control counts are derived from the existing `FinanceCloseReview` domain
  reporting APIs. Journal facts, control payloads, and command contexts are
  excluded.
- Fact, control, account, actor, and identifier paths are bounded. A missing
  close row returns `Missing` metadata. A missing read-projection fact is
  marked stale, an old projection is stale, and a future timestamp is
  explicitly unknown.
- Errors are mapped to fixed `InvalidRequest`, `InvalidProjection`, and
  `PersistenceUnavailable` categories without SQL or payload disclosure.

## Database and contract effects

- No schema migration or database write was added.
- The adapter reads the authoritative close row through the existing close
  store until a later packet supplies a dedicated close-review read
  projection. Report freshness uses the existing
  `finance_posted_journals` tenant read projection and forced-RLS scope.
- The infrastructure public export adds the typed adapter, DTOs, metadata,
  status/error enums, and report bounds.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure finance_close_read -- --nocapture` — PASS;
  5 focused tests.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure` — PASS; active tests passed and
  database-dependent suites remained explicitly ignored without disposable
  URLs.
- Full workspace wave is recorded in `erpx/EXECUTION_LEDGER.md` after this
  entry.

## Security and unfinished work

- No tenant credentials, raw JSONB, payment data, command contexts, or
  secrets enter the DTO boundary. Storage references remain internal to the
  already-routed adapter.
- Authenticated scope/entitlement enforcement, global-read audit records,
  server read transport, a dedicated close-review read projection, close
  workers/reconciliation, Finance UI, and the broader G8 gate remain open.
- The adapter does not claim that a close review is fresh solely because the
  write row exists; missing or stale Finance source projection metadata is
  surfaced to callers.

## Integration order and rollback

1. Compose the adapter into an authenticated server query after applying the
   existing tenant/scope/entitlement and global-read audit boundaries.
2. Add a later migration and worker projection for close-review rows, then
   replace the temporary authoritative-row read without changing DTOs.
3. Add database-backed transport and end-to-end evidence before any Finance
   completion claim.

Rollback is additive: remove `finance_close_read.rs`, its public export, the
`CloseReviewSnapshot` accessor, focused tests, and this audit record. Existing
close persistence and G8-FIN-028 write transport remain unchanged.
