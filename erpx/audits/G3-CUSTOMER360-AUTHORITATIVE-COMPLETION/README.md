# G3 Customer 360 authoritative write/read completion slice

Date: 2026-07-28

## Scope

This packet records the current-tree G3 slice only. It does not claim external
directory, device, migration, production, or vendor certification and does not
authorize a production cutover.

## Delivered behavior

- `CustomerNote` is a validated append-only domain aggregate with bounded body,
  immutable ID, explicit customer scope, and safe read DTO conversion.
- `AddCustomerNoteCommand` and `Customer360NoteService` provide an authenticated
  application write port. The SQLx adapter checks the authoritative
  `customer_scope_bindings` write database before invoking the existing generic
  idempotent command unit of work.
- The existing aggregate snapshot, outbox, audit-event, and idempotency
  transaction persists `customer_note` / `customer.note_added` atomically.
  Replays return the original receipt; a same-ID/different-payload replay is
  rejected.
- `Customer360Projection` handles both customer aggregate events and note
  events. Customer payloads use the source event timestamp as a read-side
  update fence; note rows are inserted immutably with a redacted body and are
  rebuilt from the write-side outbox.
- `SqlxControlPlane::reconcile_customer_360_read_models` compares authoritative
  customer and note snapshots, read payloads, derived multi-holding/company/
  branch bindings, and access-audit summaries. It reports missing, stale,
  orphaned, invalid, and unprojected data without repairing or promoting a
  read model to authority.
- The existing authenticated detail read remains read-only and checks the
  write-side customer binding before reading the control read pool. Note pages
  retain bounded deterministic cursors and domain redaction.

## Focused evidence

`apps/worker/tests/customer_360.rs` now exercises, against disposable distinct
control write/read PostgreSQL databases:

1. customer outbox rebuild and derived scope bindings;
2. revocation snapshot/rebuild and binding removal;
3. append-only note aggregate snapshot, outbox, audit/idempotent replay;
4. note projection and body redaction;
5. reconciliation after rebuild; and
6. fail-closed rejection of a note using an unbound holding.

Expected focused command:

```text
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=<disposable-write>
ERPX_TEST_CONTROL_READ_DATABASE_URL=<disposable-read>
cargo test -p erpx-worker --test customer_360 \
  --offline -- --ignored --test-threads=1
```

The complete workspace ignored suite and external LDAP/AD DS/UCS acceptance
remain coordinator-owned. Missing credentialed vendor environments remain
queued rather than fabricated as certification evidence.

## Rollback boundary

This is additive application/domain/infrastructure/worker behavior over the
existing generic control-plane snapshot/outbox/read tables. Rollback is a
code-level disable/revert of the note command/projector and removal of the
additive audit packet; authoritative historical snapshots/outbox/audit rows are
not deleted. No migration numbering, protobuf, manifest, server/worker
registration, shared authorization, console shell, or Folio write ownership was
changed by this slice.
