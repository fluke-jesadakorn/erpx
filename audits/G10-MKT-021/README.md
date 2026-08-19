# G10-MKT-021 — CRM holding-read projection seam

## Delivered

The established `SqlxMarketingStore` Marketing projection adapter now handles
the three CRM outbox aggregate types emitted by G10-MKT-020:

- `marketing_crm_accepted_lead_handoff`
- `marketing_crm_opportunity`
- `marketing_crm_attribution`

The source-event page allow-list includes those types. CRM envelopes are
validated against aggregate type, version, event type, aggregate identity, and
domain payload rules before projection.

Projection rows are persisted in the holding read database. The four read-side
tables are handoff rows, opportunity rows, attribution rows, and
`crm_projection_event_receipts`. All have `data_profile_key` policies and forced
RLS. Event receipts hash the complete envelope. Exact event replay is a
no-op; a reused event ID with changed data fails; an older aggregate version
is receipt-recorded without overwriting a newer row; a different event for an
already-projected version fails closed.

`clear_projection` clears CRM rows and receipts in the same scoped rebuild
preparation as the existing Marketing projection. The worker now preserves
`event_type` through its existing Marketing projection trait so the adapter
can enforce the CRM event allow-list. No new worker handler or database
credential logic was introduced.

## Files changed

- `crates/migration/migrations_holding_read/0030_marketing_crm_projection.sql`
- `crates/migration/src/lib.rs`
- `crates/infrastructure/src/marketing_persistence.rs`
- `crates/infrastructure/tests/marketing_crm_read_projection.rs`
- `apps/worker/src/marketing_projection.rs`
- `apps/worker/src/lib.rs`
- `audits/G10-MKT-021/README.md`

The task prompt stated that migration 0030 already existed, but it was absent
from the live worktree when audited; only one additive 0030 migration was
restored, with no duplicate migration or module added.

## Verification

- `cargo fmt --all --check` — passed.
- `cargo check -p erpx-infrastructure --tests` — passed.
- `cargo check -p erpx-worker` — passed.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — passed.
- `cargo test -p erpx-infrastructure marketing_persistence` — 5 passed.
- `cargo test -p erpx-migration` — 6 passed; doc-tests 0.
- Disposable PostgreSQL test `marketing_crm_read_projection` — 1 passed.

The disposable test applied clean holding write/read migrations and verified
all four CRM events (handoff, opportunity create, opportunity transition, and
attribution), projection application, exact replay, receipt persistence,
rebuild clearing, four forced-RLS tables, and four holding-scope policies.

## Integration boundary

Main-agent integration remains responsible for the final public export and
composition review. The existing worker Marketing projection runner now
receives the event type, but no separate CRM worker registration was added.
The runner still owns claiming/routing and must be scheduled by the worker
composition root for CRM events to be projected in production.

No gRPC, UI, provider adapter, domain, server, console, or Folio changes were
made.

## Rollback

Revert the adapter/worker/test/audit changes and remove the additive 0030
migration only before it has been applied to a shared database. Once applied,
retain the migration history and roll back by disabling CRM event scheduling;
the read tables are disposable and do not affect CRM write authority.

## Security concerns

Holding scope is applied through the existing transaction-local
`erpx.data_profile_key` setting and enforced by forced RLS. Payload identity and
source versions are checked before writes. PostgreSQL disposable tests run as
the database superuser, so runtime cross-holding filtering is represented by
forced-RLS and policy metadata rather than a false superuser behavior claim;
non-superuser production roles must remain in force-RLS deployment policy.
