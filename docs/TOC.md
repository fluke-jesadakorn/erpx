# ERPX documentation index

Current address contract: [Holding → workspace → app](contracts/next/holding-addresses.md), [ADR-051](adr/ADR-051-holding-addresses-and-workspace-paths.md), [development runbook](../ops/macos/holding-development.md) and [live infrastructure receipt](../ops/macos/holding-development-live.json).

Reviewed 2026-10-23. This is the single entry point. [Production plan](production-plan.md) owns delivery scope; contracts own current behavior; recorded evidence identifies the source, environment and date it actually verified.

## Current system

- [Active Rust/Next.js delivery plan](production-plan.md), [owner migration register](status/rust-backend-migration.json), [architecture decision](adr/ADR-052-rust-backend-and-domain-aligned-frontend.md) and [PDF policy exception](adr/ADR-053-explicit-pdf-seed-key-usage-prohibitions.md).
- [Native Files original-read migration contract](contracts/next/rust-files-migration.md).
- [Native Commercial purchase migration contract](contracts/next/rust-commercial-migration.md).
- [Native scheduled Legal release evidence](contracts/next/rust-release-readiness.md) and [Legal processing/polling boundary](contracts/next/rust-legal-processing.md).
- [Native Workflow command ledger and receipt compatibility](contracts/next/rust-workflow-migration.md).
- [Native private business HTTP component contract](contracts/next/rust-business-http.md) and [development session/SSR boundary](contracts/next/rust-private-session-api.md).
- [Native Finance journal detail component contract](contracts/next/rust-finance-journal-detail.md).
- [Native Maintenance Work component](contracts/next/rust-maintenance-migration.md).
- [Native Directory scope and expiring credential decision](adr/ADR-055-native-directory-scope-and-credential-witness.md).
- [Native Inventory valuation and consumption planning](contracts/next/rust-inventory-migration.md).

- [Open-source preparation and provenance](community/open-source.md), [contributing](../CONTRIBUTING.md), [governance](../GOVERNANCE.md), [security reporting](../SECURITY.md), [licensing scope](../LICENSING.md).

- [Apps Feed and Work retirement](adr/ADR-050-apps-feed-and-work-retirement.md).

- [Product separation: registry, packages and SSO implementation boundary](contracts/next/product-separation.md), [21-host desired-state manifest](contracts/next/product-registry.generated.json), [development rollout/rollback](../ops/macos/product-development.md)
- [Development Commercial permission-evidence checkpoint](testing/commercial-compatibility-2026-10-14.md): exact-source native/build/browser evidence; implementation-only Admin receipts and remaining acceptance
- [Development product navigation checkpoint](testing/product-navigation-2026-10-14.md): shared dirty confirmation, fifteen Company previews, exact-source full native lane and remaining migration gates
- [Development product-entry checkpoint](testing/product-entry-2026-10-15.md): per-role landings, explicit role selection, private server time preferences and current verification limits
- [Development Commercial temporal-source checkpoint](testing/commercial-temporal-2026-10-15.md): source-owned eligibility lease, actual late-receipt regressions and exact-candidate native/build/browser limits
- [Development Helpdesk obligation checkpoint](testing/support-obligations-2026-10-15.md): concrete owner settlement reader, first-snapshot confinement and exact-source native/build/browser limits
- [Development Commercial settlement and Apps history checkpoint](testing/commercial-apps-settlement-2026-10-15.md): final owner-source withdrawal guards, retained limited-purpose entry, exact-source full native/build and remaining migration gates
- [Development Service reference checkpoint](testing/service-references-2026-10-15.md): typed owner references, currency/location/technician choices and current drawer error-state repair; [receipt attribution correction](testing/service-reference-receipt-correction-2026-10-15.md)
- [Development Service intake checkpoint](testing/service-intake-2026-10-15.md): atomic Support case/Service order, installed 1048, current drawer repair and exact-source verification limits
- [Development package history and owner projection checkpoint](testing/commercial-history-owner-2026-10-15.md): retained history, CRM/metric source authority, installed 1047 and exact-source verification limits
- [Development Helpdesk surface purchase checkpoint](testing/support-surfaces-corrected-2026-10-15.md): owner controls/Work/files/jobs, Calendar/customer evidence, repaired GET alias and current native/build/browser limits
- [Development Helpdesk command purchase checkpoint](testing/support-commands-2026-10-15.md): transaction purchase guards, shadow/expiry/replay/lock order and current full-native/build limits
- [Development navigation boundary checkpoint](testing/ui-boundaries-2026-10-15.md): explicit caller appearance, preserved legacy scope, exact native viewport/theme/SSR proof and remaining frozen acceptance
- [Product migration acceptance register: 33 modules, owner workflows and seven shared/public surfaces](status/product-migration-acceptance.json), [active v2 current-source receipts](status/product-migration-receipts-v2.json), [dated v1 receipt history](status/product-migration-receipts.json)
- [Development Company package Preview/Apply and rollback](../ops/macos/commercial-development-migration.md)

- [Start and verify](../README.md), [developer instructions](../AGENTS.md), [local development seed](local-seed.md)
- [Current owner contracts](contracts/next/README.md), [33-module catalogue workflows](contracts/next/catalogue.md), [route reference](contracts/next/route-table.md)
- [Native Rust runtime and retained request leases](contracts/next/rust-runtime.md), [native Account credential migration and remaining parity](contracts/next/rust-account-migration.md)
- [Service boundaries and isolated verification](contracts/next/service-boundaries.md)
- [Executive remote MCP and agent connection guide](contracts/next/executive-mcp.md)
- [Record detail drawers](contracts/next/record-drawers.md), [record/table coverage](contracts/next/record-drawer-inventory.json)
- [Hospitality ownership and Guest Accounts](adr/ADR-040-hospitality-module-ownership-and-guest-accounts.md) · [development verification](testing/hospitality-boundaries-2026-10-08.md)
- [Canonical receipt storage](adr/ADR-042-canonical-command-receipt-partitions.md)
- [Command guard transaction scope](adr/ADR-041-transaction-local-command-assertions.md)
- [Service no-charge confirmation](adr/ADR-046-service-no-charge-confirmation.md)
- [Source-bound worker preparation foundation](adr/ADR-047-source-bound-worker-preparation-foundation.md)
- [Ticket-bound Support Portal](adr/ADR-048-ticket-bound-support-portal.md)
- [Product app workspaces and development verification](adr/ADR-049-product-app-workspaces-and-development-verification.md)
- [Business glossary](../CONTEXT.md), [architecture decisions](adr/README.md), [contract compatibility policy](../CONTRACT_POLICY.md)
- [Operations](../ops/README.md), [private production runtime](../ops/macos/README.md), [incident](../ops/runbooks/nextjs-incident.md), [recovery](../ops/runbooks/nextjs-recovery.md)
- [Workplace routes and handoffs](journeys/2026-09-26-workplace-journey-inventory.md), [UI components](ux-ui/component-inventory.md), [UI migration evidence](ux-ui/ui-migration-2026-10-02.md), [UI standardization status and coverage](ux-ui/ui-standardization.md)
- [Product brand guide](brand/ERPX-brand-guide.md), [runtime brand assets](../public/brand/)

## Verification and acceptance

- [People: relationships and independent account administration](contracts/next/people-external-access.md)

- [Repository file placement and relocation verification — 2026-10-13](testing/repository-file-organization-2026-10-13.md)
- [Required browser verification round and incomplete coverage — 2026-10-13](testing/repository-required-browser-verification-2026-10-13.md)
- [Attribution of the 70 browser failures — 2026-10-14](testing/repository-failure-attribution-2026-10-14.md)

- [Independent engineering/provider/release inventory](status/capability-delivery-matrix.md)
- [Dated engineering completion receipt — 2026-10-03](testing/evidence/2026-10-03-engineering-completion.json): recorded 33/33 modules, 187/187 capabilities and 12/12 engineering gates; scoped to its original source/candidate
- [Finance expansion evidence — 2026-10-05](testing/evidence/2026-10-05-finance-expansion.md): completed scoped FE-01..07 engineering acceptance, native desktop/mobile proof and independent accounting goldens; separate from the 187-capability baseline; [current check receipt](testing/evidence/2026-10-05-finance-expansion-checkpoint.json)
- [Engineering scenario register](../tests/catalogue/engineering-scenarios.json), [required gate scenarios](../tests/catalogue/engineering-gate-scenarios.json), [Legal engineering UAT](../tests/catalogue/legal-uat-engineering-cases.json), [attachment owner cases](../tests/catalogue/engineering-attachment-owner-cases.json)
- [Company release acceptance](contracts/next/business-readiness.md), [explicit inventory reviews](status/capability-gate-reviews.json), [generated inventory manifest](status/business-acceptance-manifest.json)
- [Checksum evidence ledger](status/business-acceptance-evidence.json), [verification guide](testing/README.md), [durable evidence](testing/evidence)
- [Home and menu streaming measurements — 2026-10-09](testing/home-menu-streaming-2026-10-09.json)
- [Investor capture and offline gallery](testing/investor-capture.md)
- [E2E specification](testing/e2e-specification.md), [isolated browser simulation](testing/isolated-browser-simulation.md), [working demo](testing/working-demo-company.md), [generated source-route inventory](testing/uat-route-inventory.md)
- Dated official-source reviews: [Thai payroll](research/thai-payroll-rules-2026-10-02.md), [Thai Finance tax](research/thai-finance-tax-rules-2026-10-03.md), [Thai accounting](research/thai-accounting-research-2026-09-30.md), [NPAEs reporting](testing/npaes-reporting-2026-10-03.md)

Engineering results do not certify a provider, Company release, statutory filing, bank/device or production operation. A later documentation or generator revision does not restamp the earlier engineering run. Reports under `testing/` retain dated findings; their old gaps and hostname observations are not current delivery instructions.

## Maintenance and recovery

Edit the owner contract and the production plan when their facts change. Regenerate catalogue/status artifacts with `bun run status:capabilities`; verify with `bun scripts/docs/generate-capability-matrix.ts --check`. Run `bun run docs:check` for links, anchors and source references. Do not hand-edit generated artifacts or infer gate reviews from observations.

Superseded plans, duplicated Rust maps and completed documentation-triage tickets were removed after consolidating surviving ownership/control requirements into the catalogue and owner contracts. [Cutover and verified Git recovery references](../README.md#recovery) retain their recovery boundary. [Archived immutable Rust evidence](archive/README.md) remains date-scoped; it does not describe the current stack.

Staging operations are documented in the [staging runbook](../ops/macos/staging.md) and require an explicit staging request. Normal development verification uses registered [Acme Holding](https://acme.jesadakorn.com/) and central Account.

- [People evaluation: Skills, Performance & Compensation](contracts/next/people-evaluation.md) — annual evidence, scoped comparisons and reviewed compensation.

### PMS design, vendor comparison and 2026-10-09 implementation

- [Official PMS research and three-year TCO inputs](research/pms-market-research-2026-10-09.md)
- [Per-feature ownership and acceptance register](testing/pms-feature-register-2026-10-09.md)
- [Implementation checks and remaining acceptance](testing/pms-implementation-2026-10-09.md)
- [Mock pilot inputs, reviewers and reconciliation checks](testing/pms-mock-pilot-2026-10-09.md)
- [67-capability mock engineering review register](testing/pms-feature-review-2026-10-11.md)

The active delivery checklist remains [production plan](production-plan.md).
