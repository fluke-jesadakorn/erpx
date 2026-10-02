# ERPX documentation index

[Current production plan](production-plan.md) is the sole active delivery plan for the Thai-first Today workplace across all business groups, released one accepted flow at a time. The earlier hotel and restaurant track remains a separate business acceptance track inside that plan. Read it for scope, owners, criteria, evidence and blockers; its ten-working-day checkpoint is not a launch date. The [workplace journey inventory](journeys/2026-09-26-workplace-journey-inventory.md) maps current routes to owners, handoffs and remaining gaps.

## Current behavior and decisions

- [Contracts](contracts/next/README.md), including [attachments](contracts/next/attachments.md), [release acceptance](contracts/next/business-readiness.md), [PMS](contracts/next/pms.md), [POS](contracts/next/pos.md), [Finance](contracts/next/finance.md), [Payroll](contracts/next/payroll-core.md)
- [Business glossary](../CONTEXT.md), [architecture decisions](adr/README.md), [contract policy](../CONTRACT_POLICY.md)
- [Runtime / operations](../ops/macos/README.md), [local seed](local-seed.md), [developer instructions](../AGENTS.md)

## Evidence and verification

Evidence establishes only the behavior and environment stated in each record. Historical reports do not promote release gates or expand the current scope.

- [Capability inventory](status/capability-delivery-matrix.md), [generated manifest](status/business-acceptance-manifest.json), [review source](status/capability-gate-reviews.json)
- [Checksum evidence ledger](status/business-acceptance-evidence.json); durable files stay in `testing/evidence/`
- [E2E cases](testing/e2e-specification.md), [isolated browser simulation](testing/isolated-browser-simulation.md), [working demo](testing/working-demo-company.md)
- [Backend/API performance research](testing/backend-performance-2026-09-30.md); the proposed refactor sequence remains in the [current production plan](production-plan.md#backendapi-architecture-and-development-performance--2026-09-30).
- [Thai accounting official research](testing/thai-accounting-research-2026-09-30.md), [development foundation evidence](testing/evidence/2026-09-30-thai-accounting-foundation.md)
- [VAT correction lifecycle — 2026-09-30](testing/evidence/2026-09-30-vat-correction-lifecycle.md); delivery remains in the [current production plan](production-plan.md#บัญชีไทยตั้งแต่เอกสารถึงปิดปี--2026-09-30).
- [Generated page-route inventory](testing/uat-route-inventory.md) lists source routes and their acceptance boundary; it does not prove user journeys passed.
- [Recovery simulation](testing/recovery-simulation.md), [stock reservations](testing/shared-stock-reservation-simulation.md), [routing review](testing/routing-attention-simulation.md)
- [Supplier payment](testing/supplier-payment-simulation.md), [RFQ](testing/procurement-rfq-simulation.md), [supplier promises](testing/supplier-delivery-promises.md)
- [Returns/refunds](testing/return-refund-simulation.md), [inventory transit](testing/inventory-transit-simulation.md), [quality holds](testing/quality-hold-simulation.md)
- [Hospitality implementation report](testing/hospitality-pilot-implementation-2026-09-23.md), [rollout report](testing/hospitality-feature-rollout-2026-09-23.md), [first-time setup verification](testing/evidence/2026-09-30-hospitality-setup.md)
- [Cross-department scenarios](testing/cross-department-business-scenarios.md), [historical progress/evidence](testing/cross-department-implementation-progress.md)

## History and broader product references

- [Archived September plans and requirement retention](archive/planning-2026-09/README.md)
- [Domain maps](plan/README.md), [historical traceability](plan/traceability.md), [archive recovery](plan/ARCHIVE.md), [cutover](../CUTOVER.md)
- [Mockup](../Designed/mockup/README.md) is a design reference, not the current application or a completeness claim.

Reports elsewhere under `testing/` retain their date-specific findings, including modules deferred from the hotel release. Do not create a competing daily plan; update `production-plan.md` and the affected contract/evidence only.
