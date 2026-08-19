# G7 Operations — Durable Source Detection Cursor

## Bounded gap

The Operations detection scheduler previously rebuilt its request list from the
beginning of the source-owned holding outbox on every completion pass. That was
safe for replay but did not provide durable progress for a growing source feed:
restart cost grew with the full history, and a bounded pass could not express
which page had already been durably handled.

This slice adds a holding-scoped source-feed high-water mark using the existing
forced-RLS `erpx_holding.operations_projection_checkpoints` table. It is stored
under `operations_detection_cursor` and does not change migrations, protobuf,
shared authorization, registration, or production command ownership.

## Authoritative flow

1. The registered Operations completion worker reads the source-owned outbox
   after the persisted cursor in `(occurred_at, event_id)` order.
2. Each source event is scope-checked and matched only to coordinator-supplied
   Operations detection rules and authorization/MFA proof.
3. Every matched request is persisted through the existing authoritative
   `detect_exception_at` transaction, including immutable source provenance,
   Operations events, idempotency, and outbox emission.
4. Only after all requests in that page complete does the worker save the page
   cursor. A crash between those transactions replays idempotent detection;
   it cannot skip a source event. Concurrent workers cannot move the cursor
   backwards.
5. The existing Operations projector drains the resulting immutable event
   stream into `operations_read_model`; the existing authorized server summary
   and exception-list RPCs and the Dioxus Operations screen therefore expose
   the detected state without a new transport contract.
6. Existing source reconciliation and full Operations reconciliation remain in
   the same completion pass and fail the pass when authoritative links,
   outbox parity, checkpoints, or read freshness are unhealthy.

## Rollback

`prepare_projection_rebuild("operations_read_model")` clears the disposable
read rows, normal projection checkpoints, and detection cursor in one routed
read-side transaction. The existing Operations projection runner then replays
immutable Operations events; write-side events and Folio remain untouched.

## Changed files

- `crates/infrastructure/src/operations_persistence.rs` — durable cursor
  identity, monotonic persistence/read/clear methods, and rollback clearing.
- `crates/infrastructure/src/lib.rs` — exports the G7 cursor projection name.
- `apps/worker/src/operations_completion.rs` — cursor-backed page application
  and completion-pass integration.
- `apps/worker/tests/operations_completion_authoritative.rs` — live scheduler
  proof of cursor persistence and replay behavior.
- `crates/infrastructure/tests/operations_persistence.rs` — live SQLx proof of
  round-trip, monotonicity, and rebuild-safe cursor clearing.

No shared auth, migration, protobuf, manifest/lockfile, server/worker
registration, console shell, `MASTER_PLAN.md`, or `EXECUTION_LEDGER.md` file
was changed.

## Verification

- `cargo fmt --all --check` — reports only the pre-existing unformatted
  console-shell block at `apps/console/src/main.rs:16624`; the changed G7
  files are formatted and the console file was left untouched.
- `cargo test -p erpx-infrastructure --lib --offline` — 216 passed, 10 ignored.
- `cargo test -p erpx-worker --lib --offline` — 213 passed, 2 ignored.
- Live SQLx cursor acceptance — 1 passed.
- Live SQLx Operations scheduler acceptance — 1 passed.
- Package Clippy with warnings denied remains blocked by the pre-existing
  unrelated `append_holding_backup_job_event` too-many-arguments warning at
  `crates/infrastructure/src/provisioning.rs:6550`; that file was not changed.

External LDAP/AD/UCS environments are not required by this bounded source-feed
slice and no vendor certification result is claimed here.
