# G11-AST-001 — Assets and Maintenance domain contracts

## Task

Bounded domain-only implementation for financial assets, custody/location
transfers, accounting lifecycle, maintenance work orders, reliability data,
downtime aggregation, and the explicit Operations capacity-impact handoff.

## Delivered behavior

- `AssetScope` maps a holding/company/optional branch to the existing
  `TenantScope` and fails closed when a command scope does not contain it.
- `Asset` has deterministic `AST-<uuid>` references and command idempotency,
  optimistic aggregate versions, closed lifecycle transitions, custody and
  location history, and authenticated actor/command attribution on transfers
  and disposal.
- Asset accounting uses integer minor units with checked arithmetic,
  straight-line depreciation, impairment, revaluation, disposal, residual
  value protection, and non-negative book-value enforcement.
- `MaintenanceWorkOrder` supports preventive and corrective work with closed
  Draft/Scheduled/InProgress/OnHold/Completed/Cancelled transitions,
  deterministic references, idempotent commands, parts, labor, downtime,
  cost totals, and overlap-safe downtime aggregation.
- `ReliabilityRecord` validates observation windows and exposes MTBF and
  availability basis points without floating-point arithmetic.
- `OperationsCapacityImpactHandoff` carries a deterministic reference and an
  existing `SourceTransactionRef` so Operations can project maintenance
  downtime without owning asset or work-order state.

## Files changed

- `erpx/crates/domain/src/asset_maintenance.rs`
- `erpx/audits/G11-AST-001-052/README.md`

No shared exports, contracts, persistence, migrations, application, server,
worker, UI, or existing files were changed.

## Contract and database effects

- No shared contract redesign.
- No database effects or migrations.
- Main-agent integration must export the module from
  `crates/domain/src/lib.rs` after this handoff.

## Verification

- `rustfmt --edition 2024 --check crates/domain/src/asset_maintenance.rs` —
  PASS.
- `cargo test -p erpx-domain` — PASS, 96 existing crate tests, 0 failed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — BLOCKED by an unrelated pre-existing formatting
  diff in `crates/infrastructure/src/sales_persistence.rs`; that file was not
  modified.

Because the packet is explicitly forbidden from editing `domain/src/lib.rs`,
the new module is not yet in the crate module graph. Therefore the seven
`#[cfg(test)]` tests in `asset_maintenance.rs` are intentionally pending main
agent export and must be collected and run during integration. The passing
crate test and clippy results above do not claim those pending tests.

## Required integration order

1. Main agent exports `pub mod asset_maintenance;`.
2. Run `cargo fmt --all --check`, then focused domain tests and clippy.
3. Review the new tests for scope containment, sibling transfer denial,
   company transfer authorization, accounting arithmetic, lifecycle closure,
   reliability metrics, downtime overlap, and Operations handoff identity.
4. Wire persistence and event/outbox contracts in a later bounded packet; do
   not make Operations the source of asset or maintenance records.

## Unfinished work

- SQLx repositories, tenant migrations, read projections, workers, transports,
  UI, accounting journal handoff, depreciation schedules, and migration data
  tooling remain outside this domain-only packet.
- Asset status synchronization with maintenance work orders remains an
  application-layer orchestration concern.

## Security concerns

- Every command requires a valid `CommandContext`, non-empty actor reference,
  and containing tenant scope; transfer checks both source and destination.
- Secret or credential fields are not modeled.
- Persistence must enforce the same scope, version, and idempotency checks
  transactionally and must preserve custody/audit history immutably.

## Rollback

The packet is additive and isolated. Rollback is removal of the new module and
its audit evidence before export; no database or runtime state is affected.
