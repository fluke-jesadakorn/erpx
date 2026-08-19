# G7-004-EXCEPTION-PERSISTENCE-038 evidence

## Delivered behavior

- Added a typed `SqlxOperationsStore::save_exception_case` entry point that
  uses the existing validated aggregate/source-reference transaction path.
- Added `read_active_exception_page`, scoped to the routed holding and the
  rebuildable holding read database.
- Active reads allow only `detected`, `assigned`, `acknowledged`, and
  `escalated` states, use deterministic newest-update ordering with aggregate
  ID tie-breaking, return `has_more`/resume metadata, and cap one page at 100.
- Read-row JSON is revalidated against aggregate identity, version, and
  holding before returning a typed `ExceptionCase`.
- Added a focused page-boundary test. Existing aggregate event persistence
  continues to enforce lifecycle validation, optimistic versions, source
  references, idempotency, and replay behavior.

## Files changed

- `crates/infrastructure/src/operations_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `audits/G7-004-EXCEPTION-PERSISTENCE-038/evidence/README.md`

No migration, worker, server, protobuf, UI, Folio, or source transaction files
were changed.

## Exact verification

```text
cargo fmt -p erpx-infrastructure -p erpx-server -- --check  PASS
cargo test -p erpx-infrastructure --lib                       PASS: 43 passed, 3 ignored
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings  PASS
```

The existing disposable Operations write/read integration suite remains the
independent evidence for holding RLS, event idempotency, projection replay,
ordered event pages, and replacement-read preparation.

## Unfinished work

Detection producers, assignment, acknowledgement, escalation, SLA timers,
expired-user reassignment, manager escalation, worker scheduling, authenticated
routes, full Operations UI, and Folio/ERPX dashboard reconciliation remain
open. This packet does not close G7.

## Security and rollback

All reads use the routed holding transaction and read-side RLS context; the
typed metadata check fails closed on cross-holding or corrupt state. Rollback
is limited to the new typed methods, export, focused test, and this evidence
file; generic Operations persistence and existing migrations are unaffected.
