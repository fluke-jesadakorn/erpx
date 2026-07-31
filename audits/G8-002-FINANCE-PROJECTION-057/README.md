# G8-002 Finance journal projection seam — packet 057

## Delivered behavior

- Exported the Finance AP/AR subledger domain module and collected its tests
  in the repository domain crate.
- Added bounded Finance journal source-event pages from the tenant write
  outbox with stable aggregate/version cursors.
- Added tenant read projection application with identity checks, contiguous
  version enforcement, idempotent replay, forced-RLS checkpoints, and
  holding-scoped rebuild clearing.
- Added a tenant-routed worker runner for page application and full
  clear-and-replay rebuilds with redacted errors and bounded requests.
- Added the read-side checkpoint migration `0022_finance_projection.sql`.

## Independent verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 114 passed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 75 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 24 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

## Security and rollback

All source and read operations use holding-bound transactions and fail closed
on invalid identity, cursor, version, or payload data. Worker errors are
redacted. The read database remains disposable; rollback removes the runner
and checkpoint migration while preserving authoritative write journals.

## Remaining work

AP/AR persistence, cash/bank workflows, Finance transport, budgets, SoD and
approval limits, consolidation, Thai tax, statements, Operations event
production, migration reconciliation, and the G8 gate remain open.
