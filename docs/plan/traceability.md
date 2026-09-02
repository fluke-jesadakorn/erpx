# Traceability — Requirement → Code → Spec

> **Archived — Rust era (pruned 2026-09-02).** Every row below links to archived Rust contracts (`crates/proto/proto/erpx/*/service.proto` on `backup/rust-2026-09-02`, briefly copied to `docs/contracts/proto/`) and Rust implementations (`crates/contexts/*/src/application.rs`). This branch is now Next.js (`src/app/`) — the living route table is TBD under `docs/contracts/next/` and the archived table is kept here for reference. See `CUTOVER.md` (2026-09-02), `CONTRACT_POLICY.md`, and `ADR-020`.

## Living — Next.js routes (TBD, grows as `src/app/api` lands)

| Route | Method | Context | Spec (light → deep) | Proto (archived) | Status |
|---|---|---|---|---|---|
| `POST /api/sales/quotes` | POST | sales | `docs/plan/02/.../sales.md:9` → `03/sales.md:15` | `sales/service.proto:CreateQuote` | TBD |
| `POST /api/finance/journals` | POST | finance | `docs/plan/02/.../finance.md:9` → `03/finance.md:15` | `finance/service.proto:CreateJournal` | TBD |
| `POST /api/pos/sales` | POST | pos | `docs/plan/02/.../pos.md:9` → `03/pos.md:15` | `pos/service.proto:CreateSale` | TBD |
| `GET /api/customer360/:id` | GET | customer_360 | `docs/plan/02/.../customer_360.md` | `customer_360/service.proto` | TBD |

See `docs/contracts/next/route-table.md` for full living table.

<details>
<summary>Archived — Rust RPCs (≈552) — click to expand (summary per context)</summary>

| Context | RPCs | Contract (archived) | Spec |
|---|---|---|---|
| agent | 30 | `crates/proto/proto/erpx/agent/service.proto` | `docs/plan/02-capabilities/13-ai-collaboration/agent.md` |
| assets | 8 | `assets/service.proto` | `assets.md` |
| benefits | 9 | `benefits/service.proto` | `benefits.md` |
| brand | 8 | `brand/service.proto` | `brand.md` |
| collaboration | 34 | `collaboration/service.proto` | `collaboration.md` |
| commercial | 22 | `commercial/service.proto` | `commercial.md` |
| crm | 10 | `crm/service.proto` | `crm.md` |
| customer_360 | 4 | `customer_360/service.proto` | `customer_360.md` |
| customer_authority | 9 | `customer_authority/service.proto` | `customer_authority.md` |
| ecommerce | 26 | `ecommerce/service.proto` | `ecommerce.md` |
| field_service | 12 | `field_service/service.proto` | `field_service.md` |
| finance | 32 | `finance/service.proto` | `finance.md` |
| identity_access | 34 | `identity_access/service.proto` | `identity_access.md` |
| inventory | 7 | `inventory/service.proto` | `inventory.md` |
| learning | 9 | `learning/service.proto` | `learning.md` |
| legal | 12 | `legal/service.proto` | `legal.md` |
| maintenance | 10 | `maintenance/service.proto` | `maintenance.md` |
| manufacturing | 18 | `manufacturing/service.proto` | `manufacturing.md` |
| marketing | 18 | `marketing/service.proto` | `marketing.md` |
| operations | 11 | `operations/service.proto` | `operations.md` |
| organization | 9 | `organization/service.proto` | `organization.md` |
| people_payroll | 16 | `people_payroll/service.proto` | `people_payroll.md` |
| planning | 14 | `planning/service.proto` | `planning.md` |
| platform_administration | 65 | `platform_administration/service.proto` | `platform_administration.md` |
| pos | 17 | `pos/service.proto` | `pos.md` |
| procurement | 16 | `procurement/service.proto` | `procurement.md` |
| projects | 23 | `projects/service.proto` | `projects.md` |
| quality | 7 | `quality/service.proto` | `quality.md` |
| reference | 3 | `reference/service.proto` | `reference.md` |
| sales | 14 | `sales/service.proto` | `sales.md` |
| support | 21 | `support/service.proto` | `support.md` |
| talent | 28 | `talent/service.proto` | `talent.md` |

Full 552 RPCs: `git checkout backup/rust-2026-09-02 -- docs/plan/traceability.md` or `crates/proto/proto/erpx/*/service.proto`

</details>
