# Plan — Map (single flat index for 31 contexts)

> Historical domain map; see [the current production plan](../production-plan.md) for release scope. Counts describe this reference map, not the current module registry.

> **Parallel file — new.** This flat table is the fastest way to find any context without remembering its group number (e.g., `sales = 03-finance-sales-pos`). It links light (`02`, 26L) ↔ deep (`03/@-and-ai`, 213L) ↔ archived proto (37 on `backup/rust-2026-09-02` `f97b74d6`) ↔ living Next route (TBD in `src/app/api` + `docs/contracts/next/route-table.md`). See also `docs/TOC.md` → `docs/plan/README.md` → `docs/plan/01-vision-and-scope.md` (spine).

## How to use

- **Know the name but not the group?** Find row by `Context` → jump straight to its `02` and `03` files.
- **Sales?** `sales` → Group `03-finance-sales-pos` → `docs/plan/02-capabilities/03-finance-sales-pos/sales.md` (13 RPCs) → `docs/plan/03-cross-cutting/@-and-ai/sales.md` (`SALES_QUOTE_FIELDS`) → `sales/service.proto` (archived) → `POST /api/sales/quotes` (TBD).

## Flat index (31 rows)

| Context | Group | Light (`02`, 26-35L) | Deep (`03/@-and-ai`, 207-236L) | Proto (archived, `backup/rust-2026-09-02`) | Next route (TBD) | @ID example | Social |
|---|---|---|---|---|---|---|---|
| brand | 01-platform-governance | `02-capabilities/01-platform-governance/brand.md` | `@-and-ai/brand.md` | `brand/service.proto` (8 RPCs) | `src/app/api/brand/*` TBD | `@brand/brand-2026-001` | post draft + lane publish |
| commercial | 01-platform-governance | `02-capabilities/01-platform-governance/commercial.md` (22 RPCs, Stripe-authoritative) | `@-and-ai/commercial.md` (OwnerUnsupported) | `commercial/service.proto` | `src/app/api/commercial/*` TBD | `@commercial/plan-2026-001` | post plan + lane grace |
| identity_access | 01-platform-governance | `02-capabilities/01-platform-governance/identity_access.md` | `@-and-ai/identity_access.md` | `identity_access/service.proto` | `src/app/api/identity/*` TBD | `@identity_access/principal-001` | post grant + lane expiry |
| organization | 01-platform-governance | `02-capabilities/01-platform-governance/organization.md` | `@-and-ai/organization.md` | `organization/service.proto` | `src/app/api/org/*` TBD | `@organization/org-001` | post node + lane suspend |
| platform_administration | 01-platform-governance | `02-capabilities/01-platform-governance/platform_administration.md` | `@-and-ai/platform_administration.md` | `platform_administration/service.proto` | `src/app/api/platform/*` TBD | `@platform_administration/holding-001` | post milestone + lane fail |
| customer_360 | 02-customer-support | `02-capabilities/02-customer-support/customer_360.md` (3 RPCs, federated read) | `@-and-ai/customer_360.md` | `customer_360/service.proto` | `src/app/api/customer360/*` TBD | `@customer_360/cust-001` | read-cards, no lane |
| customer_authority | 02-customer-support | `02-capabilities/02-customer-support/customer_authority.md` (9 RPCs, Merge) | `@-and-ai/customer_authority.md` | `customer_authority/service.proto` | `src/app/api/customer-authority/*` TBD | `@customer_authority/cust-auth-001` | post binding + lane merge |
| support | 02-customer-support | `02-capabilities/02-customer-support/support.md` (21 RPCs, SLA clocks) | `@-and-ai/support.md` | `support/service.proto` | `src/app/api/support/*` TBD | `@support/ticket-2026-509` | post ticket + lane SLA<2h |
| finance | 03-finance-sales-pos | `02-capabilities/03-finance-sales-pos/finance.md` (35L prod, 31 RPCs, 7 invariants, Finance-First) | `@-and-ai/finance.md` (236L, OwnerUnsupported, per-RPC table) | `finance/service.proto` | `src/app/api/finance/*` TBD | `@finance/journal-2026-117` | post journal + lane period-gate |
| pos | 03-finance-sales-pos | `02-capabilities/03-finance-sales-pos/pos.md` (16 RPCs, Refund bounded) | `@-and-ai/pos.md` (OwnerUnsupported) | `pos/service.proto` | `src/app/api/pos/*` TBD | `@pos/sale-2026-001` | post sale + lane refund |
| sales | 03-finance-sales-pos | `02-capabilities/03-finance-sales-pos/sales.md` (13 RPCs: CreateQuote→Fulfill, Quote→order→posting) | `@-and-ai/sales.md` (213L, **SALES_QUOTE_FIELDS OwnerPublished**) | `sales/service.proto` | `src/app/api/sales/quotes` TBD | `@sales/quote-2026-042` | post QT + lane <3d |
| assets | 04-supply-chain-ops | `02-capabilities/04-supply-chain-ops/assets.md` (8 RPCs, draft→retired) | `@-and-ai/assets.md` | `assets/service.proto` | `src/app/api/assets/*` TBD | `@assets/asset-042` | post lifecycle + lane degraded |
| inventory | 04-supply-chain-ops | `02-capabilities/04-supply-chain-ops/inventory.md` (7 RPCs) | `@-and-ai/inventory.md` | `inventory/service.proto` | `src/app/api/inventory/*` TBD | `@inventory/move-2026-883` | post move + lane shortage |
| maintenance | 04-supply-chain-ops | `02-capabilities/04-supply-chain-ops/maintenance.md` (10 RPCs) | `@-and-ai/maintenance.md` | `maintenance/service.proto` | `src/app/api/maintenance/*` TBD | `@maintenance/wo-2026-001` | post WO + lane blocked |
| operations | 04-supply-chain-ops | `02-capabilities/04-supply-chain-ops/operations.md` (11 RPCs, exception) | `@-and-ai/operations.md` | `operations/service.proto` | `src/app/api/operations/*` TBD | `@operations/exc-2026-001` | post exception + lane escalation |
| procurement | 04-supply-chain-ops | `02-capabilities/04-supply-chain-ops/procurement.md` (16 RPCs, REQUISITION_FIELDS) | `@-and-ai/procurement.md` (**REQUISITION_FIELDS**) | `procurement/service.proto` | `src/app/api/procurement/*` TBD | `@procurement/req-2026-014` | post REQ + lane approval |
| quality | 04-supply-chain-ops | `02-capabilities/04-supply-chain-ops/quality.md` (7 RPCs, gate) | `@-and-ai/quality.md` | `quality/service.proto` | `src/app/api/quality/*` TBD | `@quality/check-001` | post check + lane failed |
| legal | 05-people-legal | `02-capabilities/05-people-legal/legal.md` | `@-and-ai/legal.md` | `legal/service.proto` | `src/app/api/legal/*` TBD | `@legal/doc-001` | post version + lane approval |
| people_payroll | 05-people-legal | `02-capabilities/05-people-legal/people_payroll.md` | `@-and-ai/people_payroll.md` | `people_payroll/service.proto` | `src/app/api/people/*` TBD | `@people_payroll/assign-2026-042` | post request + lane decision |
| crm | 06-marketing-crm | `02-capabilities/06-marketing-crm/crm.md` (10 RPCs, opportunity lifecycle) | `@-and-ai/crm.md` | `crm/service.proto` | `src/app/api/crm/*` TBD | `@crm/opportunity-2026-011` | post OPP + lane stalled |
| marketing | 06-marketing-crm | `02-capabilities/06-marketing-crm/marketing.md` (18 RPCs, provider-scoped) | `@-and-ai/marketing.md` | `marketing/service.proto` | `src/app/api/marketing/*` TBD | `@marketing/campaign-001` | post campaign + lane verify |
| manufacturing | 07-manufacturing | `02-capabilities/07-manufacturing/manufacturing.md` | `@-and-ai/manufacturing.md` | `manufacturing/service.proto` | `src/app/api/manufacturing/*` TBD | `@manufacturing/order-001` | post order + lane disposition |
| planning | 08-planning | `02-capabilities/08-planning/planning.md` | `@-and-ai/planning.md` | `planning/service.proto` | `src/app/api/planning/*` TBD | `@planning/plan-001` | post scenario + lane release |
| projects | 09-projects | `02-capabilities/09-projects/projects.md` (23 RPCs, WBS) | `@-and-ai/projects.md` | `projects/service.proto` | `src/app/api/projects/*` TBD | `@projects/proj-2026-001` | post node + lane blocked |
| ecommerce | 10-ecommerce | `02-capabilities/10-ecommerce/ecommerce.md` (26 RPCs, cart→checkout) | `@-and-ai/ecommerce.md` | `ecommerce/service.proto` | `src/app/api/ecommerce/*` TBD | `@ecommerce/listing-001` | post listing + lane return |
| benefits | 11-talent-learning-benefits | `02-capabilities/11-talent-learning-benefits/benefits.md` (9 RPCs, window) | `@-and-ai/benefits.md` | `benefits/service.proto` | `src/app/api/benefits/*` TBD | `@benefits/enroll-001` | post plan + lane closing |
| learning | 11-talent-learning-benefits | `02-capabilities/11-talent-learning-benefits/learning.md` (9 RPCs, idempotent) | `@-and-ai/learning.md` | `learning/service.proto` | `src/app/api/learning/*` TBD | `@learning/course-001` | post course + lane overdue |
| talent | 11-talent-learning-benefits | `02-capabilities/11-talent-learning-benefits/talent.md` (28 RPCs, pipeline) | `@-and-ai/talent.md` | `talent/service.proto` | `src/app/api/talent/*` TBD | `@talent/requisition-001` | post REQ + lane offer |
| field_service | 12-field-service | `02-capabilities/12-field-service/field_service.md` (12 RPCs, human evidence) | `@-and-ai/field_service.md` | `field_service/service.proto` | `src/app/api/field-service/*` TBD | `@field_service/order-001` | post job + lane exception |
| agent | 13-ai-collaboration | `02-capabilities/13-ai-collaboration/agent.md` (30 RPCs, version approval) | `@-and-ai/agent.md` | `agent/service.proto` | `src/app/api/agent/*` TBD | `@agent/assignment-001` | post assign + lane approval |
| collaboration | 13-ai-collaboration | `02-capabilities/13-ai-collaboration/collaboration.md` (26 RPCs, threads) | `@-and-ai/collaboration.md` | `collaboration/service.proto` | `src/app/api/collaboration/*` TBD | `@collaboration/thread-001` | threads=posts + lane inbox |

Count: 31 contexts across 13 groups. Light = `02` (26-35L, domain invariants + precise Data owned). Deep = `03` (207-236L, 80% shared — see `_shared-contract.md` for @/AI/Forms/Verify/Spine/ASCII frame).

## Group summary (13)

| # | Group dir | Contexts | Purpose 1-line |
|---|---|---|---|
| 01 | platform-governance | brand, commercial, identity_access, organization, platform_administration | Holding/identity/commercial catalog |
| 02 | customer-support | customer_360, customer_authority, support | Customer authority vs federated 360 + tickets/SLA |
| 03 | finance-sales-pos | finance, sales, pos | Ledger + quotes/orders→posting + terminal sales |
| 04 | supply-chain-ops | assets, inventory, maintenance, operations, procurement, quality | Stock/procurement/ops/assets/quality/maintenance |
| 05 | people-legal | legal, people_payroll | Contracts/docs + people/payroll |
| 06 | marketing-crm | crm, marketing | Opportunities/accounts + marketing delivery |
| 07 | manufacturing | manufacturing | Work orders / BOM |
| 08 | planning | planning | Planning |
| 09 | projects | projects | WBS + dependencies |
| 10 | ecommerce | ecommerce | Channels/listings/cart→checkout |
| 11 | talent-learning-benefits | benefits, learning, talent | Enrollments/courses/pipelines |
| 12 | field-service | field_service | Agreements/orders/dispatch |
| 13 | ai-collaboration | agent, collaboration | Agents/pools + threads/mentions |

## Cross-module spine (from `docs/plan/01-vision-and-scope.md:52` + `docs/plan/03/.../ _journeys.md`)

`projects/proj-2026-001 —dependsOn→ procurement/req-2026-014 —fulfills→ inventory/move-2026-883 —billedBy→ finance/journal-2026-117 —authoredWith→ people_payroll/assign-2026-042 —blocks→ support/ticket-2026-509`

Revenue journey: `crm/opportunity-2026-011 → sales/quote-2026-042 → sales/order-2026-042 → finance/journal-2026-117 → finance/payment` (see `_journeys.md`).

## Notes

- Archive proto: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`
- Living route placeholder: `docs/contracts/next/route-table.md` + `src/app/api/<ctx>/` (TBD, see `CONTRACT_POLICY.md` + `docs/adr/ADR-020`)
- Invariants: unique 12 contexts kept in `02` (see `docs/plan/02/.../*.md:12`), generic 3 (holding-isolation/time-bounded/append-only) live in `_shared-contract.md`
- Edit affected references only; current delivery changes belong in `docs/production-plan.md`.
