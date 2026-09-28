# Shared workspace and employee services

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


## Entry points

| Person | Daily entry point |
| --- | --- |
| Every active company member, including beta | **Work → New → Service request, Equipment request, Purchase request**, alongside existing creation options |
| Requester | **Work → My requests**; the record contains progress, next action, conversation, files and completion confirmation |
| Assigned handler or team lead | **Work → Assigned to me / Team queue**; combined responsibilities do not require changing the selected role |
| Independent department reviewer | **Work → Approvals** |
| Company owner | **Administration → Organization → Service teams**; routing repairs under **Approvals & routines** |
| Handbook reader | **More → Company handbook** |
| Owner-appointed handbook editor | Company handbook → **Drafts & editing** / **New article** |

Home remains the English team feed. Its next three actions come from the same aggregated source as Work, including requests, expenses, approvals and payments. A failed source is identified separately and does not hide other work. Workspace selection still controls company, branch and department; an optional responsibility filter changes the view, not command authority. Platform remains separate.

## Three review batches

1. **Responsibilities and shared work.** `lib/responsibilities.ts`, business command adapters, authorization/navigation, Work summaries and filters, shared Home priorities, and account/workspace menus. Each business command chooses a complete applicable grant; it cannot combine a role from one scope with access from another. Command commits now check the grant's company, scope and effective dates as well as revocation.
2. **Teams and employee services.** Migrations 0039 and 0041, service access/coverage/history, canonical request state machine, request API and UI, attachment access, Inbox links, and Administration routing repair. Shared request infrastructure includes the resource-request states used by the next batch.
3. **Resources, handbook and preparation.** Purchase-order handoff, equipment handover, handbook publication/history, migration 0040 and request AI preparation through the existing company provider/outbox service. Integration fixtures, UI journeys and compatibility tests cover the combined release.

These are review groups within one release. Apply the additive migrations before deploying the application and jobs Worker together; the final integration gate validates their combined interfaces. Keep the existing expense, leave, task, accounting, inventory and purchasing histories.

## Routing and authority

- The ten service categories are templates. No staff assignment or business permission is created until an owner explicitly configures a team. A team can cover several services and locations, and a person can belong to several teams.
- Coverage selects department, then branch, then company. An unavailable configured local team produces a routing issue instead of silently diverting work. Disabled coverage is excluded. Open requests retain their saved team until an explicit, reasoned reassignment.
- Service submission needs a title, description and authorized location, but no employee profile. Missing coverage retains the submitted request as blocked. Drafts can remain incomplete.
- Equipment and purchase submission require the claimant's active staff profile and organizational approval data. Equipment also requires a needed date. Purchase estimates and needed date can remain unknown.
- Resource approval reuses the expense department policy: greatest eligible active rank, tied independent candidates, reporting-superior fallback, then independent owner. Claimant and preparer are excluded. Resource requests do not run through Accounts or payment stages.
- Leads assign current authorized team members. Team membership alone grants no financial, AI-administration, billing or role-management authority. Eligibility, coverage, routing inputs and request versions are rechecked in the committing transaction.
- Service work progresses through queued, in progress, waiting for requester, resolved and closed. Requesters explicitly confirm or reopen; there is no automatic closure. Questions, replies and messages remain in the same record. Reassignment, decisions, handovers and configuration changes retain history.
- Spending from a service job creates a linked resource request on behalf of the original requester. Closing the service job cannot approve it.
- Equipment handover is evidence, not an inventory or accounting command. Purchase orders use the existing Purchasing authority and independent order approval/receipt commands. Creating and linking an order commits atomically; retries return the same order. Fulfillment waits for complete receipt, including after partial receipts, and checks the order version again at commit.
- Beta supports requests, teams, handbook and preparation. ERP purchase execution remains explicitly unavailable in beta.

## Handbook and AI

Handbook content is company scoped. Only owners appoint or revoke editors. Saves create immutable draft revisions; publication explicitly selects a saved revision. Employee search, direct URLs and revision links expose only published content. Editors can inspect prior drafts, and readers can inspect previous published revisions. Archived articles are unavailable to readers.

Request AI prepares wording, category suggestions and a summary from the saved request text. It uses the shared company text-provider selection and quota ledger; it does not read facts from attachments or invent their contents. Suggestions cite the saved request version. A human explicitly applies individual suggested fields. Routing, approval, handover, payment and publication remain human actions governed by deterministic policy.

Preparation is queued through the existing outbox/jobs Worker, bound to company, request version and provider revision. Uploads change the evidence/request version. Access changes, edits or provider changes invalidate results. Worker leases and bounded retries prevent duplicate execution; failures retain the request and allow manual completion or retry. Provider credentials stay in the existing encrypted provider service, outside request proposals and browser responses. No live AI connection is required to use employee services.

Browser recovery preserves unsaved request, decision, team and handbook fields through refresh and Back. A concurrent server edit requires review. A successful save or a status-only AI refresh does not falsely create a conflict.

## Local rollout and verification

Use the pinned Bun/Next/OpenNext toolchain. Migrations 0039–0041 are additive and applied to both local holding databases; no remote deployment is part of this change.

```sh
bun run db:migrate:local
bun scripts/seed-services-e2e.ts
bun scripts/seed-services-e2e.ts --beta
bun run lint
bun run build:cloudflare
# Run integration only after the Cloudflare build completes.
bun run test:integration
# Start the already-built local Cloudflare preview on a free port 8787, then:
ERPX_E2E_BASE_URL=http://localhost:8787 node node_modules/@playwright/test/cli.js test tests/e2e/shared-workspace.spec.ts tests/e2e/expense-v2.spec.ts
ERPX_E2E_BASE_URL=http://localhost:8787 ERPX_SERVICES_FIXTURE=.scratch/shared-workspace/beta-fixture.json node node_modules/@playwright/test/cli.js test tests/e2e/shared-workspace.spec.ts
```

`seed-services-e2e.ts` creates a separate local company and synthetic accounts; it never alters existing users. The fixture, logs and screenshots are under `.scratch/shared-workspace/`. `tests/employee-services.test.ts`, `tests/request-ai.test.ts`, and `tests/work-sources.test.ts` cover domain, authority, recovery and source-isolation behavior. Existing navigation/account tests use combined responsibilities instead of obsolete role-switching controls.

Verified locally on 2026-09-08:

| Gate | Result | Evidence under `.scratch/shared-workspace/` |
| --- | --- | --- |
| ESLint | Passed without warnings | `lint-release.log` |
| TypeScript and Next.js production build | Passed, including the build run by OpenNext | `types-worker.log`, `build-cloudflare-release.log` |
| Cloudflare application build | Passed | `build-cloudflare-release.log` |
| ERP and beta jobs Worker bundles | Both dry-run builds passed; no deployment | `jobs-build.log`, `beta-jobs-build.log` |
| Migration and integration suite | 18 files, 130 tests passed **after** the final Cloudflare build | `integration-final.log` |
| Core ERP browser checks | All 48 applicable checks passed across the main run and targeted corrections | `e2e-cloudflare.log`, `e2e-final.log` |
| Social post → task → evidence → independent acceptance, with AI failure | Desktop and mobile passed | `e2e-final.log` |
| Employee services in an isolated beta company | All 12 desktop/mobile checks passed across the beta run and targeted corrections | `e2e-beta.log`, `e2e-beta-final.log` |
| Layout and keyboard | 390, 768 and 1440px, light/dark; no horizontal overflow; mobile form controls at least 44px; New dialog focus return passed | `screenshots/`, browser logs |

The browser runs cover 62 distinct executed checks across ERP, social and beta. Four duplicate mobile cases intentionally defer to desktop tests that already cover the relevant route or full viewport matrix. Initial browser failures came from test setup: retaining the owner account while asserting HR permissions, and skipping a required evidence field before streamed content appeared. The corrected tests explicitly establish the account and wait for the field; targeted reruns passed without weakening application checks. Existing expense payment assertions now wait for the payment command response before checking the refreshed view.

The beta fixture is a separate company with its own beta policy and members. It verifies that service/equipment journeys and handbook editing work while approved purchase requests remain saved and direct ERP purchasing commands return 403. Live AI credentials were not configured for this verification. AI integration uses a synthetic provider, and UI tests verify manual completion with AI unavailable; real provider availability remains deployment configuration.

Three review patches are available in `.scratch/shared-workspace/review/`: `01-responsibilities.patch`, `02-teams-and-requests.patch`, and `03-resources-handbook-and-ai.patch`. `files.json` lists the files in each group. Applying all three to the pre-change source snapshot was checked and reproduces the current source. These preserve the work already present before this plan; they are review artifacts rather than commits or separate intermediate releases.

## Recovery

Keep the additive tables and evidence when reverting application code. Do not drop request, handover or handbook history as a UI rollback. Restore the application and jobs Worker to the same prior release, or deploy a forward fix after reviewing migration compatibility. Local pre-change source and D1 snapshots are retained outside Git; they are development recovery artifacts, not a production backup strategy.
