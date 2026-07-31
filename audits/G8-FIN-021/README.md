# G8-FIN-021 — Finance close-control source outbox publication

## Task contract

- **Task ID:** G8-FIN-021
- **Objective:** Atomically publish one tenant-scoped outbox event whenever a
  source-owned close-control evidence row is inserted or advances to a
  strictly newer `source_version`.
- **Dependencies:** G8-FIN-018 close domain, G8-FIN-019 close persistence,
  G8-FIN-020 source evidence adapter and migration 0037.
- **Allowed paths:** source evidence adapter, its focused integration test, and
  this audit record. No migration was needed.
- **Interfaces consumed:** `FinanceCloseControlEvidenceSourceRecord`,
  `SqlxFinanceCloseControlEvidenceStore`, scoped `begin_scoped`, and the
  existing forced-RLS `erpx_tenant.outbox_events` table.
- **Interfaces produced:** `finance.close_control_evidence_published.v1`
  tenant event with source identity/version and bounded control/evidence
  context; replay-safe publication behavior.

## Delivered behavior

`publish` now inserts or updates the authoritative evidence row and appends the
outbox event in the same SQL transaction. The existing advisory lock serializes
the control key. An exact same-version replay returns without an event; stale,
same-version-conflicting, source-identity-changing, scope-invalid, and
closed-period/closed-review writes fail before commit and append no event. A
strictly newer version appends exactly one additional event.

The event uses the existing tenant outbox schema. Its aggregate identity is a
deterministic holding/company/period/control ID, so separate controls cannot
collide in aggregate ordering even when they share a source aggregate. Its
payload contains a
bounded source event key, source type/aggregate/version, holding/company/period
and control code, and the validated evidence reference, resolution state,
observation time, and bounded source payload inside the repository’s existing
`{aggregate_type, aggregate, version}` tenant-event envelope. It does not add
credentials or unbounded caller context.

## Database effects

No migration or schema change was required. The existing forced-RLS outbox is
written with the same `holding_ref` scope as the evidence row. The evidence
identity and close-immutability trigger rules are unchanged.

## Verification

Focused commands:

```text
cargo fmt --all --check
cargo check -p erpx-infrastructure
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
cargo test -p erpx-infrastructure finance_close_control_evidence
ERPX_TEST_TENANT_WRITE_DATABASE_URL=... cargo test -p erpx-infrastructure --test finance_close_control_evidence -- --ignored --nocapture
```

The ignored disposable test applies clean tenant migrations and asserts:

1. insert → exactly one event with the expected type, aggregate version, and
   source/control payload;
2. exact replay → no additional event;
3. version 2 → exactly one additional event;
4. stale, conflicting, changed-source, wrong-company, and closed writes → no
   additional events.

Recorded results:

- `cargo fmt --all --check` — PASS after the final envelope-shape change.
- `cargo check -p erpx-infrastructure` — PASS before the concurrent CRM
  projection edits began.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS
  before those edits; the clean reattempt is currently blocked by unrelated
  unresolved symbols in `crates/infrastructure/src/marketing_persistence.rs`.
- `cargo test -p erpx-infrastructure finance_close_control_evidence` — the
  source unit suite passed 3 tests after the final change; a clean reattempt is
  currently blocked by the same unrelated CRM compile errors.
- `ERPX_TEST_TENANT_WRITE_DATABASE_URL=... cargo test -p erpx-infrastructure
  --test finance_close_control_evidence -- --ignored --nocapture` — PASS on a
  disposable database for the atomic insert/replay/version/rejection matrix
  after the envelope and control-scoped aggregate-identity refinements.

## Security review

The transaction sets the tenant setting through `begin_scoped`; the outbox
table is forced-RLS and the insert uses the adapter’s validated holding
reference. Source identity, version, and scope are repeated in the payload for
projection validation. Source payload size and all source text fields remain
bounded by G8-FIN-020 validation. Event publication occurs only after close,
period, identity, and monotonic-version checks succeed.

## Rollback and unresolved boundary

Rollback is a source-only code revert; no migration rollback is necessary.
Because the legacy tenant outbox schema has no separate event-key column, event
idempotency is enforced by the transaction’s advisory lock and persisted
source-version binding. The remaining main integration boundary is worker/read
projection consumption of this event and any future migration toward the full
canonical event-envelope columns; this packet does not modify that boundary.
