# G6 durable backup-job lifecycle events

## Gap closed

The server-facing `HoldingProvisioner::schedule_backup_job` path already
persisted durable backup and restore-test work, and the existing backup worker
already claimed that queue and refreshed the control read model. It did not,
however, publish an outbox event or aggregate snapshot for the schedule and
worker-owned state transitions. A process could therefore leave authoritative
job history without a replayable lifecycle event.

## Delivered

- `crates/infrastructure/src/provisioning.rs`
  - adds a bounded `platform.holding_backup_job` aggregate;
  - atomically persists a redacted aggregate snapshot and control-plane
    outbox event when a job is scheduled, claimed, completed, or retried/dead-
    lettered;
  - preserves holding/holding predicates, lease ownership, idempotency, and
    existing five-attempt bounds;
  - excludes backup references, restore-target references, candidate database
    keys, PITR metadata, and raw error text from event payloads;
  - rolls back the job/snapshot when the event write cannot commit.
- `crates/infrastructure/tests/backup_jobs.rs`
  - verifies a 1,002-job SQLx write/read projection;
  - verifies 1,002 scheduled events plus claim/completion/failure lifecycle
    events and aggregate versioning;
  - verifies idempotent replay does not publish a second event;
  - verifies an outbox idempotency conflict rolls back the newly inserted job;
  - verifies event payloads do not contain backup references.

The existing worker path remains the execution owner: its run loop projects
the write-side queue to the secret-free control read table before claiming
due work. The existing server Platform Operations backup schedule/list APIs
remain the transport/composition owners; this slice changes their shared
authoritative persistence boundary only.

## Verification

The first live disposable two-control-database run passed before the final
idempotency/rollback assertions were added. The strengthened run is queued
behind unrelated workspace Cargo work; the current compile blocker is an
unowned type-inference error in `crates/infrastructure/src/operations_persistence.rs`.
It must be rerun after that concurrent lane restores workspace compilation:

```text
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=<disposable-write-db> \
ERPX_TEST_CONTROL_READ_DATABASE_URL=<disposable-read-db> \
cargo test -p erpx-infrastructure --test backup_jobs --offline -- \
  --ignored --test-threads=1
```

Targeted rustfmt and `cargo check -p erpx-infrastructure --offline` passed for
this slice before the unrelated Operations edit entered the compilation
frontier. No external backup service or production database was used.

## Rollback

Disable the new event consumer/projector path and rebuild the existing backup
read table from the authoritative `holding_backup_jobs` rows. Reverting the
owned persistence/test changes leaves the queue, worker, read API, and
append-only existing events intact; do not delete aggregate snapshots or
outbox/audit history. The disposable acceptance databases are safe to drop;
no production state was changed.

## Acceptance boundary

This packet does not claim credentialed MinIO, PostgreSQL PITR, restore-target,
LDAP, AD DS, UCS, production activation, command-owner transfer, or Folio
write retirement.
