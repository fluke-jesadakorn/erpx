# Borrowed PostgreSQL consumption planning

This slice supplies real source reads to the portable consumption application.
It creates no receipt, movement, reservation, journal, Money snapshot or owner
event. A plain `ConsumptionPlan` is data; an opaque `PgConsumptionPreparation`
retains the actual same-transaction owner facts and provides no write authority.

Commerce owns the warehouse, product and posting-settings facts. Its native
public reader proves the current Company/branch, active records, versions and
tracking mode, and retains the actual chart/inventory/cogs account mapping.
Names and other unused large product bodies are not fetched. Commerce settings
currency is deliberately not a posting-currency check: the original adapter
uses the current Finance chart's base currency for both posting and base.

Quality owns hold decisions. Its reader returns exact bounded semantic facts:
any active whole-quantity hold, an unscoped whole hold, partial held quantity and
the requested blocked lot IDs. `released`, `cancelled` and `disposed` are the
only terminal exclusions. The current `0252` rule quarantines all stock for any
whole hold, otherwise `min(stock, sum(partial quantities))`. The dedicated
`0283` `QUALITY_HOLD` reduction error applies to a whole hold without a lot.
Any active hold for a selected lot/warehouse blocks that lot. Relevant foreign
Company/branch/product facts fail closed rather than hiding a denial.

Quality uses one aggregate row and at most 500 supplied bounded lot IDs; it does
not collect every hold or fetch reason/inspection bodies. Partial sums use
PostgreSQL NUMERIC and cap at `i64::MAX` before transfer. This preserves the
decision for every admitted Inventory quantity (at most JavaScript's safe
integer maximum). A changed reason alone does not change the decision proof;
changed effective quantities, scope, whole flags or blocked lots do.

Inventory owns `commerce_stock` despite the historical prefix, plus its lots
and balances. Its adapter reads only those owned tables and the public Commerce
and Quality facades. Finance facts are a mandatory `PgConsumptionFinancePort`
with no default. The concrete test bridge consumes Finance's real public
`inventory_consumption` facade, not copied Finance SQL or a permissive result.
The portable date/stock/config/period/lot validation order remains in use.
Finance account eligibility remains a later mapping guard, preserving the
original error order rather than hiding a lot error behind early mapping checks.

Call the utility lock bundle together with current IAM, source-owner,
reservation, Workflow and Finance/Money source preparation before the first
Holding snapshot. The adapter starts no transaction, retry or provider call.
Opaque facts bind the real database, backend, transaction and postmaster
lifetime. `require_current` retains the original Commerce, Quality and Finance
facts and exact selected stock/lot inputs in that unit. Selected lot identity
TEXT is SQL-bounded before transfer; these explicit native admission bounds do
not claim that legacy arbitrary TEXT had an identical limit.

Planning deliberately does not grant an own-reservation allowance, clear a
Quality hold, reject every deferred availability condition, or certify the
absence of later movements. The eventual write phase still needs actual
Maintenance/Field Service current state, personal permissions and purchases,
the real order/reservation witness and expiry, stock/date/mapping guards, dated
Money and draft journal preparation, receipt-before-effects ordering, immutable
source/event and lot evidence, reservation consumption, exact stock/lot
after-images and final source/current authority retention. The existing `0944`
source/Finance/Money guard and `0239` lot-control constraints must run in that
same unit. This slice mounts no route and is not a complete Inventory owner.

Disposable fully migrated PostgreSQL suites cover actual Commerce scope and
mapping facts, current semantic hold decisions, whole/partial/lot distinctions,
large irrelevant bodies, exact sum saturation, source mutation and transaction
fences, real stock/lot valuations through all public owners, canonical error
ordering and writers committing while the utility fence waits. These component
tests are not production acceptance or source-owner authorization proofs.
