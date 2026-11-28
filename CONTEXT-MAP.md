# ERPX context map

The [business glossary](CONTEXT.md) defines shared terminology. This map describes ownership and the boundaries between contexts. [ADR-052](docs/adr/ADR-052-rust-backend-and-domain-aligned-frontend.md) defines dependency direction.

## Commercial fulfillment and accounting

| Context | Owns | Shared contract |
| --- | --- | --- |
| Sales | Sales activity and the customer-facing sales process | Consumes Commerce fulfillment results and Finance document results through their published interfaces |
| Commerce | Order fulfillment, order versions and the recorded link between fulfillment and goods valuation | [Purchase receipt](modules/commerce/server/src/public/receipt.rs) and [sales delivery](modules/commerce/server/src/public/sales_delivery.rs) |
| Inventory | Stock movements, quantities, lots and reservation effects | Provides the stock receipt and stock issue effects used by Commerce |
| Finance | Ledger rules, functional valuation for posting and posted journal entries | Provides the [goods journal interface](modules/finance/server/src/public/commerce_goods_journal.rs) |
| Quality | Receipt hold decisions | Provides the hold decision used by the [Inventory receipt adapter](modules/inventory/server/src/public/commerce_receipt.rs) |

For goods or mixed sales delivery, Commerce loads the order, Inventory applies the stock issue, Finance posts the delivery journal, and Commerce advances the order version. The native command runs these effects, its receipt, audit record and outbox in one caller-owned serializable transaction. Purchase receipt uses the Inventory receipt and Quality hold ports before the Finance goods journal. Composition binds the concrete adapters to these interfaces. These relationships describe the public Rust contracts. Current caller cutover is tracked in the [migration delivery register](docs/status/rust-delivery-bundles.json).

## Products and extensions

A Product presents workflows from one or more business Modules. Each Module retains ownership of its records and decisions. A Commercial Package grants a Company access to a set of capabilities under its entitlement policy.

A future Plugin Release is a version of external functionality approved in the Platform catalogue. A Plugin Installation enables an approved release for a Holding within explicitly permitted Companies and capabilities. The plugin host checks installation scope and current user authority before calling an owner. Inventory remains the authority for stock queries exposed to a plugin.
