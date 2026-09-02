# 01-platform-governance — Holding/identity/commercial catalog

> **Parallel file — new.** Group 01-platform-governance index. Value stream purpose: Holding/identity/commercial catalog — the platform foundation for all other groups.

**Journey:** Upstream of sales/ecommerce — commercial catalog revisions pin orders

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| brand | Owns brand drafts/publishing | `brand.md` | `../../03-cross-cutting/@-and-ai/brand.md` | `brand/service.proto` | `src/app/api/brand/*` TBD |
| commercial | Stripe-authoritative catalog/billing/entitlement (22 RPCs) | `commercial.md` | `../../03-cross-cutting/@-and-ai/commercial.md` | `commercial/service.proto` | `src/app/api/commercial/*` TBD |
| identity_access | Principals/roles/RBAC, directory connectors | `identity_access.md` | `../../03-cross-cutting/@-and-ai/identity_access.md` | `identity_access/service.proto` | `src/app/api/identity_access/*` TBD |
| organization | Holders/companies/branches, authority scope | `organization.md` | `../../03-cross-cutting/@-and-ai/organization.md` | `organization/service.proto` | `src/app/api/organization/*` TBD |
| platform_administration | Holding provisioning, data-plane agent | `platform_administration.md` | `../../03-cross-cutting/@-and-ai/platform_administration.md` | `platform_administration/service.proto` | `src/app/api/platform_administration/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/01-platform-governance/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`