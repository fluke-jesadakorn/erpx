# 04-supply-chain-ops — Stock/procurement/operations/assets/quality/maintenance

> **Parallel file — new.** Group 04-supply-chain-ops index. Value stream purpose: Stock/procurement/operations/assets/quality/maintenance — supply-chain execution.

**Journey:** Supply: projects WBS —dependsOn→ procurement req —fulfills→ inventory move —billedBy→ finance (Journey 2)

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| assets | Asset lifecycle draft→retired (8 RPCs) | `assets.md` | `../../03-cross-cutting/@-and-ai/assets.md` | `assets/service.proto` | `src/app/api/assets/*` TBD |
| inventory | Stock + projections (7 RPCs) | `inventory.md` | `../../03-cross-cutting/@-and-ai/inventory.md` | `inventory/service.proto` | `src/app/api/inventory/*` TBD |
| maintenance | Work orders bound to assets (10 RPCs) | `maintenance.md` | `../../03-cross-cutting/@-and-ai/maintenance.md` | `maintenance/service.proto` | `src/app/api/maintenance/*` TBD |
| operations | Exceptions/work-queue (11 RPCs) | `operations.md` | `../../03-cross-cutting/@-and-ai/operations.md` | `operations/service.proto` | `src/app/api/operations/*` TBD |
| procurement | Requisition→PO→receipt→match (16 RPCs, REQUISITION_FIELDS) | `procurement.md` | `../../03-cross-cutting/@-and-ai/procurement.md` | `procurement/service.proto` | `src/app/api/procurement/*` TBD |
| quality | Quality controls gating ops (7 RPCs) | `quality.md` | `../../03-cross-cutting/@-and-ai/quality.md` | `quality/service.proto` | `src/app/api/quality/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/04-supply-chain-ops/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`