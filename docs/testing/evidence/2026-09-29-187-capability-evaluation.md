# ERPX all-capability mapped acceptance evaluation

Date: 2026-09-29

## Execution summary

- Inventory checked: **187 capabilities**, **21 scenarios** (S01, S02, S03, S04, S05, S06, S07, S08, S09, S10, S11, S12, S13, S14, S15, S16, S17, S18, S19, S20, S21).
- Runnable file references: **68 unique files**: 57 integration files and 11 browser files; all referenced files exist.
- Integration execution: **57/57 files, 532/532 tests passed** using Node 24.20.0, Bun 1.2.17, PostgreSQL 18.6.
- Browser execution: **11/11 directly mapped E2E files passed** in an isolated local simulation.
- Capability mappings: **180/187** link to integration tests, **22/187** link directly to browser tests, and **7** have browser-only runnable refs. Each of the 187 capabilities has at least one mapped runnable file; each linked file passed.
- These are shared/grouped suite executions mapped to capabilities, not 187 unique one-to-one test files. Passing the mapped suites is an automated observation; it does not show that each capability’s full acceptance criteria or every gap is closed.

## Acceptance status

- Accepted gate updates from this run: **0**.
- Business acceptance manifest currently has **748/748 gate entries pending** (automated, browser, provider, and production for each capability). This run adds partial automated evidence only; the generator does not promote gates from observations.
- Provider sandbox approval, production verification, and company/reviewer sign-off were not performed by this local simulation. No claim of company acceptance is made.

## Capability-by-capability mapping

| Capability | Scenarios | Mapped runnable files | Executed result |
|---|---|---|---|
| `agent/agent-catalog` | S20 | `tests/agent-governance.test.ts` | Pass (integration) |
| `agent/assignments` | S20 | `tests/agent-governance.test.ts` | Pass (integration) |
| `agent/feature-field-suggestions` | S20 | `tests/agent-governance.test.ts` | Pass (integration) |
| `agent/feature-invocation-evidence` | S20 | `tests/agent-governance.test.ts` | Pass (integration) |
| `agent/feature-pools-routing` | S20 | `tests/agent-governance.test.ts` | Pass (integration) |
| `agent/feature-private-sessions` | S20 | `tests/agent-governance.test.ts` | Pass (integration) |
| `agent/versions` | S20 | `tests/agent-governance.test.ts` | Pass (integration) |
| `assets/asset-register` | S12, S13 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `assets/condition` | S12, S13 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `assets/feature-maintenance-lifecycle` | S12, S13 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `assets/feature-placement` | S12, S13 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `assets/feature-retirement` | S12, S13 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `benefits/coverage` | S18 | `tests/benefits.test.ts` | Pass (integration) |
| `benefits/dependents` | S18 | `tests/benefits.test.ts` | Pass (integration) |
| `benefits/feature-enrollment-windows` | S18 | `tests/benefits.test.ts` | Pass (integration) |
| `benefits/feature-plans` | S18 | `tests/benefits.test.ts` | Pass (integration) |
| `benefits/feature-provider-payroll` | S18 | `tests/benefits.test.ts` | Pass (integration) |
| `brand/feature-category-locks` | S19 | `tests/branding.test.ts` | Pass (integration) |
| `brand/feature-draft-workspace` | S19 | `tests/branding.test.ts` | Pass (integration) |
| `brand/feature-published-identity` | S19 | `tests/branding.test.ts` | Pass (integration) |
| `collaboration/feature-action-cards` | S12, S20 | `tests/e2e/demo-collaboration-action-card.spec.ts` | Pass (browser) |
| `collaboration/feature-attachments` | S12, S20 | `tests/e2e/demo-collaboration-attachments.spec.ts` | Pass (browser) |
| `collaboration/feature-comments-moderation` | S12, S20 | `tests/social-beta.test.ts` | Pass (integration) |
| `collaboration/feature-following-reactions` | S12, S20 | `tests/social-beta.test.ts` | Pass (integration) |
| `collaboration/feature-record-relations` | S12, S20 | `tests/e2e/demo-collaboration-record-relations.spec.ts` | Pass (browser) |
| `collaboration/feature-topics-feed` | S12, S20 | `tests/social-beta.test.ts` | Pass (integration) |
| `collaboration/mentions` | S12, S20 | `tests/e2e/demo-collaboration-social.spec.ts` | Pass (browser) |
| `collaboration/threads` | S12, S20 | `tests/e2e/demo-collaboration-social.spec.ts` | Pass (browser) |
| `commercial/entitlements` | S15 | `tests/billing.test.ts` | Pass (integration) |
| `commercial/feature-catalog-plans` | S15 | `tests/billing.test.ts` | Pass (integration) |
| `commercial/feature-grace-recovery` | S15 | `tests/billing.test.ts` | Pass (integration) |
| `commercial/feature-subscriptions` | S15 | `tests/billing.test.ts` | Pass (integration) |
| `commercial/invoices` | S15 | `tests/billing.test.ts` | Pass (integration) |
| `crm/accounts` | S01, S20 | `tests/business-commands.test.ts` | Pass (integration) |
| `crm/activities` | S01, S20 | `tests/customer-followups.test.ts`<br>`tests/crm-activities.test.ts`<br>`tests/e2e/demo-crm-activities.spec.ts` | Pass (integration + browser) |
| `crm/feature-leads` | S01, S20 | `tests/crm.test.ts` | Pass (integration) |
| `crm/feature-opportunities` | S01, S20 | `tests/crm.test.ts` | Pass (integration) |
| `crm/feature-sales-handoff` | S01, S20 | `tests/business-commands.test.ts` | Pass (integration) |
| `customer_360/feature-customer-notes` | S01, S20 | `tests/customer-notes.test.ts` | Pass (integration) |
| `customer_360/feature-customer-profile` | S01, S20 | `tests/business-commands.test.ts` | Pass (integration) |
| `customer_360/sales-summary` | S01, S20 | `tests/business-commands.test.ts` | Pass (integration) |
| `customer_360/service-timeline` | S01, S20 | `tests/customer-timeline.test.ts` | Pass (integration) |
| `customer_authority/access-history` | S01, S20 | `tests/customer-access.test.ts` | Pass (integration) |
| `customer_authority/duplicate-review` | S01, S20 | `tests/customer-duplicates.test.ts` | Pass (integration) |
| `customer_authority/feature-authority-bindings` | S01, S20 | `tests/customer-access.test.ts` | Pass (integration) |
| `customer_authority/feature-identity-changes` | S01, S20 | `tests/customer-identity-changes.test.ts` | Pass (integration) |
| `ecommerce/channels` | S16 | `tests/ecommerce.test.ts` | Pass (integration) |
| `ecommerce/feature-carts` | S16 | `tests/ecommerce.test.ts` | Pass (integration) |
| `ecommerce/feature-checkout` | S16 | `tests/ecommerce.test.ts` | Pass (integration) |
| `ecommerce/feature-fulfillment` | S16 | `tests/ecommerce.test.ts` | Pass (integration) |
| `ecommerce/listings` | S16 | `tests/ecommerce.test.ts` | Pass (integration) |
| `ecommerce/returns` | S16 | `tests/ecommerce.test.ts` | Pass (integration) |
| `field_service/feature-agreements` | S10 | `tests/field-service.test.ts` | Pass (integration) |
| `field_service/feature-billing-release` | S10 | `tests/field-service.test.ts` | Pass (integration) |
| `field_service/feature-dispatch` | S10 | `tests/field-service.test.ts` | Pass (integration) |
| `field_service/feature-exceptions` | S10 | `tests/field-service.test.ts` | Pass (integration) |
| `field_service/feature-visits` | S10 | `tests/field-service.test.ts` | Pass (integration) |
| `field_service/service-orders` | S10 | `tests/field-service.test.ts` | Pass (integration) |
| `finance/feature-bank-reconciliation` | S02, S04, S14 | `tests/finance-close-reconciliation.test.ts` | Pass (integration) |
| `finance/feature-chart-of-accounts` | S02, S04, S14 | `tests/e2e/demo-finance-chart-setup.spec.ts` | Pass (browser) |
| `finance/feature-opening-balances` | S02, S04, S14 | `tests/finance-opening-balances.test.ts` | Pass (integration) |
| `finance/feature-payments` | S02, S04, S14 | `tests/finance-generic-payments.test.ts`<br>`tests/finance-payment-obligations.test.ts`<br>`tests/e2e/demo-payroll-runs.spec.ts` | Pass (integration + browser) |
| `finance/feature-period-close` | S02, S04, S14 | `tests/finance-fx-revaluation.test.ts` | Pass (integration) |
| `finance/feature-receivables-aging` | S02, S04, S14 | `tests/finance-fx-revaluation.test.ts` | Pass (integration) |
| `finance/feature-reports` | S02, S04, S14 | `tests/e2e/demo-finance-reports.spec.ts` | Pass (browser) |
| `finance/journals` | S02, S04, S14 | `tests/finance-accruals.test.ts`<br>`tests/finance-consolidation.test.ts` | Pass (integration) |
| `finance/payables` | S02, S04, S14 | `tests/finance-accruals.test.ts`<br>`tests/finance-consolidation.test.ts` | Pass (integration) |
| `identity_access/access-reviews` | S12, S13, S15 | `tests/iam.test.ts` | Pass (integration) |
| `identity_access/assignments` | S12, S13, S15 | `tests/account-invitation-delivery.test.ts` | Pass (integration) |
| `identity_access/feature-directory-connectors` | S12, S13, S15 | `tests/iam.test.ts` | Pass (integration) |
| `identity_access/feature-identity-conflicts` | S12, S13, S15 | `tests/iam.test.ts` | Pass (integration) |
| `identity_access/feature-people-principals` | S12, S13, S15 | `tests/account-recovery.test.ts` | Pass (integration) |
| `identity_access/feature-roles-permissions` | S12, S13, S15 | `tests/iam.test.ts` | Pass (integration) |
| `inventory/availability` | S02, S03, S14 | `tests/inventory-lot-serial.test.ts` | Pass (integration) |
| `inventory/feature-issues-movements` | S02, S03, S14 | `tests/inventory-transit.test.ts` | Pass (integration) |
| `inventory/feature-receipts` | S02, S03, S14 | `tests/inventory-lot-serial.test.ts` | Pass (integration) |
| `inventory/feature-stock-register` | S02, S03, S14 | `tests/inventory-lot-serial.test.ts` | Pass (integration) |
| `inventory/reservations` | S02, S03, S14 | `tests/stock-reservations.test.ts` | Pass (integration) |
| `learning/course-catalog` | S18 | `tests/learning.test.ts` | Pass (integration) |
| `learning/feature-assignments` | S18 | `tests/learning.test.ts` | Pass (integration) |
| `learning/feature-attendance-results` | S18 | `tests/learning.test.ts` | Pass (integration) |
| `learning/feature-completion` | S18 | `tests/learning.test.ts` | Pass (integration) |
| `learning/feature-eligibility-handoff` | S18 | `tests/learning.test.ts` | Pass (integration) |
| `legal/contracts` | S01, S09, S21 | `tests/legal.test.ts` | Pass (integration) |
| `legal/feature-archive` | S01, S09, S21 | `tests/legal.test.ts` | Pass (integration) |
| `legal/feature-document-access` | S01, S09, S21 | `tests/legal.test.ts` | Pass (integration) |
| `legal/feature-documents` | S01, S09, S21 | `tests/legal.test.ts` | Pass (integration) |
| `legal/feature-legal-holds` | S01, S09, S21 | `tests/legal.test.ts` | Pass (integration) |
| `maintenance/downtime` | S07, S10 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `maintenance/feature-blocked-work` | S07, S10 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `maintenance/feature-completion` | S07, S10 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `maintenance/feature-maintenance-history` | S07, S10 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `maintenance/work-orders` | S07, S10 | `tests/asset-maintenance.test.ts` | Pass (integration) |
| `manage_erpx/billing` | S15, S19 | `tests/billing.test.ts`<br>`tests/billing-api.test.ts` | Pass (integration) |
| `manage_erpx/executive` | S15, S19 | `tests/platform-operations.test.ts` | Pass (integration) |
| `manage_erpx/iam` | S15, S19 | `tests/iam.test.ts`<br>`tests/iam-providers.test.ts` | Pass (integration) |
| `manage_erpx/overview` | S15, S19 | `tests/platform-operations.test.ts` | Pass (integration) |
| `manufacturing/feature-costs-genealogy` | S11 | `tests/production.test.ts` | Pass (integration) |
| `manufacturing/feature-labor-output` | S11 | `tests/production.test.ts` | Pass (integration) |
| `manufacturing/feature-specifications` | S11 | `tests/production.test.ts` | Pass (integration) |
| `manufacturing/material-readiness` | S11 | `tests/production.test.ts` | Pass (integration) |
| `manufacturing/orders` | S11 | `tests/production.test.ts` | Pass (integration) |
| `manufacturing/quality-disposition` | S11 | `tests/production.test.ts` | Pass (integration) |
| `marketing/feature-content-previews` | S01, S16 | `tests/marketing.test.ts` | Pass (integration) |
| `marketing/feature-domains` | S01, S16 | `tests/marketing.test.ts` | Pass (integration) |
| `marketing/feature-engagement` | S01, S16 | `tests/e2e/demo-marketing-to-crm-lead.spec.ts`<br>`tests/marketing.test.ts` | Pass (integration + browser) |
| `marketing/feature-forms-profiles` | S01, S16 | `tests/marketing.test.ts` | Pass (integration) |
| `marketing/feature-lead-candidates` | S01, S16 | `tests/marketing.test.ts` | Pass (integration) |
| `marketing/feature-provider-readiness` | S01, S16 | `tests/marketing.test.ts` | Pass (integration) |
| `operations/feature-approval-decisions` | S09, S10, S12 | `tests/staff-work.test.ts`<br>`tests/e2e/demo-task-delegation.spec.ts` | Pass (integration + browser) |
| `operations/feature-escalations` | S09, S10, S12 | `tests/staff-work.test.ts`<br>`tests/e2e/demo-task-delegation.spec.ts` | Pass (integration + browser) |
| `operations/feature-exception-queue` | S09, S10, S12 | `tests/staff-work.test.ts`<br>`tests/e2e/demo-task-delegation.spec.ts` | Pass (integration + browser) |
| `operations/feature-operations-summary` | S09, S10, S12 | `tests/staff-work.test.ts`<br>`tests/e2e/demo-task-delegation.spec.ts` | Pass (integration + browser) |
| `operations/feature-queue-policies` | S09, S10, S12 | `tests/staff-work.test.ts`<br>`tests/e2e/demo-task-delegation.spec.ts` | Pass (integration + browser) |
| `operations/feature-resolution` | S09, S10, S12 | `tests/staff-work.test.ts`<br>`tests/e2e/demo-task-delegation.spec.ts` | Pass (integration + browser) |
| `organization/feature-business-units` | S12, S13 | `tests/organization.test.ts`<br>`tests/social-beta.test.ts`<br>`tests/e2e/demo-organization-lifecycle.spec.ts` | Pass (integration + browser) |
| `organization/feature-node-lifecycle` | S12, S13 | `tests/organization.test.ts`<br>`tests/social-beta.test.ts`<br>`tests/e2e/demo-organization-lifecycle.spec.ts` | Pass (integration + browser) |
| `organization/organization-tree` | S12, S13 | `tests/organization.test.ts`<br>`tests/social-beta.test.ts`<br>`tests/e2e/demo-organization-lifecycle.spec.ts` | Pass (integration + browser) |
| `organization/sharing` | S12, S13 | `tests/organization.test.ts`<br>`tests/social-beta.test.ts`<br>`tests/e2e/demo-organization-lifecycle.spec.ts` | Pass (integration + browser) |
| `people_payroll/employees` | S12, S13 | `tests/people-lifecycle.test.ts` | Pass (integration) |
| `people_payroll/feature-expenses` | S12, S13 | `tests/people-lifecycle.test.ts` | Pass (integration) |
| `people_payroll/feature-leave-requests` | S12, S13 | `tests/people-lifecycle.test.ts` | Pass (integration) |
| `people_payroll/feature-payroll-runs` | S12, S13 | `tests/e2e/demo-payroll-runs.spec.ts`<br>`tests/payroll-core.test.ts` | Pass (integration + browser) |
| `people_payroll/payroll-summaries` | S12, S13 | `tests/e2e/demo-payroll-runs.spec.ts`<br>`tests/payroll-core.test.ts` | Pass (integration + browser) |
| `planning/feature-capacity` | S03, S11 | `tests/production.test.ts` | Pass (integration) |
| `planning/feature-plan-comparison` | S03, S11 | `tests/production.test.ts` | Pass (integration) |
| `planning/recommendations` | S03, S11 | `tests/production.test.ts` | Pass (integration) |
| `planning/scenarios` | S03, S11 | `tests/production.test.ts` | Pass (integration) |
| `planning/shortages` | S03, S11 | `tests/production.test.ts` | Pass (integration) |
| `platform_administration/feature-backup-recovery` | S15, S19 | `tests/backup-restore-operations.test.ts` | Pass (integration) |
| `platform_administration/feature-connectors` | S15, S19 | `tests/platform-operations.test.ts` | Pass (integration) |
| `platform_administration/feature-holdings` | S15, S19 | `tests/platform-operations.test.ts` | Pass (integration) |
| `platform_administration/feature-infrastructure` | S15, S19 | `tests/recovery-simulation.test.ts` | Pass (integration) |
| `platform_administration/feature-migration-runs` | S15, S19 | `tests/platform-operations.test.ts` | Pass (integration) |
| `platform_administration/feature-platform-audit` | S15, S19 | `tests/platform-operations.test.ts` | Pass (integration) |
| `platform_administration/feature-projections-events` | S15, S19 | `tests/platform-operations.test.ts` | Pass (integration) |
| `platform_administration/feature-temporary-access` | S15, S19 | `tests/iam.test.ts` | Pass (integration) |
| `platform_administration/provisioning` | S15, S19 | `tests/platform-operations.test.ts` | Pass (integration) |
| `pms/cashier` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/dashboard` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/folios` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/front-desk` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/guests` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/housekeeping` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/inventory` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/night-audit` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/reports` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/reservations` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/rooms` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pms/settings` | S07, S08 | `tests/pms.test.ts` | Pass (integration) |
| `pos/feature-payments-receipts` | S05, S06, S08 | `tests/pos.test.ts` | Pass (integration) |
| `pos/feature-register-session` | S05, S06, S08 | `tests/pos.test.ts` | Pass (integration) |
| `pos/feature-sales-baskets` | S05, S06, S08 | `tests/pos.test.ts` | Pass (integration) |
| `pos/feature-voids` | S05, S06, S08 | `tests/pos.test.ts` | Pass (integration) |
| `pos/refunds` | S05, S06, S08 | `tests/pos.test.ts` | Pass (integration) |
| `procurement/feature-invoice-match` | S03, S12 | `tests/purchase-to-payment.test.ts` | Pass (integration) |
| `procurement/feature-supplier-history` | S03, S12 | `tests/supplier-promises.test.ts` | Pass (integration) |
| `procurement/goods-receipts` | S03, S12 | `tests/purchase-to-payment.test.ts` | Pass (integration) |
| `procurement/purchase-orders` | S03, S12 | `tests/purchase-to-payment.test.ts` | Pass (integration) |
| `procurement/requisitions` | S03, S12 | `tests/employee-services.test.ts` | Pass (integration) |
| `projects/dependencies` | S09 | `tests/staff-work.test.ts` | Pass (integration) |
| `projects/feature-attachments` | S09 | `tests/staff-work.test.ts` | Pass (integration) |
| `projects/feature-handoffs` | S09 | `tests/staff-work.test.ts` | Pass (integration) |
| `projects/feature-project-graph` | S09 | `tests/staff-work.test.ts` | Pass (integration) |
| `projects/feature-projects-wbs` | S09 | `tests/staff-work.test.ts` | Pass (integration) |
| `projects/feature-time-costs` | S09 | `tests/staff-work.test.ts` | Pass (integration) |
| `quality/feature-cancellation` | S03, S04, S11 | `tests/quality.test.ts` | Pass (integration) |
| `quality/inspections` | S03, S04, S11 | `tests/quality.test.ts` | Pass (integration) |
| `quality/release-gates` | S03, S04, S11 | `tests/quality.test.ts` | Pass (integration) |
| `quality/results` | S03, S04, S11 | `tests/quality.test.ts` | Pass (integration) |
| `sales/feature-cancellations` | S01, S02, S04 | `tests/flow-routing.test.ts` | Pass (integration) |
| `sales/feature-revenue-handoff` | S01, S02, S04 | `tests/commerce-returns.test.ts` | Pass (integration) |
| `sales/fulfillment` | S01, S02, S04 | `tests/business-commands.test.ts` | Pass (integration) |
| `sales/orders` | S01, S02, S04 | `tests/flow-routing.test.ts` | Pass (integration) |
| `sales/quotes` | S01, S02, S04 | `tests/business-commands.test.ts` | Pass (integration) |
| `support/feature-attachments` | S04, S10, S15 | `tests/support-evidence.test.ts` | Pass (integration) |
| `support/feature-capabilities` | S04, S10, S15 | `tests/support-sessions.test.ts` | Pass (integration) |
| `support/feature-support-sessions` | S04, S10, S15 | `tests/support-sessions.test.ts` | Pass (integration) |
| `support/incidents` | S04, S10, S15 | `tests/support-incidents.test.ts` | Pass (integration) |
| `support/sla-watch` | S04, S10, S15 | `tests/support-sla.test.ts` | Pass (integration) |
| `support/tickets` | S04, S10, S15 | `tests/support.test.ts` | Pass (integration) |
| `talent/applications` | S17 | `tests/talent.test.ts` | Pass (integration) |
| `talent/feature-candidates-privacy` | S17 | `tests/talent.test.ts` | Pass (integration) |
| `talent/feature-hire-handoffs` | S17 | `tests/talent.test.ts` | Pass (integration) |
| `talent/feature-proposals` | S17 | `tests/talent.test.ts` | Pass (integration) |
| `talent/offers` | S17 | `tests/talent.test.ts` | Pass (integration) |
| `talent/requisitions` | S17 | `tests/talent.test.ts` | Pass (integration) |

## Evidence files

- [Integration suite log](2026-09-29-187-capability-integration-tests.txt)
- [Browser suite log](2026-09-29-187-capability-browser-tests.txt)
- Browser run was performed against an isolated simulation, then the exact simulation run and its disposable data were destroyed.

## Limits

- Test inventory comes from each capability’s `implementation.test` field in `docs/status/business-acceptance-manifest.json`; semicolon/comma-separated runnable test references were expanded and checked for existence.
- Several capabilities share a test file. Suite success confirms the tests in the mapped files ran and passed; it does not prove every distinct capability acceptance clause is covered.
- External provider, live production, security-owner review, and business-owner acceptance remain separate gates and need their designated evidence/reviewer.
