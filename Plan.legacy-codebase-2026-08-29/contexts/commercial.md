# Context — commercial

**Crate:** `crates/contexts/commercial` · **Proto:** `crates/proto/proto/erpx/commercial/service.proto` · **Adapters:** `crates/adapters/commercial-sqlx` (where present) · **UI:** `apps/console/src/features/commercial` / `crates/ui-domains/*` / `crates/console-features/commercial`

## Services & RPCs

AllocateCompanyEntitlement, ActivateCompanyEntitlement, ActivateBranchEntitlement, CreateCheckoutSession, CreateCustomerPortalSession, GrantGraceExtension, CreateCatalogPlan, PublishCatalogPlan, CreateSubscription, ApplyBillingEvent, CreateEntitlement, ApplyEntitlementChange, CheckEntitlement, ListSaasCatalog, GetHoldingSubscription, ListHoldingInvoices, GetCatalogPlan, ListCatalogPlans, GetSubscription, ListSubscriptions, GetEntitlement, ListEntitlements

Total RPCs: 22.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/commercial-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/commercial/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/commercial` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
