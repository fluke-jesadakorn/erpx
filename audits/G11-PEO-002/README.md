# G11-PEO-002 — HR/payroll holding-write persistence

## Delivered behavior

- Added `SqlxHrPersistence` for employees, leave requests, expense claims,
  payroll runs with normalized lines, and expired-role reassignment metadata.
- Added forced holding RLS and typed holding-write tables in migration 0038.
- Added optimistic version checks, deterministic idempotency request hashes,
  replay/conflict handling, atomic outbox events, terminal-state triggers,
  approval separation, and payroll total/line reconciliation.
- Kept read projections, transport, workers, and UI out of this packet.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure hr_persistence -- --nocapture` — PASS; 4
  tests.
- `cargo test -p erpx-migration -- --nocapture` — PASS; 6 tests.
- Infrastructure Clippy — PASS after independent integration with the Finance
  close projection packet.

## Open boundary and rollback

HR commands, read projections, expiry workflow worker integration, payroll
reports, and live disposable-database migration evidence remain open. Rollback
is limited to rebuilding/rolling back the holding-write test database migration;
existing migrations, source data, and Folio behavior remain unchanged.
