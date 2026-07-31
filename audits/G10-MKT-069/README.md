# G10-MKT-069 — Durable Marketing delivery scheduler

## Delivered behavior

The Marketing delivery executable is now driven by a durable control-plane
scheduler row. The scheduler migration adds a recurring job and claim table;
the repository provides idempotent scheduling, `FOR UPDATE SKIP LOCKED`
claiming, half-open leases, renewal, recurring completion, bounded exponential
retry, terminal dead-lettering, and explicit dead-letter replay.

The scheduler wrapper renews its control-plane lease while the existing
multi-tenant delivery pass runs. A successful pass is rescheduled at the
configured cadence. A failed pass is recorded as retryable until the bounded
attempt limit, then remains in the control-plane dead-letter state. Tenant
delivery claims/results remain in the tenant write database, so the scheduler
does not become a second business source of truth.

## Evidence

- Fresh disposable PostgreSQL proof — 1 PASS: control migration application,
  idempotent schedule, claim, renewal, recurring completion, terminal
  dead-letter, and replay; the disposable database was removed after the run.
- Infrastructure scheduler unit tests — 2 PASS.
- Worker scheduler unit tests — 2 PASS.
- Migration embedding tests — 2 PASS.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only
  existing explicitly ignored disposable-database tests remaining ignored.
- `cargo check -p erpx-server` — PASS.
- Dioxus web and desktop checks — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- PostgreSQL readiness and `git diff --check` — PASS.

## Scope and remaining work

This packet closes the previously open durable control-plane scheduler/lease/
DLQ integration for the Marketing delivery worker. Vendor-native provider
reconciliation, channel onboarding, live external provider evidence, the
broader Marketing/ERP gates, migration/cutover, and Folio write retirement
remain open.
