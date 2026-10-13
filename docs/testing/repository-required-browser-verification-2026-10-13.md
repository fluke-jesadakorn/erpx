# Required browser verification — 2026-10-13

The user ended this verification round after concurrent IAM/People changes invalidated the running source fingerprint. The results below identify the reorganization candidate that was actually tested; they do not certify the current working tree.

| Required cases | Passed | Failed | Skipped | Unverified |
| ---: | ---: | ---: | ---: | ---: |
| 2,701 | 1,430 | 70 | 0 | 1,201 |

The [case-level status register](repository-required-browser-verification-2026-10-13.json) preserves every required file/title/project identity. Unverified includes two cases whose test report could not be accepted because source changed during execution. Zero skipped applies to the retained observations; it does not establish results for the unverified cases.

## Candidate and scope

- Runtime fingerprint: `fc82d203d8ba9b94936fcf7af7a06b8ef9aadfe76189da3d5435030f87da3272`.
- Standalone candidate: `.runtime/releases/2026-10-13T03-22-24-757Z`; not deployed.
- Retained E2E/journey scope: 410 passed, 53 failed, 28 unverified out of 491 requirements.
- Retained UAT/fictional-provider scope: 1,020 passed, 17 failed, 1,173 unverified out of 2,210 requirements. UAT mobile requirements were not reached.
- Real providers were not selected. Tests used owned disposable cohorts and the pinned Bun 1.4.3 / Node 26.11.1 toolchain.

The user explicitly chose required cases once instead of forward/reverse execution. Earlier attempts include recorded retries. Later execution retained actual assertion failures and retried only pre-execution failures. Every accepted positive case has completed cleanup and an absent owned directory. Original receipts and immutable dated evidence were not rewritten. The detailed aggregate and raw receipts remain under ignored `.scratch/reorganization/required-round-closed.json` and `.scratch/dev/test-suites/browser/`.

## Reviewed register correction

With explicit user approval, two preferred-role desktop/mobile register titles were corrected to match the unchanged test file. All six preferred-role requirements passed after correction. Assertions and case counts did not change. The admission record separately preserves the old and corrected verification fingerprints and verifies the other 571 test/CI files were unchanged. Renamed identities required fresh execution.

## Findings and limits

Observed failures include a Next development overlay intercepting an action button, tests waiting for `details.beta-composer > summary`, `COMPANY_UNAVAILABLE` in journeys, and generic error pages for invalid-ID UAT routes. Existing page/component bodies and the pre-relocation test snapshots support specific pre-existing mismatches; this round did not execute a complete pre-relocation baseline and does not claim every failure is unrelated to relocation. Assertions and business logic were preserved.

The 18 supplemental hospitality setup requirements passed against the compiled candidate on the required port 3104 and are included once in the E2E counts. A later attempt to use that compiled workflow on 3102 was rejected by the existing harness before assertions; those attempts are excluded. An interrupted incomplete-cleanup receipt was also excluded, its exact owned partial cohort was removed, and its cases were selected again.

Development scanner signatures crossed the existing 24-hour freshness limit during fixture preparation. Official FreshClam updated daily definitions to 28152, the development scanner reloaded, and execution resumed. No freshness limit was relaxed. Failed preparation is not counted as an executed assertion failure.

Both runners stopped when concurrent source edits changed runtime and verification fingerprints. Their empty/preparation-only receipts for the new source are excluded. The user then requested ending this round and reporting incomplete coverage. The current source requires a new candidate and verification; earlier results are not restamped as current acceptance.

## Failed suites

| Suite | Failed required cases |
| --- | ---: |
| `collaboration.demo-collaboration-action-card.e2e` | 1 |
| `collaboration.home-feed-review.e2e` | 2 |
| `collaboration.social-ai-loading.e2e` | 2 |
| `collaboration.social-beta.e2e` | 12 |
| `customer_authority.demo-retail-customer-authority.journeys` | 2 |
| `identity_access.front-quick-access.e2e` | 2 |
| `identity_access.remembered-workspace.e2e` | 2 |
| `inventory.demo-inventory-lot-serial.e2e` | 2 |
| `marketing.demo-marketing-to-crm-lead.journeys` | 1 |
| `operations.staff-work.e2e` | 2 |
| `organization.chart.e2e` | 6 |
| `organization.draft-panels.e2e` | 1 |
| `people_payroll.self-service.e2e` | 4 |
| `people_payroll.thai-hotel-v1-final-pay.journeys` | 2 |
| `people_payroll.thai-hotel-v1-overtime.journeys` | 2 |
| `people_payroll.thai-hotel-v1-payroll.journeys` | 2 |
| `pos.thai-hotel-v1-pos.journeys` | 4 |
| `procurement.thai-hotel-v1-procurement.journeys` | 2 |
| `shared.business-journeys.demo-engineering-business.journeys` | 1 |
| `shared.ui.page-layouts.e2e` | 1 |
| `shared.ui.uat-route-matrix.uat` | 17 |


See the [repository organization report](repository-file-organization-2026-10-13.md) and [old-to-new path register](repository-file-relocations.json) for the completed file placement work and earlier structural/native checks.

## End-of-round availability

After the user ended verification, local development health on port 3100, [public development health](https://console-dev-erpx.jesadakorn.com/api/health) and private production health on loopback port 3101 each returned HTTP 200. The authenticated [People page](https://console-dev-erpx.jesadakorn.com/holdings/acme/people) rendered its organization and people panel in the actual browser. Development web/jobs remained detached, PostgreSQL continued listening on 5432, and the disposable browser cohort directory was empty. Staging was not visited or changed, and no deployment was performed.
