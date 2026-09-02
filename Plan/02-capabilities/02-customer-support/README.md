# 02-customer-support — Customer authority vs federated 360 view + support tickets/SLA.

> **Parallel file — new.** Group 02-customer-support index. Value stream purpose: Customer authority vs federated 360 view + support tickets/SLA.

**Journey:** Support tickets block projects; customer_360 reads support+authority

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| customer_360 | Federated read projection, no write ownership (3 RPCs) | `customer_360.md` | `../../03-cross-cutting/@-and-ai/customer_360.md` | `customer_360/service.proto` | `src/app/api/customer_360/*` TBD |
| customer_authority | Customer authority bindings, Merge + audit (9 RPCs) | `customer_authority.md` | `../../03-cross-cutting/@-and-ai/customer_authority.md` | `customer_authority/service.proto` | `src/app/api/customer_authority/*` TBD |
| support | Tickets/sessions/capabilities/SLA clocks (21 RPCs) | `support.md` | `../../03-cross-cutting/@-and-ai/support.md` | `support/service.proto` | `src/app/api/support/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/02-customer-support/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`