# ERPX documentation index

Reviewed 2026-10-03. This is the single entry point. [Production plan](production-plan.md) owns delivery scope; contracts own current behavior; recorded evidence identifies the source, environment and date it actually verified.

## Current system

- [Start and verify](../README.md), [developer instructions](../AGENTS.md), [local development seed](local-seed.md)
- [Current owner contracts](contracts/next/README.md), [33-module catalogue workflows](contracts/next/catalogue.md), [route reference](contracts/next/route-table.md)
- [Executive remote MCP and agent connection guide](contracts/next/executive-mcp.md)
- [Record detail drawers](contracts/next/record-drawers.md), [record/table coverage](contracts/next/record-drawer-inventory.json)
- [Business glossary](../CONTEXT.md), [architecture decisions](adr/README.md), [contract compatibility policy](../CONTRACT_POLICY.md)
- [Operations](../ops/README.md), [private production runtime](../ops/macos/README.md), [incident](../ops/runbooks/nextjs-incident.md), [recovery](../ops/runbooks/nextjs-recovery.md)
- [Workplace routes and handoffs](journeys/2026-09-26-workplace-journey-inventory.md), [UI components](ux-ui/component-inventory.md), [UI migration evidence](ux-ui/ui-migration-2026-10-02.md), [UI standardization status and coverage](ux-ui/ui-standardization.md)
- [Product brand guide](brand/ERPX-brand-guide.md), [design references](../Designed/README.md)

## Verification and acceptance

- [Independent engineering/provider/release inventory](status/capability-delivery-matrix.md)
- [Dated engineering completion receipt — 2026-10-03](testing/evidence/2026-10-03-engineering-completion.json): recorded 33/33 modules, 187/187 capabilities and 12/12 engineering gates; scoped to its original source/candidate
- [Finance expansion evidence — 2026-10-05](testing/evidence/2026-10-05-finance-expansion.md): completed scoped FE-01..07 engineering acceptance, native desktop/mobile proof and independent accounting goldens; separate from the 187-capability baseline; [current check receipt](testing/evidence/2026-10-05-finance-expansion-checkpoint.json)
- [Engineering scenario register](testing/engineering-scenarios.json), [required gate scenarios](testing/engineering-gate-scenarios.json), [Legal engineering UAT](testing/legal-uat-engineering-cases.json), [attachment owner cases](testing/engineering-attachment-owner-cases.json)
- [Company release acceptance](contracts/next/business-readiness.md), [explicit inventory reviews](status/capability-gate-reviews.json), [generated inventory manifest](status/business-acceptance-manifest.json)
- [Checksum evidence ledger](status/business-acceptance-evidence.json), [verification guide](testing/README.md), [durable evidence](testing/evidence/)
- [E2E specification](testing/e2e-specification.md), [isolated browser simulation](testing/isolated-browser-simulation.md), [working demo](testing/working-demo-company.md), [generated source-route inventory](testing/uat-route-inventory.md)
- Dated official-source reviews: [Thai payroll](testing/thai-payroll-rules-2026-10-02.md), [Thai Finance tax](testing/thai-finance-tax-rules-2026-10-03.md), [Thai accounting](testing/thai-accounting-research-2026-09-30.md), [NPAEs reporting](testing/npaes-reporting-2026-10-03.md)

Engineering results do not certify a provider, Company release, statutory filing, bank/device or production operation. A later documentation or generator revision does not restamp the earlier engineering run. Reports under `testing/` retain dated findings; their old gaps and hostname observations are not current delivery instructions.

## Maintenance and recovery

Edit the owner contract and the production plan when their facts change. Regenerate catalogue/status artifacts with `bun run status:capabilities`; verify with `bun scripts/generate-capability-matrix.ts --check`. Run `bun run docs:check` for links, anchors and source references. Do not hand-edit generated artifacts or infer gate reviews from observations.

Superseded plans, duplicated Rust maps and completed documentation-triage tickets were removed after consolidating surviving ownership/control requirements into the catalogue and owner contracts. [Cutover and verified Git recovery references](../CUTOVER.md) retain their recovery boundary. [Archived immutable Rust evidence](archive/README.md) remains date-scoped; it does not describe the current stack.

Staging operations are documented in the [staging runbook](../ops/macos/staging.md) and require an explicit staging request. Normal development verification uses [the development console](https://console-dev-erpx.jesadakorn.com/).

- [People evaluation: Skills, Performance & Compensation](contracts/next/people-evaluation.md) — annual evidence, scoped comparisons and reviewed compensation.
