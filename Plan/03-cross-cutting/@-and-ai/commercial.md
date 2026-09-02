# Commercial — Deep `@` + AI (Production — Stripe-authoritative)

**Group:** `01-platform-governance` · **Code:** `crates/contexts/commercial` · **Proto:** `crates/proto/proto/erpx/commercial/service.proto` (`CommercialCommandService, CommercialQueryService` · 22 RPCs) · **Adapters:** `crates/adapters/commercial-sqlx` · **UI:** `apps/console/src/features/commercial`

**Scope:** Extends `Plan/02-capabilities/01-platform-governance/commercial.md`. Central polymorphic `@` contract applies verbatim — see `_shared-contract.md` for shared @/AI/Forms/Verify/Spine/ASCII frame.

## Services & RPCs (22)

- **Services:** CommercialCommandService, CommercialQueryService
- **RPCs:** AllocateCompanyEntitlement, ActivateCompanyEntitlement, ActivateBranchEntitlement, CreateCheckoutSession, CreateCustomerPortalSession, GrantGraceExtension, CreateCatalogPlan, PublishCatalogPlan, CreateSubscription, ApplyBillingEvent, CreateEntitlement, ApplyEntitlementChange, CheckEntitlement, ListSaasCatalog, GetHoldingSubscription, ListHoldingInvoices, GetCatalogPlan, ListCatalogPlans, GetSubscription, ListSubscriptions, GetEntitlement, ListEntitlements

| RPC | Data touch |
|-----|------------|
| `CreateCatalogPlan` | `commercial` aggregate + outbox |
| `PublishCatalogPlan` | `commercial` aggregate + outbox |
| `CreateSubscription` | Stripe → `commercial` aggregate + outbox |
| `ApplyBillingEvent` | Stripe webhook → `billing_lifecycles` |
| `CheckEntitlement` | `entitlement_allocation_books` projection |
| `ListSaasCatalog` | `commercial_catalog_revisions` projection |

## Invariants (domain)

- Billing state is Stripe-authoritative; local state mirrors Stripe events (light `commercial.md:12` — restored, not generic).
- Generic 3 (holding-isolation/time-bounded/append-only) — see `_shared-contract.md`.

## Data owned (delta — precise, light is canonical)

- Write: `commercial_catalog_revisions + billing_lifecycles + entitlement_allocation_books + stripe_event_inbox` (light `commercial.md:16` canonical)
- Read: rebuildable projections; reference graph see `_shared-contract.md`

## Editable fields

- **This module:** `OwnerUnsupported (spec-ahead)` — no owner-published schema
- Binding: see `_shared-contract.md`

## Business flows (unique)

1. **Happy:** CreateCatalogPlan → Publish → CreateSubscription (Stripe) → ActivateCompanyEntitlement — creates typed edges, mirrored locally.
2. **Exception:** ApplyBillingEvent failure → dead-letter replay (CQRS DLQ).
3. **AI-assisted:** see `_shared-contract.md`.

## Route & UI delta

- **Route:** `/settings/billing + /platform/holdings` + `/work + /reference/item/:id`
- **Spec row:** `— OwnerUnsupported (spec-ahead)`

## Traceability

- **Proto:** `crates/proto/proto/erpx/commercial/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/commercial/src/domain.rs` (Stripe authority ADR-007)
- **Adapters:** `commercial-sqlx`
- **UI:** `apps/console/src/features/commercial`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `commercial`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `Plan/MAP.md`.
