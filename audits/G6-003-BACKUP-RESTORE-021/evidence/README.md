# G6-003 backup/restore executor seam evidence

This packet adds a typed, injectable backup/restore execution boundary for
holding-scoped recovery. It is a production-safety seam, not evidence that
live PostgreSQL backup/PITR tooling is already enabled.

## Delivered behavior

- Backup and restore requests are bound to the opaque holding/holding reference
  and reject mismatched or cross-holding results.
- Failed, expired, unverified, and replacement-unverified restore results fail
  closed before a projection switch.
- A replacement read database can be activated only after the typed executor
  reports a verified usable result and the existing state machine permits the
  switch.
- Debug output is redacted so database URLs, credentials, keys, and object
  storage secrets are not exposed.

## Independent verification

Run from `erpx/`:

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo test -p erpx-infrastructure --test provisioning` | PASS; 2 tests, 2 database tests ignored |
| `cargo test -p erpx-infrastructure --lib` | PASS; 42 tests, 3 database tests ignored |
| `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS |

## Open work and rollback

No real `pg_dump`/PITR/object-backup adapter, scheduler, restore-test history
producer, or platform operations route was added. Folio remains the command
owner. Rollback is limited to the additive executor seam and its evidence;
retain existing holding registry and replacement-read guards.
