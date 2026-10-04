# ERPX UAT execution report

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

**Run date:** 2026-09-25
**Scope:** implemented page routes and supported workflow paths in the current source tree. This report does not certify every capability listed in the product catalog.

The isolated desktop/mobile route matrix and simulated business journeys passed. Full public UAT remains open because the public Front hostname does not resolve and the available staging browser session is signed out.

## Route inventory and matrix

The generated [route inventory](uat-route-inventory.md) contains **158** source page routes. Its source fingerprint is `b9cfa3d7b0d5b85255d26dfa11eba1d6b66f1cbaad6891c9f2f618e0c16ecaff`.

The final Playwright run used isolated simulation `fe7095e14f6f` on port 3104 and passed **1,468/1,468 checks** in 6.0 minutes across Desktop Chrome and Pixel 7. The captured route evidence contains **1,466 rows**: 733 per viewport over all 158 route templates. Coverage includes anonymous access, Front access, intended role profiles, permission denials, invalid detail IDs, empty and placeholder states, and holding, branch, department, and combined branch-plus-department scopes. The source fingerprint matched the inventory. There were no browser/server page errors, navigation errors, or horizontal overflow findings.

The 1,462 HTTP 200 responses include normal pages and expected access states. Four responses were 404: each viewport returned 404 for the representative public booking property because the simulation hotel is not published in the public booking catalog, and each viewport returned 404 for a deliberately invalid property ID. The invalid-ID behavior is correct; the public booking flow for a published property was not exercised by this fixture.

Route evidence is retained privately at `.scratch/uat/fe7095e14f6f/route-results.jsonl`. The matrix is implemented in [uat-route-matrix.spec.ts](../../tests/e2e/uat-route-matrix.spec.ts). The 3104 simulation was destroyed after the run; the pre-existing simulation on 3102 was left untouched and remains healthy.

## Simulated workflow journeys

Eight browser journeys passed on both Desktop Chrome and Pixel 7 using disposable simulation data:

| Journey | Desktop | Pixel 7 |
| --- | --- | --- |
| AI work assistant: draft, version/stale handling, citations, source invalidation, manual fallback | Passed | Passed |
| Overtime handoff to payroll | Passed | Passed |
| Sales to collection | Passed | Passed |
| Hotel operations | Passed | Passed |
| Hotel payroll | Passed | Passed |
| POS shift and payment flow | Passed | Passed |
| Procurement request flow | Passed | Passed |
| Hotel overtime | Passed | Passed |

The journeys used isolated fixture databases. The tested flows covered primary actions, validation or stale-state recovery where applicable, role-aware actions, and persisted results after reload.

## Defects addressed

- POS now exposes the active sale and complete payment/refund review details needed to finish the shift workflow; refund approval links return to ERPX for review.
- Front POS links that review actions in ERPX now use the ERPX origin rather than the Front hostname.
- Front refund approval/rejection stays in the ERPX review queue, with its access policy aligned to the POS flow.
- The deterministic AI snapshot guard uses the correct SQL bind count, and the request worker resolves the active holding correctly.
- Hotel housekeeping labels and task flow, payroll draft ownership, business-date formatting, Thai Front labels, and browser-simulation `.localhost` routing were corrected in the tested paths.

## Automated verification

| Check | Result |
| --- | --- |
| AI provider regression tests | 28/28 passed |
| `bun run lint` | Passed; 0 errors and 2 existing unused-variable warnings (`service-navigation.tsx`, `people-payroll.ts`) |
| `bun run typecheck` | Passed |
| `bun run test:integration` | 140 files passed, 2 skipped; 1,088 tests passed, 3 skipped |
| `bun run deploy:build` | Passed using isolated release output |

The successful release is `.runtime/releases/2026-09-25T01-31-43-294Z` with software version `release-62bbe0de7631cdea2d24938fb2ea0107417ee64afbf47f7263d73d4e9cd71ca6`. The build emitted a Turbopack tracing warning because `src/server/runtime/env.ts` uses a dynamic filesystem path; the build still completed.

## Service and public UI checks

| Target | Result |
| --- | --- |
| `http://127.0.0.1:3100/api/health` | HTTP 200; development server remains running |
| `http://127.0.0.1:3101/api/health` | HTTP 200; private production service |
| `http://127.0.0.1:3103/api/health` | HTTP 200; staging service |
| `https://dev-erpx.jesadakorn.com/api/health` | HTTP 200 |
| `https://erpx.jesadakorn.com/api/health` | HTTP 200 |
| PostgreSQL `127.0.0.1:5432` | Accepting connections |
| Pre-existing simulation on port 3102 | HTTP 200; preserved |
| Task simulation on port 3104 | Destroyed; no listener remains |

The signed-in public dev Chrome session rendered the Asset register page and its owner registration form. The staging page redirected to the normal sign-in screen because the available session is signed out; no authentication bypass was used. The public Front host `dev-pos.jesadakorn.com` returned `ERR_NAME_NOT_RESOLVED` in Chrome and could not be resolved with `curl`, so public Front PMS/POS navigation could not be verified. No Cloudflare route changes were made.

## Completion state

The implemented-route simulation matrix and the listed desktop/mobile journeys passed. The UAT goal remains active pending two external conditions: restore public DNS for `dev-pos.jesadakorn.com` and provide an authenticated staging browser session through the normal sign-in flow. After those are available, verify the affected Front and staging pages through their public hostnames before closing the goal.
