# Find the code for a change

Use the route to find code for a particular screen. Use the owner to find the business rule or persisted data behind it. A product can bring several owners together. For example, [Supply](../packages/contracts/src/product-registry.ts) presents Procurement, Inventory, and Quality in one application.

## Trace a screen

| What you are changing | Start here for Service Count |
| --- | --- |
| URL, layout, or route settings | [Service App Router entry](../apps/service/src/app/holdings/[holdingSlug]/services/field-service/settings/initial-stock/page.tsx) |
| Warehouse and product selection on page load | [Service page](../packages/feature-service/src/routes/holdings/[holdingSlug]/services/field-service/settings/initial-stock/page.tsx) |
| Form labels and interactions | [Field Service form](../packages/feature-field_service/src/components/initial-physical-stock-count.tsx) |
| Quantity, unit cost, or valuation when the draft is saved | [Inventory's `value_initial_count`](../packages/server-inventory/src/domain/service_initial_physical_count.rs) |
| Permission checks and calls to other owners during draft creation | [Rust service composition](../src/composition/src/service_initial_physical_stock_create.rs) |

## Check a rounding issue

1. Reproduce the issue with the [Service Count demo](start-here.md). Record quantity, unit cost, expected value, and the value you see after reopening the draft.
2. Compare the result with [Inventory's rounding test](../tests/unit/inventory/rust-service-initial-physical-count.test.rs). The case with a quantity of `1.500` and a unit cost of `6.01` expects `9.02`.
3. With the pinned Bun and Rust toolchains, run the registered Rust unit suite from the repository root or the attached container workspace:

   ```sh
   bun scripts/testing/test.ts unit --suite=inventory.rust-service-initial-physical-count.unit
   ```

The suite runs without a database. If the stored value and the unit test agree, inspect how the form displays it. If you change the calculation, update the relevant case and run it again. Include the command and observed result in your [PR](../CONTRIBUTING.md#pull-requests-and-review).
