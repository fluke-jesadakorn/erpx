# Repository relocation failure attribution — 2026-10-14

The 70 failed requirements from the [closed browser round](repository-required-browser-verification-2026-10-13.md) were compared with the captured pre-relocation source. **69 have evidence of a pre-existing failure or selector mismatch; no relocation regression is confirmed. One procurement desktop hit-target failure remains inconclusive.** No application logic or canonical assertions were changed.

| Classification | Cases |
| --- | ---: |
| Pre-existing, reproduced with the same primary failure (including the POS retry) | 68 |
| Pre-existing selector mismatch established from original source and rendered markup | 1 |
| Exact post-relocation geometric failure still unresolved | 1 |
| Confirmed relocation regression | 0 |

The [case-level attribution](repository-failure-attribution-2026-10-14.json) preserves all 70 suite/title/project identities and the before/after report paths. The original 70 failures remain failed in their original report; this audit does not restamp earlier receipts or certify the current working tree. The other 1,201 unverified requirements were outside this audit.

## Evidence and comparison controls

- The comparison copy under ignored `.scratch/reorganization/failure-audit/before/` uses original snapshot bytes for 5,690 application files, 64 fixture files and 558 test files. All checksums match the snapshot. It does not restore deleted folders in the workspace.
- Executable bodies/assertions in all 21 affected test files match after normalizing import relocation and the one UAT catalogue-read path. No original assertion, case title, timeout or application implementation was edited.
- All 53 E2E/journey and 17 UAT cases were executed against the baseline. UAT used an isolated baseline standalone build; no deployment occurred. Three ambiguous cases received unchanged targeted retries.
- The captured baseline already lacked the deleted engineering TypeScript configuration and generated next-env declarations. Comparison-only harness adapters omit that missing config, supply generated type references from the pinned Next installation, materialize dependencies and relocate standalone links. The copied application/fixtures/tests are unchanged.
- A derived runner under ignored `.scratch` continues collecting recorded failures while preserving original assertion and fixture-isolation checks. It labels execution as comparison, rejects failed cleanup and never marks a receipt passed when a required case failed. Phase/PID suffixes prevent evidence directory collisions. These adapters do not modify the canonical runner.
- Toolchain: Bun 1.4.3, Node 26.11.1, Next 16.4. Disposable cohorts used only authorized simulation ports 3102/3104 and owned `erpx_browser_*` databases. Real providers and staging were not selected.

## Findings

The 17 UAT failures have identical primary messages before and after relocation. They include existing generic-error rendering for `POST_NOT_FOUND`, holding-only Finance/payroll guards under narrower scopes, Company HR authority guards and invalid-ID record-detail navigation timeouts. Correcting those behaviors would exceed file organization.

Other matched failures include missing/stale selectors, obsolete payroll URL expectations, fixture scope/Company mismatches, missing FX configuration and existing interaction timeouts. Four timeout groups show `nextjs-portal` pointer interceptions in both baseline and retained after traces; the Marketing case can fail at different dialogs under the same existing overlay.

For Home feed desktop, both baseline end-to-end attempts stopped earlier during login/navigation. Its original after trace waits for `details.beta-composer`. Original `social-home.tsx` already mounts `ComposerLauncher`; `useToolActivation` starts false, and the original initial launcher is a button until explicitly activated. A markup probe renders the unmodified original component with stylesheet imports adapted and confirms a button, not details. This is source/markup evidence of the existing selector mismatch, not a claim that the baseline end-to-end attempt reached that selector.

The POS transfer mobile retry reproduced the original POST-response timeout without changing its test or application code. The first baseline attempt progressed further and failed opening a receipt, showing the workflow is already unstable. The retry used permitted simulation port 3104; its origin difference is explicit in the retained metadata.

### Remaining uncertainty: procurement desktop

The retained after run failed the manual center-point hit check in `clickInVisualViewport` while assigning a workflow owner. The first baseline run passed that interaction and failed `FX_GAIN_ACCOUNT_REQUIRED`; a second unchanged baseline run failed to open the Assign / change owner dialog. These prove existing workflow problems but do not establish the exact cause of the original geometric miss. A relocation contribution to that point has not been conclusively ruled out. No speculative CSS, readiness, business-rule or assertion change was made.

## Case coverage

| Suite | Failed requirements investigated |
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

Raw diagnostics, traces, source-checksum proofs and comparison adapters remain in ignored `.scratch/reorganization/failure-audit/`. Existing dated evidence and source credentials were preserved.

## End-of-work availability

The existing workspace development process remains listening on port 3100 with pinned Node 26.11.1. Final HTTP checks returned 200 for local development, public development and private production health endpoints; shared PostgreSQL remained available on 5432. The authenticated Home page at `https://console-dev-erpx.jesadakorn.com/holdings/acme/home` rendered Today and Team updates, with the account menu enabled. This availability check does not certify the unresolved procurement case or the concurrently changing working tree. No staging access or deployment occurred.
