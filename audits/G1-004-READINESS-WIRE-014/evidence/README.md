# G1-004-READINESS-WIRE-014 evidence

## Delivered

- Added control-write migration `0011_worker_readiness.sql` for durable worker
  liveness observations.
- Worker registered-handler passes persist the configured worker identity's
  heartbeat only after a successful handler pass.
- The server `/readyz` path reads the durable heartbeat, probes the explicitly
  configured holding write/read routes, and evaluates projection and backup/PITR
  observations through the existing bounded freshness policies.
- Missing, stale, future, failed, or unconfigured observations remain
  `not_ready`; readiness no longer uses a per-request green default.
- Readiness responses expose only bounded state labels and redacted status;
  connection URLs, credentials, SQL errors, and registry references do not
  cross the unauthenticated boundary.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- Worker, server, infrastructure, and identity focused suites — PASS.
- Disposable `sqlx_control_plane` test with fresh control write/read databases
  — PASS after recompiling the embedded migration; migration 0011 and the
  durable heartbeat round-trip were exercised, and both databases were dropped.

## Remaining concerns

- A live green `/readyz` response still requires deployment configuration for
  an expected readiness holding, holding routing template/admin connection,
  expected worker UUID, fresh projection metadata, and verified backup/PITR.
- This packet does not make an unconfigured deployment ready and does not
  replace the worker's eventual production scheduler/backup implementation.
