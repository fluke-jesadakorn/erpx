# G7-003 Operations aggregate contracts

Task: `G7-003-OPERATIONS-AGGREGATE-CONTRACTS-043`

Status: bounded packet complete; ready for main-agent review/integration.

## Delivered behavior

The Operations domain now provides validated, source-reference-only contracts
for:

- `OperationalPlan`
- `OperationalCommitment`
- `FulfillmentPlan`
- `CapacityPlan`
- `WorkQueuePolicy`
- `OperatingCalendar`
- `Shift`
- `ShiftHandover`
- `StandardOperatingProcedure`
- `DispatchBatch`

Each aggregate has a lifecycle status and validated constructor. Aggregates
that can reference multiple source events retain a `source_refs` collection;
`OperationalCommitment` and `ExceptionCase` retain their single immutable
source reference. The `OperationsAggregate::source_references` contract
exposes the declared provenance to persistence without creating source
transactions in Operations.

Validation covers:

- explicit holding/company/branch containment;
- branch-list uniqueness and branch-only plan scopes;
- Shift company/branch containment (including the previously tautological
  holding check);
- bounded names, domains, steps, assignees, timezones, and SLA values;
- positive dependency IDs and unique commitment/item IDs;
- valid time windows and commitment due times;
- rejection of Operations-owned source domains and duplicate source event IDs.

Lifecycle transitions reject illegal transitions and backwards command times,
validate a candidate copy before mutation, and preserve terminal states. The
canonical payload path sorts unordered IDs and provenance references for stable
replay/event bytes.

## Persistence contract

`SqlxOperationsStore::save_aggregate` now derives provenance from the aggregate
itself. The explicit `save_aggregate_with_source_refs` and `apply_event` paths
fail closed unless the supplied references exactly match the aggregate's
declared references. This prevents a caller from persisting a detached source
reference set while keeping aggregate state and the immutable reference table
aligned. Existing optimistic versioning, atomic snapshot/event writes, tenant
routing, and replay behavior remain unchanged.

## Changed paths

- `crates/domain/src/operations.rs`
- `crates/infrastructure/src/operations_persistence.rs`
- `crates/infrastructure/tests/operations_persistence.rs`
- `audits/G7-003-OPERATIONS-AGGREGATES-043/evidence/README.md`

No server, worker, protobuf, migration, source-transaction, sales, purchase,
stock, invoice, journal, UI, or Folio paths were changed.

## Verification

All commands below were run from `erpx/` after the final owned-path format:

- `rustfmt --edition 2024 crates/infrastructure/src/operations_persistence.rs crates/infrastructure/tests/operations_persistence.rs` — PASS.
- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain operations::tests` — PASS; 8 passed, 56 filtered.
- `cargo test -p erpx-domain` — PASS; 64 passed.
- `cargo test -p erpx-infrastructure --test operations_persistence --no-fail-fast` — PASS; 3 passed, 3 ignored.
- `cargo test -p erpx-infrastructure --lib` — PASS; 50 passed, 4 ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-server` — PASS.
- `git diff --check` — PASS.

The source-referenced disposable PostgreSQL round-trip/replay test is present
as `source_referenced_operations_aggregate_round_trips_and_replays`, but is
ignored in this environment because both
`ERPX_TEST_TENANT_WRITE_DATABASE_URL` and `ERPX_TEST_TENANT_READ_DATABASE_URL`
are absent. The non-database provenance mismatch test passes.

## Residuals / integration order

This packet does not claim source-context event producers, Operations worker
scheduling/detection, read-model routes/UI, planning analytics, SLA monitors,
or Folio dashboard reconciliation. Those remain downstream work. The next
safe integration step is to wire source-domain event producers and worker
projectors against these stable aggregate types, preserving Operations as an
orchestration layer only.

## Rollback

Revert only the three implementation/test paths and this evidence directory;
no database migration or external runtime state was changed.
