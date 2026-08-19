# G6-PRV-029 — Holding-bound backup artifact retention

## Objective and ownership

Close the gap between durable backup-history cleanup and physical object
cleanup without allowing metadata deletion to imply that an object was
removed. Control-plane SQLx selects object locators from the active retention
claim; the worker owns the external MinIO deletion; the adapter owns bounded
S3-compatible transport. No client or holding credential is exposed.

## Delivered

- `MinioObjectStore::delete_object_bounded` signs a short-lived `DELETE`,
  validates namespace/key bounds, treats HTTP 404 as idempotent success, and
  maps transport failures to the existing redacted adapter errors.
- `BackupObjectTransport` and `PostgresMinioBackupAdapter` expose a
  holding/namespace/chain/reference-bound deletion operation.
- `HoldingBackupTarget::for_run_in_namespace` propagates the registry-owned
  storage namespace through production full-backup and restore-test paths;
  isolated legacy adapter tests retain an explicit configured-namespace
  fallback.
- The retention repository selects only object references belonging to the
  active, lease-valid, holding/holding-bound claim and the same bounded eligible
  job set used for metadata cleanup. Debug output redacts every locator.
- Production `holding_backup_retention` worker registration injects the
  configured PostgreSQL/MinIO adapter. Non-dry-run actions fail closed when the
  adapter is absent, retry when object selection/deletion fails, and complete
  metadata only after all selected artifact deletions succeed. Retries are safe
  because missing objects are success.

## Evidence

- Focused backup adapter tests — PASS; 8 tests, including holding-bound
  idempotent artifact deletion and invalid-reference rejection.
- Focused retention repository unit tests — PASS; 4 tests.
- Focused worker retention unit tests — PASS; fail-closed bounds remain green.
- Disposable two-control-database SQL proof — PASS; retention idempotency,
  holding/holding binding, object-reference selection, page-one stale-row
  rebuild, cleanup, retry, terminal failure, and exact database cleanup.
- `cargo check -p erpx-infrastructure -p erpx-worker` — PASS.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; only the explicitly disposable PostgreSQL
  integration tests remain ignored without their required databases.
- Dioxus console web/desktop checks — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings and no errors;
  `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — PASS; Folio `/operations` returned HTTP 307;
  `git diff --check` — PASS.
- Rerun of the disposable two-control-database retention proof after namespace
  propagation — PASS; temporary databases were removed by cleanup.

## Open boundary

The local MinIO health endpoint is reachable on its host-native configured port,
but no credentialed live artifact put/delete proof was run in this packet.
PostgreSQL PITR/WAL archive support, backup catalog reconciliation, and the
remaining G6 Platform Operations actions remain open. The worker's advisory
candidate scan is still fenced by exact per-action claims; it is not presented
as a global `SKIP LOCKED` scan.

## Rollback

Revert the deletion transport, namespace propagation, retention object
selection, worker injection, tests, and this audit record together. Keep the
metadata-only retention path disabled for non-dry-run production execution
until the adapter is restored.
