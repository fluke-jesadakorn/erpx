# Context — ecommerce

**Crate:** `crates/contexts/ecommerce` · **Proto:** `crates/proto/proto/erpx/ecommerce/service.proto` · **Adapters:** `crates/adapters/ecommerce-sqlx` (where present) · **UI:** `apps/console/src/features/ecommerce` / `crates/ui-domains/*` / `crates/console-features/ecommerce`

## Services & RPCs

CreateChannel, ApproveChannel, TransitionChannel, CreateListing, ApproveListing, PublishListing, WithdrawListing, CreateCart, PutCartLine, LockCart, StartCheckout, ReconcileCheckoutRevalidation, ReconcileCheckoutReservation, ReconcileCheckoutPayment, ReconcileCheckoutOrder, RequestReturn, ReconcileReturnDisposition, ReconcileReturnRefund, ProposeFulfillment, TransitionFulfillment, GetChannel, GetListing, GetCart, GetCheckout, GetReturn, GetFulfillment

Total RPCs: 26.

## Domain (sample)

Aggregates / types: CommerceRealm, CommerceHumanDecisionEvidence

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/ecommerce-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/ecommerce/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/ecommerce` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
