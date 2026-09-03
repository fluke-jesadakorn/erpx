# Ecommerce — Deep `@` + AI (Production)

**Group:** `10-ecommerce` · **Code:** `crates/contexts/ecommerce` · **Proto:** `crates/proto/proto/erpx/ecommerce/service.proto` (`EcommerceCommandService, EcommerceQueryService` · 26 RPCs) · **Adapters:** `crates/adapters/ecommerce-sqlx` · **UI:** `apps/console/src/features/ecommerce` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/10-ecommerce/ecommerce.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (26)

- **Services:** EcommerceCommandService, EcommerceQueryService
- **RPCs:** ** CreateChannel, ApproveChannel, TransitionChannel, CreateListing, ApproveListing, PublishListing, WithdrawListing, CreateCart, PutCartLine, LockCart, StartCheckout, ReconcileCheckoutRevalidation, ReconcileCheckoutReservation, ReconcileCheckoutPayment, ReconcileCheckoutOrder, RequestReturn, ReconcileReturnDisposition, ReconcileReturnRefund, ProposeFulfillment, TransitionFulfillment, GetChannel, GetListing, GetCart, GetCheckout, GetReturn, GetFulfillment

| RPC | Service | Data touch |
|-----|---------|------------|
| `** CreateChannel` | Command/Query | `ecommerce` aggregate + outbox |
| `ApproveChannel` | Command/Query | `ecommerce` aggregate + outbox |
| `TransitionChannel` | Command/Query | `ecommerce` aggregate + outbox |
| `CreateListing` | Command/Query | `ecommerce` aggregate + outbox |
| `ApproveListing` | Command/Query | `ecommerce` aggregate + outbox |
| `PublishListing` | Command/Query | `ecommerce` aggregate + outbox |
| `WithdrawListing` | Command/Query | `ecommerce` aggregate + outbox |
| `CreateCart` | Command/Query | `ecommerce` aggregate + outbox |
| `PutCartLine` | Command/Query | `ecommerce` aggregate + outbox |
| `LockCart` | Command/Query | `ecommerce` aggregate + outbox |
| `StartCheckout` | Command/Query | `ecommerce` aggregate + outbox |
| `ReconcileCheckoutRevalidation` | Command/Query | `ecommerce` aggregate + outbox |

## Invariants (domain — keep)

- Storefront reads from commercial catalog revisions; orders flow to sales.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `ecommerce` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `commercial_catalog_revisions + orders` (light `ecommerce.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** CreateChannel/Listing → Publish → Cart → Checkout (revalidation/reservation/payment/order) → Fulfillment/Return — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** `PublishListing`→post; checkouts→sales-linked updates; `RequestReturn`→lane (`store-ops`).
- **Comments:** open; catalog revisions linked, never duplicated.

## Route & UI delta

- **Route:** `/commerce + /sales` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title ECOMMERCE, list ecommerce-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/ecommerce/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/ecommerce/src/domain.rs`
- **Adapters:** `ecommerce-sqlx`
- **UI:** `apps/console/src/features/ecommerce`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `ecommerce`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
