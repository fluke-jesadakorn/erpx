# Native Inventory valuation and consumption planning

This component implements portable Inventory rules under [ADR-052](../../adr/ADR-052-rust-backend-and-domain-aligned-frontend.md). Its [owner register](../../status/rust-backend-migration.json) marks domain/application foundations only. Inventory business migration, PostgreSQL effects, HTTP/SSR, reservations/Quality/Commerce/Finance composition, providers, jobs and native cutover remain pending. Existing TypeScript remains active.

## Source ownership and arithmetic

The Rust owner exports ordered landed-cost allocation, transit quantity/value settlement, dispatch-date rules and validated reorder quantities. Inventory uses its existing cumulative rounded allocation with the final source receiving the exact residual. It must not substitute Finance's largest-remainder allocator or reorder receipt lines. Pending transit losses reserve receivable quantity but do not enter settled cumulative value until actually approved; the native rule retains that distinction.

All integer operands and cumulative weights must fit the existing exact JavaScript safe-integer domain. Intermediates use integer arithmetic. Native boundaries explicitly reject unsafe aggregate weights, negative weights/quantities and invalid denominators instead of reproducing accidental floating precision or undefined internal-helper behavior. Reorder text follows the actual application's ASCII decimal shape:1–12 whole digits and up to3 fractional digits. Extra decimals, multiple separators and surrounding whitespace cannot silently enter through the old lower-level helper. These are documented admission guards, not claims that invalid inputs matched the old helper.

## Consumption planning ports

`public::consumption::prepare_consumption` reads scoped stock, functional-currency posting configuration, exactly one posting period and the selected lots through required application ports. Domain rules preserve source query/failure order for validated commands, full-depletion residual valuation, distinct allocations totaling the entire quantity, current lot versions, active/unexpired stock and whole serials. The returned plan retains exact original stock and lot valuation facts and ordered allocations. It contains no authorization, SQL statement, generic repository, transaction manager or provider call.

The eventual native adapter must keep actual Company/Branch/warehouse/product/source ownership, reservations and Quality holds, source/movement dates, current Finance mappings, accounting/Money effects, immediate/deferred lot constraints, audit/outbox and final source/authority witnesses in the source owner's single retained command unit. A public plan or copied stock facts cannot grant any of these. Maintenance/Field Service composition must supply its real source authorization; synthetic planning tests are not acceptance of that integration.

## Verification

The [B0 oracle](../../../fixtures/inventory/rust-valuation/README.md) executes exact retained functions on pinned Node26.11.1, recording seven complete source hashes and356 outcomes. Native tests compare values, ordered lot costs, business error codes/messages and actual query/failure traces. Separate conservation/boundary tests cover the safe-admission guards. The development generator is not imported by Rust production code, and Node is not a backend calculation dependency.
