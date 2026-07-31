# G6-PRV-031 — Tenant-bound backup object inventory reconciliation

## Objective

Close the backup-retention reconciliation boundary without exposing storage
credentials or accepting a partial provider page as complete evidence.

## Delivered behavior

- MinIO/S3-compatible listing uses a short-lived signed provider query bound to
  one namespace and tenant backup-chain prefix.
- Listing validates entry and response bounds and rejects truncated or malformed
  XML responses.
- Backup adapters normalize only the opaque artifact references belonging to
  the requested tenant and chain.
- Retention workers compare expected control-plane references with provider
  inventory and persist `matched`, `drifted`, or `unavailable` evidence.
- Inventory evidence is keyed to the retention action, replay-safe, and
  rebuildable into the separate control read database.
- Provider inventory failure records redacted unavailable evidence and retries
  before metadata deletion; metadata is not completed from partial inventory.

## Verification

- `cargo fmt --all` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib object_storage::tests` — PASS; 5 tests.
- `cargo test -p erpx-infrastructure --lib backup_retention::tests` — PASS; 4 tests.
- Fresh disposable two-database SQL retention flow — PASS; migration 0056/0009,
  inventory evidence and control-read projection included; databases removed
  by cleanup.

## Open boundaries

Credentialed live MinIO inventory proof remains environment-dependent because
the current workspace has no configured backup credentials. PostgreSQL PITR/WAL
execution, owner seeding, provider onboarding, mobile targets, and the broader
G6/G12/G14 gates remain open.

## Rollback

Revert migrations 0056 and read migration 0009 together with bounded listing,
adapter, worker, projection, and reconciliation-report changes. Existing
retention action and deletion safeguards remain independently reviewable.
