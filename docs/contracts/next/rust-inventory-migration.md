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

## Unreserved consumption guards

The native preparation now exposes a borrowed `finance_source()` reference to the actual retained Finance proof. It does not reconstruct a proof from the plain plan or grant posting permission.

A separate writer-side utility fence locks Inventory's existing `inventory_active_reservations` and `inventory_ledger` views before the first Holding snapshot. PostgreSQL18 recursively locks relations named by a locked view; the owner coordinator must use its fixed lock order and acquire the strongest needed writer intent before reads. The eventual parts writer takes Finance journal/line/Money intent first, then these Inventory utilities and the remaining factual sources. Existing write-role privileges are unchanged. See the primary [PostgreSQL LOCK contract](https://www.postgresql.org/docs/18/sql-lock.html).

`require_unreserved_stock_and_movement_date` uses zero own-reservation allowance, the actual active-reservation view and the retained Quality whole/partial quantity facts. Numeric SQL arithmetic avoids aggregate overflow. Availability refusal precedes future-ledger-date refusal. `require_selected_lot_available` rechecks each selected lot's original identity/balance/status/expiry and actual current hold facts before that lot's effect. This is a guard, not stock authorization or a mutation capability, and it does not add Field Service's separately owned reservation allowance.

The full12-case actual PostgreSQL target passes, including active/expired reservations, whole/partial/selected-lot holds, backdating versus equal movement date, a real reservation writer waiting ahead of the view fence, and same-unit source/version changes. No journal, Money or consumption effect is inferred from those reads.

The current factual method first revalidates the broad planning proof. Combined same-unit Finance/other-lot changes plus an availability/date denial can therefore produce a source-change error earlier than the original effect-specific assertion. Exact combined-denial precedence remains a gate for the actual owner apply assembly; this component does not claim complete command parity. Maintenance's actual source/purchase/receipt and intended stock/lot/Finance/Money/event after-images remain required, as do the separate Field Service reservation/event policies.


## Borrowed Maintenance consumption effects

The native Inventory writer consumes its opaque preparation into a retained unreserved-effects object before Finance preparation. Deterministic command/source/actor identities are input data, not authorization. The actual Maintenance coordinator must supply current IAM, source policy and Assets purchase admission, use Finance's owned parts bridge, and retain the original Workflow receipt/audit/outbox and notifications in the same transaction. This path accepts no own-reservation credit and cannot implement Field Service's distinct reservation-trigger/version protocol.

Expected stock, selected lot balances/status/versions, other-warehouse balances and consumption/evidence/move identities are derived before writes. Full owned lot rows are hashed inside PostgreSQL so unrelated persisted fields cannot be changed silently or copied into logs. Journal insertion precedes the consumption row; consumption precedes immediate-FK lot evidence and negative lot moves; the actual Money snapshot precedes stock reduction. Final checks compare the predetermined images after the remaining owner effects and deferred constraints, rather than adopting trigger output as a new baseline. No independent transaction, retry, receipt or authorization is created.

The existing0331 attachment trigger increments the physical lot version when depletion changes status; partial consumption leaves that version unchanged. A lot remains active when another warehouse still has positive stock. Both rules are represented in the precomputed images. Initial PostgreSQL verification exposed the missing depletion version increment; the original failed run was retained and the implementation corrected without relaxing guards.

The expanded16-case PostgreSQL target passes, including successful untracked/partial/depleted/multiple-lot/serial effects, other-warehouse retention, the original reservation/date/Quality planning cases, and complete rollback on a stock trigger rewrite or a final lot identity rewrite. The effect tests use actual native Finance Money preparation plus explicit synthetic journal/Work fixture effects and real deferred schema constraints. They do not prove native owner authorization, purchase/Workflow/Finance bridge integration, notification behavior or command completion. Full owner assembly and the combined-denial ordering gate above remain pending.
