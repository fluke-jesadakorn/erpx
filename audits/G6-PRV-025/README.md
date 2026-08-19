# G6-PRV-025 — Durable holding backup scheduling, history, and worker execution

## Delivered

- Control migration `0052_holding_backup_jobs.sql` stores bounded full and
  restore-test jobs with holding/holding binding, idempotency hash, attempts,
  leases, retry/dead-letter state, and verification status.
- Control-read migration `0007_holding_backup_jobs.sql` stores only the
  secret-free rebuildable history projection.
- `HoldingProvisioner::schedule_backup_job` is idempotent and rejects PITR,
  malformed restore requests, unbounded availability, and attempts outside
  `1..=5`.
- `HoldingProvisioner::claim_backup_jobs` uses `FOR UPDATE SKIP LOCKED`, active
  holding/holding-state binding, bounded database-clock leases, stale-lease
  reclamation, and atomically creates a started run plus its durable
  job/attempt link before returning the claim.
- Completion and failure transitions require the exact job, worker, claim, and
  live database-clock lease, and verify the final update row count. Retry
  backoff is bounded; terminal failures become durable `dead_letter` rows with
  code-only error text.
- The control read projection advances by `(updated_at, id)` cursor pages until
  the complete source history is copied, including histories larger than one
  worker batch.
- The worker `holding_backup` handler refreshes the read projection, executes
  full backups through the PostgreSQL/MinIO adapter, prepares holding-bound
  restore candidates, and uses the fenced routed read activation path.
- Platform Operations exposes authenticated scheduling and read-side history
  transport. Responses never include backup references, restore references,
  database keys, credentials, or error text.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-migration -p erpx-infrastructure -p erpx-worker -p erpx-server` — PASS.
- `cargo test -p erpx-infrastructure provisioning` — PASS; 12 tests passed,
  including bounded/redacted/idempotent/PITR-safe scheduling validation. The
  existing disposable PostgreSQL provisioning tests remain explicitly ignored
  by their own fixture contract.
- `cargo test -p erpx-infrastructure --test backup_jobs backup_jobs_project_all_pages_and_link_claim_attempts_durably -- --ignored --nocapture` — PASS; a fresh pair of disposable control databases projected 1,001 jobs across cursor pages, claimed two jobs, and verified two durable job/attempt/run links. Both databases were removed after the run.
- `cargo test -p erpx-infrastructure --test provisioning provisioning_gates_activation_and_rejects_metadata_only_read_switch -- --ignored --nocapture` — PASS; a fresh pair of disposable control databases verified activation health gates and that metadata-only read switching fails closed. Both databases were removed after the run.
- `cargo test -p erpx-infrastructure --test backup_adapter` — PASS; 7 tests.
- `cargo test -p erpx-worker backup_runtime` — PASS; the focused filter has no
  matching legacy tests; worker backup classification coverage is included in
  the full worker unit suite.
- `cargo test -p erpx-migration` — PASS; 9 tests.

## Deliberate boundary

True PostgreSQL PITR is still fail-closed. The current contract has no WAL
archive manifest, recovery point/LSN, or physical cluster restore target, so
neither scheduling nor the adapter labels a logical dump as PITR. Live
MinIO/PostgreSQL adapter execution and physical holding restore still require
deployment credentials and a disposable holding fixture; those are not claimed
by this local evidence. The live control-plane SQL claim/projection proof does
not extend to MinIO or true PITR.

Holding activation/suspension/recovery, owner seeding, backup cleanup, backup
retention policy, production PITR, broader Platform Operations pages, and the
remaining G6/G14 requirements remain open.

## Rollback

Revert only the G6-PRV-025 files and migration additions listed in the
integration ledger. Do not reset unrelated Folio or ERPX worktree changes.

## Coordinator continuation — credentialed PostgreSQL/MinIO round trip — 2026-07-28

The live acceptance boundary is now exercised by
`crates/infrastructure/tests/backup_live.rs`. With disposable PostgreSQL
databases and a credentialed MinIO namespace, it:

- migrates physically distinct holding write/read databases;
- runs the real configured PostgreSQL 18 `pg_dump` and `pg_restore` binaries;
- uploads and downloads the holding/chain/reference-bound custom archive through
  the real MinIO object transport over HTTPS with a trusted internal CA;
- restores into a distinct candidate read database and verifies
  `erpx_holding.customer_summary` plus forced holding RLS;
- reconciles the complete object inventory, deletes the exact artifact, and
  verifies that the inventory is empty; and
- drops every disposable database after the run.

The ignored acceptance passed on the host services. The default PATH exposed
PostgreSQL 14 tools against the PostgreSQL 18 server and was rejected by the
real adapter; the test now requires explicit tool overrides for a compatible
deployment instead of hiding a version mismatch. The adapter also no longer
passes `--file=-` to `pg_dump`: stdout capture is authoritative and avoids
creating a literal file named `-`.

`MinioObjectStore::new_with_ca_certificate` accepts public internal-CA trust
material while the signed-request boundary still requires an HTTPS endpoint;
credentials remain environment-only and redacted. This closes the local
read-side logical backup/restore/object-cleanup proof. True PITR/WAL recovery,
write-side production backup certification, provider-native retention, and
production approval/cutover remain queued and are not claimed here.

## Coordinator continuation — durable PITR recovery-point admission — 2026-07-28

The coordinator added the bounded `PitrRecoveryPoint` contract and control
migration `0062_holding_pitr_recovery_points.sql`. A PITR job now requires
exactly one recovery timestamp or PostgreSQL LSN plus an opaque WAL archive
reference and physical restore-target reference. Full and restore-test jobs
reject PITR metadata. The same metadata is included in the control request
hash, persisted on the authoritative write-side job and run rows, carried
through the claim/run binding, and excluded from read-model payloads and
debug output.

The platform protobuf/API accepts the recovery point with RFC3339/LSN and
reference validation. Control-read migration `0013` admits the `pitr` kind
without projecting recovery references. The fresh disposable PostgreSQL
`backup_jobs_project_all_pages_and_link_claim_attempts_durably` acceptance
passed with 1,001 full jobs plus one durable PITR job, read projection, and
claim/run links; both databases were removed.

This closes PITR metadata admission and persistence only. The worker still
fails closed until a physical `pg_basebackup`/WAL archive/physical-cluster
restore executor is configured and verified. No logical dump is labelled as
PITR, and no production backup, restore, activation, owner switch, Folio
write retirement, or cutover occurred.

## Coordinator continuation — physical PITR artifact capture seam — 2026-07-28

`crates/infrastructure/src/pitr.rs` now provides a production-bound,
fail-closed capture adapter. It runs argv-only `pg_basebackup` in tar mode
with WAL disabled, reads a bounded WAL archive directory without following
symlinks, persists holding/chain/backup-scoped objects through the existing
MinIO transport, and reads every object back before recording a manifest with
sizes and SHA-256 digests. Staging directories are private and removed after
the attempt; command output, references, and credentials remain outside logs.

The adapter is deliberately not registered as the worker's PITR executor yet.
`UnconfiguredPhysicalPitrRestoreExecutor` returns configuration failure until
physical-cluster restore, WAL replay, target lifecycle, and independent
post-recovery verification are implemented and exercised against a credentialed
deployment. This slice closes capture-boundary behavior and tests, not G6
physical restore or production certification.

## Coordinator continuation — physical PITR replay executor seam — 2026-07-28

`PostgresPhysicalPitrRestoreExecutor` now provides the real replay path behind
an explicit `PitrRestoreTargetResolver`. It verifies the holding/chain/backup
manifest and every object digest, rehydrates the base tar and WAL files under
the resolved restore root, writes `recovery.signal` and a target-time/LSN
`restore_command`, starts the target with argv-only `pg_ctl`, checks
`pg_is_in_recovery()` through argv-only `psql`, and stops the target again.
Target paths are root-contained and database URLs are redacted from Debug.

The resolver is intentionally unconfigured and the executor is not registered
with the worker. Credentialed physical restore/replay, target lifecycle, and
independent deployment certification remain queued; no production target was
started and no activation or cutover authority changed.
