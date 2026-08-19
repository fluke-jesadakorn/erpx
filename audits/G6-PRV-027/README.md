# G6-PRV-027 — Holding backup retention and cleanup

Implemented a bounded retention contract as a separate control-plane action
aggregate. It is holding/holding-bound through an exact locked registry check,
uses idempotent request hashes, has durable pending/claimed/completed state,
and reclaims only terminal (`completed`, `failed`, `dead_letter`) backup jobs
strictly older than a validated cutoff. Each action has a bounded row limit,
attempt limit, and lease. Dry-run actions transition durably but do not delete
source jobs, runs, links, or references. Replays of a completed action return
the persisted report without repeating deletion.

The cleanup removes selected job/run history and job-run links in one write
transaction. It reports only bounded counts for released object references;
backup, restore, storage, credential, and error text never crosses the public
receipt/report or debug boundary. A rebuildable read projection contains the
same safe counts and status fields only.

Files:

- `crates/infrastructure/src/backup_retention.rs`
- `crates/infrastructure/tests/backup_retention.rs`
- `crates/migration/migrations/0053_holding_backup_retention.sql`
- `crates/migration/migrations_read/0008_holding_backup_retention.sql`

This packet does not implement true PITR, physical restore, holding lifecycle
transitions, worker/server transport, object-store deletion, or Folio cutover.
The opt-in SQL integration fixture requires two disposable control PostgreSQL
databases and must not be mistaken for live restore proof.
