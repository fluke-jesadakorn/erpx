# 07-manufacturing — Work orders / BOM

> **Parallel file — new.** Group 07-manufacturing index. Value stream purpose: Work orders / BOM — manufacturing execution.

**Journey:** Manufacturing orders consume inventory, produce assets

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| manufacturing | Specs/orders/material/labor/cost (12+ RPCs) | `manufacturing.md` | `../../03-cross-cutting/@-and-ai/manufacturing.md` | `manufacturing/service.proto` | `src/app/api/manufacturing/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/07-manufacturing/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`