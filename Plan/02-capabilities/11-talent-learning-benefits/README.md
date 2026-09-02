# 11-talent-learning-benefits — Enrollments/courses/pipelines

> **Parallel file — new.** Group 11-talent-learning-benefits index. Value stream purpose: Enrollments/courses/pipelines — talent domain.

**Journey:** Talent → projects → people_payroll (Journey 3)

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| benefits | Benefits plans, enrollment windows (9 RPCs) | `benefits.md` | `../../03-cross-cutting/@-and-ai/benefits.md` | `benefits/service.proto` | `src/app/api/benefits/*` TBD |
| learning | Courses/enrollments, idempotent→benefits eligibility (9 RPCs) | `learning.md` | `../../03-cross-cutting/@-and-ai/learning.md` | `learning/service.proto` | `src/app/api/learning/*` TBD |
| talent | Recruiting pipelines, auditable decisions (28 RPCs) | `talent.md` | `../../03-cross-cutting/@-and-ai/talent.md` | `talent/service.proto` | `src/app/api/talent/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/11-talent-learning-benefits/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`