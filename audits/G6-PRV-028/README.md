# G6-PRV-028 — Retention lifecycle hardening and worker registration

## Delivered

- The retention read projection now clears and rebuilds atomically across all
  keyset pages, so a single bounded page cannot leave stale rows behind.
- Forward control migration `0055_holding_backup_retention_binding.sql` adds a
  database-enforced composite `(holding_id, data_profile_key)` foreign key.
- Completion relies on the SQL database clock for lease validity.
- Claimed actions have an authoritative retry/terminal-failure transition;
  error text remains out of the persisted/reporting contract.
- `holding_backup_retention` is registered as an executable worker handler. It
  claims due actions with bounded control-plane queries, completes metadata
  cleanup, and records retry/terminal counters.

The worker deliberately does not delete MinIO objects based on metadata
counts. Physical object deletion requires a provider-owned, namespace-bound,
idempotent object lifecycle contract and remains fail-closed.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; explicitly ignored disposable-database
  tests remained ignored.
- `cargo clippy -p erpx-infrastructure -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure backup_retention --lib` — PASS; 4 tests.
- `cargo test -p erpx-worker backup_retention --lib` — PASS; 1 test.
- `cargo test -p erpx-infrastructure --test backup_retention retention_sqlx_flow_binds_holding_holding_and_replays_idempotently -- --ignored --nocapture` — PASS on a fresh disposable control write/read database pair; page size 1 forced multi-page rebuild, a stale read row was removed, DB-backed retry/terminal failure, idempotency, application and direct SQL scope binding, dry-run, cleanup, and redaction were exercised. Databases were removed after the run.
- `cargo test -p erpx-worker --test backup_retention retention_worker_claims_completes_and_projects_an_action -- --ignored --nocapture` — PASS on a fresh disposable control write/read database pair; the worker claimed, completed, and durably projected one action with redacted error mapping. Databases were removed after the run.

## Open boundary

The candidate scan is an advisory bounded query; exact per-action SQL claims
still fence concurrent workers and return no claim when another worker wins.
This packet does not implement physical MinIO object deletion, true
PostgreSQL PITR, restore execution, holding owner seeding, remaining Platform
Operations actions, migration cutover, or Folio write retirement. The G6 and
G14 gates remain open.

## Rollback

Revert only the retention lifecycle API, worker module/registration, and
associated tests and audit record. Preserve unrelated Folio and ERPX changes.
