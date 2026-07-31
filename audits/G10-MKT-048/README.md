# G10-MKT-048 — durable Marketing Journey execution

## Scope

This packet adds the tenant-write execution cursor and the explicit worker
resume seam for Marketing Journeys. A version-zero cursor seed is persisted
without an outbox event; every execution fact advances to a positive version
and atomically persists the cursor, generic snapshot, complete shared event
envelope, and command idempotency receipt.

The cursor row carries a bounded worker lease, claim owner, retry timestamp,
attempt count, last error, and dead-letter timestamp. Claims use
`FOR UPDATE SKIP LOCKED`; expired leases can be resumed by another worker.
Worker command identity is deterministic for a run and cursor version, so an
uncertain commit can be replayed exactly. Pause and resume use the same
versioned boundary.

The worker binary exposes this only as an explicit handler with
`ERPX_WORKER_HANDLER=marketing_journey_execution`,
`ERPX_MARKETING_JOURNEY_TENANT_ID`, and
`ERPX_MARKETING_JOURNEY_HOLDING_REF`. The default decision source advances
linear and wait nodes only and dead-letters branches until a permission-aware
decision provider is supplied. It performs no channel, CRM, or n8n side
effects.

## Changed paths

- `crates/migration/migrations_tenant_write/0045_marketing_journey_execution.sql`
- `crates/infrastructure/src/marketing_journey_execution_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/marketing_journey_execution_persistence.rs`
- `crates/domain/src/marketing_journey.rs`
- `apps/worker/src/marketing_journey_execution.rs`
- `apps/worker/src/lib.rs`
- `apps/worker/src/main.rs`

## Evidence

- Migration contract test — PASS.
- Disposable PostgreSQL test `execution_claim_resume_replay_and_scope_are_durable` — PASS.
  It proved expired-lease takeover, stale-owner rejection, atomic execution,
  exact replay, pause/resume versions, outbox count, forced RLS, and
  cross-holding isolation. Unique fixture rows were cleaned.
- `cargo test -p erpx-infrastructure marketing_journey_execution` — PASS.
- `cargo test -p erpx-worker marketing_journey_execution` — PASS; 2 tests.
- Strict package Clippy for infrastructure and worker — PASS.
- Full integration wave — PASS: workspace formatting, workspace Clippy,
  workspace tests, Dioxus web/desktop checks, Folio lint/typecheck, Folio
  `/operations` runtime probe, and PostgreSQL readiness.

## Open dependencies

This packet does not claim completion of consent checks at schedule/delivery,
provider delivery, CRM handoff side effects, journey read projection,
Marketing UI, full Marketing, or the overall ERPX migration. Those remain
separately gated packets under the active goal.
