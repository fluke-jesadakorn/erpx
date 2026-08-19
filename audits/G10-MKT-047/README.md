# G10-MKT-047 — durable Marketing Journey holding-write persistence

## Scope

This packet delivers the authoritative holding-write persistence boundary for
the Marketing `Journey` aggregate. It adds migration `0044_marketing_journeys`,
forced-RLS journey and idempotency tables, a lossless JSON aggregate payload
with bounded query columns, typed create/load/transition SQLx methods, atomic
aggregate snapshots, complete shared outbox envelopes, optimistic lifecycle
versions, and exact idempotent receipts.

The domain aggregate now starts at positive version one so the initial Draft
snapshot and `marketing.journey_created.v1` event satisfy the existing
holding-write positive-version contract. Lifecycle transitions remain
optimistic and preserve the domain’s transition facts. This packet does not
claim durable journey worker execution, consent checks at scheduling/delivery,
provider delivery, CRM side effects, or Marketing completion.

## Evidence

- `cargo test -p erpx-infrastructure --test marketing_journey_persistence -- --nocapture`
  — PASS; migration contract test passed and the live test remained explicitly
  ignored by default.
- Disposable live PostgreSQL evidence:
  `ERPX_TEST_HOLDING_WRITE_DATABASE_URL='postgresql:///postgres?host=%2Ftmp&port=5432'
  cargo test -p erpx-infrastructure --test marketing_journey_persistence --
  --ignored --exact journey_write_is_atomic_idempotent_versioned_and_scope_bound
  --nocapture` — PASS; create, exact replay, transition, transition replay,
  envelope fields, forced RLS, and cross-holding isolation passed.
- Post-run cleanup queries found zero rows with the packet’s unique
  `marketing-journey-*` holding prefix in journeys, idempotency, or outbox.
- `cargo clippy -p erpx-infrastructure --test marketing_journey_persistence -- -D warnings`
  — PASS.
- `cargo test -p erpx-domain marketing_journey -- --nocapture` — PASS; 3
  tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; 168 domain tests, 153 active
  infrastructure tests plus 4 ignored, 75 server tests, 97 active worker
  tests plus 1 ignored, and no failures.
- Dioxus console web and desktop checks — PASS.
- Folio `bun run lint` and `bunx tsc --noEmit` — PASS; lint retains four
  pre-existing warnings in `infra/scripts/start-n8n.js` and zero errors.
- Folio `/operations` returned HTTP 307; PostgreSQL readiness passed; the
  existing Folio dev server remained on port 3004.

## Open dependencies

The durable worker journey executor, read projection, consent-at-schedule and
delivery enforcement, channel providers, CRM handoff integration, Marketing
UI, and the broader G10 gate remain open.
