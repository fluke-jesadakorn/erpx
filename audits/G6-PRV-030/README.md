# G6-PRV-030 — Retention claim renewal during physical cleanup

## Objective

Keep a holding-bound backup retention claim authoritative while a worker
deletes a bounded sequence of physical artifacts.

## Delivered behavior

- `SqlxControlPlane::renew_backup_retention_claim` validates the immutable
  claim, holding/holding binding, bounded lease, worker identity, claim ID,
  active state, and database-clock lease before extending `lease_until`.
- The retention worker renews immediately before each adapter deletion.
  A lost claim stops further deletion and follows the existing redacted
  retry/terminal-failure path.
- No object locator is returned to UI or persisted in the retention report;
  physical deletion remains delegated to the existing holding-bound backup
  adapter.

## Evidence

- `cargo test -p erpx-infrastructure --lib backup_retention::tests` — PASS;
  4 tests.
- `cargo test -p erpx-worker backup_retention::tests` — PASS; 1 test.
- Fresh disposable two-control-database SQL retention flow, including active
  claim renewal — PASS; 1 test. Temporary databases were dropped by a cleanup
  trap after the run.

## Open boundaries

This packet does not claim credentialed live MinIO artifact proof, PostgreSQL
PITR/WAL recovery, backup object inventory reconciliation, owner seeding, or
the remaining Platform Operations/G6/G14 requirements. A renewal occurs
before each bounded delete; an individual provider request remains bounded by
the adapter timeout and completion still rechecks the database lease.

## Rollback

Revert the additive renewal method and the worker call together. Existing
claim expiry and completion fencing remain the authoritative safety boundary.
