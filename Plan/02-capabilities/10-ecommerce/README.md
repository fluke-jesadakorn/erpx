# 10-ecommerce — Channels/listings/cart→checkout

> **Parallel file — new.** Group 10-ecommerce index. Value stream purpose: Channels/listings/cart→checkout — storefront.

**Journey:** Ecommerce orders flow to sales (orders → sales)

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| ecommerce | Channels/listings/cart→checkout (26 RPCs, catalog revision pinning) | `ecommerce.md` | `../../03-cross-cutting/@-and-ai/ecommerce.md` | `ecommerce/service.proto` | `src/app/api/ecommerce/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/10-ecommerce/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`