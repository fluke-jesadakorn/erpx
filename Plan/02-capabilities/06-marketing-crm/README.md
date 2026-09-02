# 06-marketing-crm — Opportunities/accounts + marketing delivery (provider-scoped).

> **Parallel file — new.** Group 06-marketing-crm index. Value stream purpose: Opportunities/accounts + marketing delivery (provider-scoped).

**Journey:** Funnel: marketing form → crm opportunity → sales quote

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| crm | Opportunities/accounts — decoupled from sales ordering (10 RPCs) | `crm.md` | `../../03-cross-cutting/@-and-ai/crm.md` | `crm/service.proto` | `src/app/api/crm/*` TBD |
| marketing | Content/journeys/delivery + provider/domain lifecycle (18 RPCs, abuse windows) | `marketing.md` | `../../03-cross-cutting/@-and-ai/marketing.md` | `marketing/service.proto` | `src/app/api/marketing/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/06-marketing-crm/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`