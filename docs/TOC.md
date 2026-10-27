# ERPX documentation index

Reviewed 2026-10-27. This is the single entry point. The [production plan](production-plan.md) owns current delivery scope and remaining work; [owner contracts](contracts/next/README.md) define behavior. Dated evidence certifies only the source, environment and scope it actually inspected.

## Current system

- [Active Rust/Next.js delivery plan](production-plan.md), [owner migration register](status/rust-backend-migration.json), [architecture decision](adr/ADR-052-rust-backend-and-domain-aligned-frontend.md) and [PDF policy exception](adr/ADR-053-explicit-pdf-seed-key-usage-prohibitions.md).
- [Native Files original-read migration contract](contracts/next/rust-files-migration.md).
- [Native Commercial purchase migration contract](contracts/next/rust-commercial-migration.md).
- [Native scheduled Legal release evidence](contracts/next/rust-release-readiness.md) and [Legal processing/polling boundary](contracts/next/rust-legal-processing.md).
- [Native Workflow command ledger and receipt compatibility](contracts/next/rust-workflow-migration.md).
- [Native private business HTTP component contract](contracts/next/rust-business-http.md) and [development session/SSR boundary](contracts/next/rust-private-session-api.md).
- [Native Finance journal detail component contract](contracts/next/rust-finance-journal-detail.md).
- [Native Maintenance Work component](contracts/next/rust-maintenance-migration.md).
- [Native Field Service reserve/issue parts component](contracts/next/rust-field-service-migration.md).
- [Native selected Identity ingress](contracts/next/rust-selected-ingress.md).
- [Native Maintenance notification and Inbox component](contracts/next/rust-maintenance-notifications.md).
- [Native Finance owned parts Draft component](contracts/next/rust-finance-parts-draft.md).
- [Native Directory scope and expiring credential decision](adr/ADR-055-native-directory-scope-and-credential-witness.md).
- [Durable Directory observation provenance](adr/ADR-056-durable-directory-observation-provenance.md).
- [Native Inventory valuation and consumption planning](contracts/next/rust-inventory-migration.md).

- [Open-source preparation and provenance](community/open-source.md), [contributing](../CONTRIBUTING.md), [governance](../GOVERNANCE.md), [security reporting](../SECURITY.md), [licensing scope](../LICENSING.md).


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
- [Development Company package Preview/Apply and rollback](../ops/macos/commercial-development-migration.md)

- [Native Rust runtime and retained request leases](contracts/next/rust-runtime.md), [native Account credential migration and remaining parity](contracts/next/rust-account-migration.md)
- [Hospitality ownership and Guest Accounts](adr/ADR-040-hospitality-module-ownership-and-guest-accounts.md) · [development verification](testing/hospitality-boundaries-2026-10-08.md)
- [Canonical receipt storage](adr/ADR-042-canonical-command-receipt-partitions.md)
- [Command guard transaction scope](adr/ADR-041-transaction-local-command-assertions.md)
- [Service no-charge confirmation](adr/ADR-046-service-no-charge-confirmation.md)
- [Source-bound worker preparation foundation](adr/ADR-047-source-bound-worker-preparation-foundation.md)
- [Ticket-bound Support Portal](adr/ADR-048-ticket-bound-support-portal.md)
- [Product app workspaces and development verification](adr/ADR-049-product-app-workspaces-and-development-verification.md)


ERPX has15 products,21 active application workspaces and33 modules. `apps/compat` is retained non-product compatibility source. Development has three exact Holding hosts plus central Account; application identities are distinct from public host registrations.

- [Start and verify](../README.md), [developer instructions](../AGENTS.md), [local fictional seed](local-seed.md)
- [Holding addresses and workspace paths](contracts/next/holding-addresses.md), [ADR-051](adr/ADR-051-holding-addresses-and-workspace-paths.md), [development operations](../ops/macos/holding-development.md), [live infrastructure receipt](../ops/macos/holding-development-live.json)
- [Independent apps, packages and SSO](contracts/next/product-separation.md), [generated product/host manifest](contracts/next/product-registry.generated.json), [Apps Feed and retired Work decision](adr/ADR-050-apps-feed-and-work-retirement.md)
- [Current owner contracts](contracts/next/README.md), [33-module workflow catalogue](contracts/next/catalogue.md), [route reference](contracts/next/route-table.md), [service boundaries](contracts/next/service-boundaries.md)
- [People relationships and independent account administration](contracts/next/people-external-access.md), [Skills, Performance & Compensation](contracts/next/people-evaluation.md)
- [Executive MCP and agent connections](contracts/next/executive-mcp.md), [record drawers](contracts/next/record-drawers.md), [generated drawer inventory](contracts/next/record-drawer-inventory.json)
- [Business glossary](../CONTEXT.md), [architecture decisions](adr/README.md), [contract compatibility policy](../CONTRACT_POLICY.md)
- [Operations](../ops/README.md), [private production runtime](../ops/macos/README.md), [incident handling](../ops/runbooks/nextjs-incident.md), [recovery](../ops/runbooks/nextjs-recovery.md)
- [UI components](ux-ui/component-inventory.md), [UI standardization status](ux-ui/ui-standardization.md), [product brand guide](brand/ERPX-brand-guide.md), [runtime brand assets](../public/brand/)

Normal development entry uses [Acme](https://acme.jesadakorn.com/), [Northstar](https://northstar.jesadakorn.com/) or [ERPX Internal](https://erpx-internal.jesadakorn.com/), with central [Account](https://account-dev-erpx.jesadakorn.com/). Former app hosts return generic404; Console retains exact health/ready reads only. The old product-host rollout runbook is superseded for current routing.

## Verification and acceptance

- [Product migration coverage](status/product-migration-acceptance.json), [v2 receipt index](status/product-migration-receipts-v2.json), [dated v1 history](status/product-migration-receipts.json)
- [Verification commands and evidence rules](testing/README.md), [E2E specification](testing/e2e-specification.md), [isolated browser fixtures](testing/isolated-browser-simulation.md), [working demo](testing/working-demo-company.md), [generated route inventory](testing/uat-route-inventory.md)
- [Engineering scenarios](../tests/catalogue/engineering-scenarios.json), [gate scenarios](../tests/catalogue/engineering-gate-scenarios.json), [Legal engineering UAT](../tests/catalogue/legal-uat-engineering-cases.json), [attachment owner cases](../tests/catalogue/engineering-attachment-owner-cases.json)
- [Independent engineering/provider/release inventory](status/capability-delivery-matrix.md), [Company release criteria](contracts/next/business-readiness.md), [explicit gate reviews](status/capability-gate-reviews.json), [generated acceptance manifest](status/business-acceptance-manifest.json)
- [Checksum evidence ledger](status/business-acceptance-evidence.json), [durable dated evidence](testing/evidence), [2026-10-03 engineering baseline](testing/evidence/2026-10-03-engineering-completion.json), [Finance expansion](testing/evidence/2026-10-05-finance-expansion.md)
- [Company package Preview/Apply and rollback](../ops/macos/commercial-development-migration.md), [investor capture](testing/investor-capture.md)

Development verification is not full acceptance. Full acceptance is reserved for intentional production preparation on a frozen candidate and is deferred during this migration. Old counts, gaps, hostnames and successful runs in `testing/` or dated research are observations of that original candidate; they are not current delivery instructions or provider/Company approvals.

Dated source reviews remain available for [Thai payroll](research/thai-payroll-rules-2026-10-02.md), [Thai Finance tax](research/thai-finance-tax-rules-2026-10-03.md), [Thai accounting](research/thai-accounting-research-2026-09-30.md) and [NPAEs reporting](testing/npaes-reporting-2026-10-03.md). Cleanup does not renew their effective dates or statutory verification.

## Maintenance and recovery

Update the owning contract and active production plan when current facts change. Generate capability/status artifacts with `bun run status:capabilities` and product coverage with `bun run products:manifest`; never edit generated counts or infer gate reviews from observations. Run `bun run docs:check`, `bun run products:check`, `bun run structure:check`, `bun run test:catalogue` and `bun run architecture:check` for repository integrity.

`docs:check` checks root Markdown, current documentation and the archive navigation index; it preserves immutable evidence/link topology and verifies checksum-ledger files. Existing tracked files under ignored `.scratch/` or `.tmp/` are rejected. Keep snapshots, private credentials, logs and temporary plans outside Git. Domain/issue-tracker templates and per-app generated Next.js instructions are intentional copies, not duplicate cleanup targets.

Superseded plans, obsolete review prose and completed documentation-triage tickets are recoverable through [verified Git references](../README.md#recovery). [Archived Rust evidence](archive/README.md) retains its original bytes and dated scope. The [2026-10-13 relocation receipt](testing/repository-file-organization-2026-10-13.md) and [dated workplace journey inventory](journeys/2026-09-26-workplace-journey-inventory.md) remain historical evidence, not current source maps.

Staging requires an explicit staging request; normal development verification does not authorize its hosts, services, databases or deployment directories.

### PMS design, vendor comparison and 2026-10-09 implementation

- [Official research and TCO inputs](research/pms-market-research-2026-10-09.md), [feature ownership register](testing/pms-feature-register-2026-10-09.md)
- [Dated implementation checks](testing/pms-implementation-2026-10-09.md), [mock-pilot inputs](testing/pms-mock-pilot-2026-10-09.md), [67-capability review](testing/pms-feature-review-2026-10-11.md)

Current remaining work and the limits of the completed mock engineering scope are consolidated in the [production plan](production-plan.md#recorded-milestones-and-deferred-work).
