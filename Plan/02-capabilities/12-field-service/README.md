# 12-field-service — Agreements/orders/dispatch

> **Parallel file — new.** Group 12-field-service index. Value stream purpose: Agreements/orders/dispatch — field execution.

**Journey:** Field service orders link assets/operations → support

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| field_service | Agreements/orders, human decision evidence (12 RPCs) | `field_service.md` | `../../03-cross-cutting/@-and-ai/field_service.md` | `field_service/service.proto` | `src/app/api/field_service/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/12-field-service/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`