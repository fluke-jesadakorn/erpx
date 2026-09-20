# Staff self-service acceptance

Verified on 2026-09-17 with Bun 1.2.17, Node 24.20.0 and the pinned Next.js 16.3.4 workspace. See [implemented interfaces and boundaries](../contracts/next/staff-self-service.md).

## Automated verification

| Check | Result |
| --- | --- |
| TypeScript (`bun run typecheck`) | Passed |
| ESLint (`bun run lint`) | Passed |
| Full PostgreSQL integration suite | 62 files passed, 595 tests passed; 2 optional external-service files/tests skipped |
| Focused final self-service domain suite | 12 tests passed |
| Desktop/mobile self-service E2E | 8 tests passed |
| Personal-roster E2E after final presentation change | 2 tests passed |
| Isolated release build (`bun run deploy:build`) | Passed; release `2026-09-17T11-34-41-935Z` |
| Whitespace validation (`git diff --check`) | Passed |

The external tests require separately configured Stripe sandbox and live UCS directory environments. They were not enabled. Existing production-payroll restrictions remain in place.

Integration coverage includes concurrent leave submissions, exactly-once reservations/usage/cancellation reversals, rejection release, year-specific balances, repeatable grants/accrual catch-up, policy snapshots, roster half-days and nonworking dates, legacy working-day payloads, evidence privacy and Company isolation. Overtime coverage includes advance and retrospective paths, independent manager/HR review, returned actuals, timezone/overnight duration, future actuals and overlap rejection, explicit contractor eligibility, stale settings updates, unique payroll handoffs, manual-hours reconciliation and payroll preview source snapshots. Work projection, canonical Inbox links and redacted Calendar dates are covered.

The new browser suite is `tests/e2e/self-service.spec.ts`. It checks keyboard access, light/dark presentation, mobile layout, feed interactions, partial Home-source failures, server draft recovery after reload, stale-version recovery with retained edits, overtime preview/withdrawal, leave half-day preview, interrupted private-upload retry, and canonical shift/purchase destinations. Screenshots and logs are under ignored `.scratch/self-service/` and `.scratch/self-service-*.log`.

The browser suite used a separate Next.js server on loopback port 3102 and six disposable PostgreSQL databases owned by `erpx_test_admin`. Synthetic staff and leave entitlements existed only in that environment. Existing account credentials and real staff setup were not changed. The temporary server and databases were removed after testing.

To run integration checks with the local test-database configuration loaded:

```sh
.runtime/node/node-v24.20.0-darwin-arm64/bin/node --env-file=.env.local node_modules/vitest/vitest.mjs run
```

For browser testing, use the normal isolated seed/setup workflow and set `ERPX_E2E_BASE_URL` to its origin before running `bun run test:e2e:dev tests/e2e/self-service.spec.ts`. Do not reset an existing user's credentials to match a fixture.

## Public authenticated inspection

The existing signed-in session was used at `https://erpx.jesadakorn.com` to inspect Home, My leave, Overtime, My shifts, purchase creation and People settings. Home retained its composer/feed and prominent shortcuts; the compact mobile layout kept My day collapsed initially. Request and settings forms retained the ERPX theme. The historical built-in-role permission repair was verified by opening the actual Company policy settings after migration.

The signed-in Company owner has no active staff Engagement. Personal submission on that real account is therefore blocked by the intended setup guidance; complete employee request transactions were verified with synthetic staff in integration and E2E tests. No real leave, overtime or purchase request was submitted during public inspection.

The healthy development server on port 3100 was reused and left in its existing detached screen session with pinned Node/Bun. Logs and current PID/health evidence are under `.scratch/dev/`. Public Tunnel routing and the installed production service on port 3101 were preserved. Final checks returned HTTP 200 from both `http://127.0.0.1:3100/api/health` and `https://erpx.jesadakorn.com/api/health`; details are recorded in `.scratch/dev/self-service-verification.json`.
