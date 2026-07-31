# G6-003 Backup / Restore-Test Scheduling Evidence

Status: dependency-ready transport-neutral seam delivered; real backup and restore adapters remain intentionally unfinished.

## Delivered behavior

- Added a bounded `RestoreTestScheduleRequest` contract bound to one started `TenantBackupRun` and one validated `TenantRestoreTestTarget`.
- Added tenant-scoped due-time claiming with a maximum batch of 100, a bounded maximum of five attempts, a claim lease, and duplicate/idempotency protection.
- Added typed execution classification for verified restore tests, replacement-read-unverified results, retryable failures, expiry, and terminal failure history.
- A result is `Verified` only when both the restore target and replacement read target are verified. Failed or unverified work is never marked complete or safe to switch.
- Added tenant-scoped schedule/history lookup that returns not-found across tenant boundaries.
- Added redacted `Debug` implementations for schedule requests, schedules, claims, and history entries; credentials, URLs, pools, storage clients, and raw opaque backup/restore/idempotency refs are absent from the contract.
- Added an explicitly injected worker handler/registry seam. The default worker registry does not register restore-test work and therefore remains fail-closed until a tenant-scoped source is composed.

## Changed files

- `erpx/crates/infrastructure/src/provisioning.rs`
  - scheduling, claim, retry, recovery-history, target-validation, and focused unit tests.
- `erpx/crates/infrastructure/src/lib.rs`
  - public exports for the new transport-neutral types and bounds.
- `erpx/apps/worker/src/lib.rs`
  - `RestoreTestScheduleSource`, tenant-scoped bounded handler, explicit registry composition method, and handler tests.
- `erpx/audits/G6-003-BACKUP-RESTORE-045/README.md`
  - this handoff evidence.

No migration, SQL schema, server, proto, domain, UI, plan, or ledger files were changed.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS; 54 tests, 4 disposable-DB tests explicitly ignored.
- `cargo test -p erpx-worker --lib` — PASS; 15 tests, 1 disposable-DB test explicitly ignored.
- `cargo clippy -p erpx-domain -p erpx-application -p erpx-infrastructure -p erpx-worker --all-targets -- -D warnings` — PASS.

The original blocked handoff results were caused by the Customer 360 compile
errors and are superseded by the independently observed results above.

## Residual work

- Implement the real PostgreSQL scheduler/claim adapter using the existing control-plane backup metadata without adding unapproved schema changes.
- Implement real PostgreSQL backup/PITR and MinIO restore-target adapters; no shell commands or credentials belong in this seam.
- Persist claim leases, idempotency, retries, and recovery history atomically in the eventual adapter.
- Connect the worker handler to the durable source, execute typed restore tests, and call the existing replacement-read rebuild/verification/switch state machine only after both verifications pass.
- Add durable scheduling cadence, notifications, dead-letter/replay policy, backup/PITR evidence, restore-test history UI, platform operations routes, and integration/security tests.
- Re-run package, workspace, database, backup/restore, and end-to-end verification after `erpx-domain` is repaired.

## Security concerns

- The current scheduler is a transport-neutral in-process policy seam, not durable production storage; process loss before adapter integration loses pending claims.
- The default worker registry intentionally has no restore-test handler. Registering a handler requires an explicit tenant-scoped source and tenant identifier.
- Cross-tenant claims, schedule reads, history reads, and target/run mismatches fail closed. A worker cannot claim all tenants through this API.
- Opaque references are validated for non-empty path-safe form and are redacted from the relevant debug output. The contract carries no secrets, database URLs, credentials, SQL pools, or MinIO clients.
- Replacement-read verification is mandatory for a successful classification; no API in this packet performs activation or permits a failed result to become completed.
- Lease expiry permits a bounded retry but does not itself mark a restore as recovered. The eventual durable adapter must make claim ownership and replay semantics atomic.

## Integration order

1. Repair the unrelated `erpx-domain` compilation failures and rerun the focused infrastructure/worker tests.
2. Independently review this seam and its tests; preserve the public types and fail-closed bounds as the adapter contract.
3. Implement the control-plane PostgreSQL scheduler/claim adapter with atomic idempotency, tenant predicates, leases, retry classification, and history persistence.
4. Implement credential-isolated PostgreSQL/MinIO backup and restore-test adapters behind the existing typed executor traits.
5. Compose the tenant-scoped worker source through `handler_registry_with_restore_test_source`; keep the default registry fail-closed.
6. Integrate replacement-read rebuild/verification and safe switch evidence, then add platform operations/UI and backup/restore integration verification.

## Rollback

Revert only the four files listed in **Changed files** (including this audit directory). No database migration or external runtime state was changed by this packet.

## Coordinator continuation — retention registration fail-closed blocker — 2026-07-28

The executable worker composition now registers the durable retention handler
even when the native PostgreSQL/MinIO adapter cannot be configured. It records
the external-credential blocker and uses the existing SQLx blocker path, so a
claimed action becomes retryable/dead-letter evidence rather than causing the
worker process to abort before it can report the missing certification
dependency. When credentials are present, the same registration uses the real
adapter unchanged.

Focused worker compilation, warnings-denied worker Clippy, and the existing
disposable blocker acceptance remain the evidence boundary; no backup or
object-storage success is claimed without the external environment.

## Coordinator continuation — real backup/restore adapter acceptance — 2026-07-28

The earlier residual-work statement is superseded for the logical adapter
slice. `crates/infrastructure/tests/backup_live.rs` now performs a credential-
gated, ignored acceptance against real disposable PostgreSQL and MinIO
services. It passed with PostgreSQL 18 dump/restore tools and an HTTPS test
endpoint trusted by a disposable internal CA. The test verifies a read-side
custom archive, tenant-bound MinIO upload/download, candidate restore, forced
RLS, complete inventory reconciliation, exact artifact deletion, and physical
database cleanup. No fakes are injected into this path.

The implementation fixes stdout capture for `pg_dump` (no literal
`--file=-`) and permits explicit internal-CA trust without permitting HTTP at
the operational signed-request boundary. PITR/WAL recovery, external provider
retention, and production activation remain intentionally fail-closed.
