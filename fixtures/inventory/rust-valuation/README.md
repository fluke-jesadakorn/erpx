# Inventory valuation oracle

`conformance.v1.json` records outputs from exact helper/application function slices of private checkpoint `a5ed8737a2f05eff1ee9841fda4a094b3926d732`, executed on Node26.11.1. Seven complete source-file hashes bind the slices. The generator checks every extraction boundary, retains the actual BusinessRuleError and Finance/Commerce arithmetic declarations, and supplies explicitly synthetic consumption query/effect ports. It opens no database and grants no authority.

The corpus contains81 landed allocations,197 transit quantities,12 dispatch-date outcomes,9 validated reorder quantities, and57 consumption preparations. It preserves source order, last-row residuals, cumulative receipt/loss valuation, pending-loss availability, positive functional-currency valuation, distinct lot allocations, current lot version, expiry, whole serials and query/failure ordering. These are356 source-oracle cases; native unit-case counts are reported separately.

Regenerate or verify from the repository root using the pinned toolchain:

```sh
ERPX_NODE_BIN=/path/to/pinned/node bun scripts/testing/fixtures/generate-inventory-rust-oracle.ts
ERPX_NODE_BIN=/path/to/pinned/node bun scripts/testing/fixtures/generate-inventory-rust-oracle.ts --check
bun scripts/testing/test.ts unit --suite=inventory.rust-valuation.unit
```

Native tests additionally check conservation, zero-weight rows, order-sensitive rounding, invalid dates, unsupported unsafe aggregates, negative weights and unvalidated decimal input. These explicit safe-admission guards are not labelled exact oracle matches. No HTTP/SSR/provider/job or canonical PostgreSQL consumption adapter is activated by this component. See the [migration contract](../../../docs/contracts/next/rust-inventory-migration.md).
