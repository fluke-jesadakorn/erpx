# G9-SALES-001 — Sales domain contract slice

## Delivered behavior

`erpx/crates/domain/src/sales.rs` provides a tenant-bound Sales contract slice for:

- customer master references;
- leads and opportunities;
- quotes and sales orders;
- delivery and invoice handoffs;
- collection state and bounded payment/write-off transitions.

Every record carries a deterministic aggregate reference, a namespaced command
idempotency key, command attribution, holding/company identifiers, and an
explicit company or branch `TenantScope`. Command validation requires the
caller scope to contain the record scope. Customer references may be broader
than a branch record, but may not cross holding or company boundaries.

Lifecycle transition methods are fail-closed, terminal states cannot be
reused, document totals are recomputed from validated integer minor-unit lines,
and collection payments reject non-positive amounts and overpayment.

## Files changed

- `erpx/crates/domain/src/sales.rs`
- `erpx/audits/G9-SALES-001-052/README.md`

No existing files were modified.

## Contracts consumed

- `crate::contracts::CommandContext`
- `crate::contracts::TenantScope`
- Existing `CommandContext::validate` and `TenantScope::contains` behavior.

## Contracts produced

- `SalesError`
- `SalesAggregateKind`
- `CustomerMasterReference`
- `SalesLine`
- `Lead`, `Opportunity`, `Quote`, `SalesOrder`
- `DeliveryHandoff`, `InvoiceHandoff`, `CollectionState`
- Sales lifecycle enums and deterministic reference/idempotency helpers.

## Verification

- `cargo fmt --all --check` — passed.
- `cargo test -p erpx-domain` — PASS after main integration: 96 tests, 0
  failed; doc tests passed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — passed.

The main agent exported `pub mod sales;` and corrected the handoff fixture to
keep transition timestamps monotonic; the five embedded tests are now
collected.

## Unfinished integration

- Main agent must export the module.
- No application, protobuf, gRPC, repository, migration, worker, or UI code
  is included in this packet.
- G9 end-to-end Sales, Procurement, Inventory, and Operations reconciliation
  gates remain open.

## Security concerns

The domain contract fails closed for invalid or non-contained scopes and does
not carry credentials, payment instruments, or tenant connection details.
Persistence must enforce idempotency uniqueness and optimistic concurrency;
transport must authorize the `CommandContext` before invoking transitions.

## Integration order

1. Main agent exports `sales` from `erpx/crates/domain/src/lib.rs`.
2. Main agent reruns focused formatting, tests, and clippy.
3. Application/transport adapters map these contracts without widening scope.
4. Infrastructure may persist source-owned Sales aggregates and publish
   scoped handoff events.

## Rollback

If rejected, remove only the two files listed under “Files changed”. No shared
contract, database, or runtime rollback is required.
