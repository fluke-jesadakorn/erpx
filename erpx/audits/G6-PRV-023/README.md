# G6-PRV-023 — PostgreSQL/MinIO backup and restore-test adapter

## Delivered

The infrastructure crate now exposes `PostgresMinioBackupAdapter`, which is a
typed `BackupExecutor`/`RestoreExecutor` implementation behind the existing
`TenantBackupTarget` and `TenantRestoreTestTarget` seams.

- PostgreSQL logical backups use argv-only `pg_dump`; no shell or interpolated
  shell command is used.
- The database URL, MinIO credentials, object URLs, process arguments, command
  stderr, and transport errors are not returned in executor results, worker
  payloads, `Debug`, or adapter error text.
- MinIO transfers use short-lived presigned requests, bounded content length
  and streamed body size, tenant/chain/backup-key binding, checksum/size
  verification, and deterministic replay behavior.
- Restore tests download the bounded artifact, write a private temporary file,
  invoke argv-only `pg_restore` against the distinct candidate database, then
  run an injected/default PostgreSQL schema/RLS verifier. The existing
  `TenantProvisioner` state machine remains responsible for fenced rebuild and
  read activation.
- Missing PostgreSQL/MinIO configuration fails closed through `from_env()`.
- Process, object, verifier, and classification seams are injectable for tests.

## PITR boundary

`BackupRunKind::Pitr` is intentionally rejected as a permanent, fail-closed
operation by this logical-dump adapter. This is a concrete contract blocker,
not an unbounded fallback: the current typed request has no recovery timestamp
or LSN, WAL archive/slot reference, or physical-cluster target. The restore
target is a database-level replacement read key and the existing `pg_restore`
path accepts a logical custom archive; a physical `pg_basebackup`/WAL PITR
artifact cannot be safely restored into that target. MinIO currently has no
WAL-manifest/retention contract either.

Implementing true PITR requires a shared contract and durable metadata change
outside this packet (recovery point/LSN, WAL archive manifest, physical
restore target lifecycle, and a restore executor that verifies the recovered
cluster before the existing generation-fenced activation). Until that contract
exists, the adapter does not mislabel a logical dump as PITR.

## Verification

The focused `backup_adapter` tests use fakes only; they do not connect to
PostgreSQL or MinIO and do not mutate live or disposable databases. They cover
configuration/debug redaction, argv separation, tenant/object binding,
checksum and size bounds, transient/permanent classification, duplicate replay,
and failed-restore safety. A real PostgreSQL/MinIO integration remains
unproven by this packet.

## Rollback and integration order

Revert only the G6-PRV-023 files listed in the handoff. Integrate the
infrastructure adapter first, then the worker composition, then run the
focused tests and full workspace wave. No active read route or replacement
fence is changed by this packet.
