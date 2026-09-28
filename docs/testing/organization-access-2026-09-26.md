# Organization & Access verification — 2026-09-26

The implementation combines the former Positions, Employees and Directory & GPO destinations at `/holdings/:slug/technology?view=organization`. Existing record links remain valid. See [the behavior contract](../contracts/next/hr-it-technology.md) and [ADR-034](../adr/ADR-034-inherited-gpo-and-atomic-position-moves.md).

## Backend coverage

- `organization-access.test.ts`: Company and local scope filtering, IT read access without compensation or HR commands.
- `position-gpo.test.ts`, `gpo-rule-migration.test.ts`: additive inheritance, duplicate sources, partial withdrawal, future positions, missing identities, stale/expired previews, legacy adapters, owned settings, retries, concurrent workers and single-connection pools. The migration bridge preserves existing and orphan ownership evidence without provider writes.
- `position-moves.test.ts`: whole-cohort atomicity, unchanged status/dates/salary, actual Level and leave history, stale cohorts, cancellation, revoked authority, manager graphs and overlapping moves. An injected mid-batch failure rolls back every holder and the Position.
- `people-lifecycle.test.ts`: existing placement/provisioning behavior and summaries spanning inherited policies and multiple Directory connections.
- `organization.test.ts`, `organization-chart.test.ts`, `directory-operations.test.ts`, `workplace-navigation.test.ts`, `page-layouts.test.ts`: existing workflows and navigation regression checks. Directory removals recheck current source ownership under the connection lock, and package withdrawals retire their membership claims with the verified child operation so a later GPO removal cannot leave an unowned membership behind.

## Browser verification

Authenticated dev was inspected through the existing normal IT session at [the dev console](https://console-dev-erpx.jesadakorn.com/holdings/acme/technology?view=organization), including the three-pane layout, collapsed hierarchy, Level targets, GPO assignment form, and a 390 × 844 mobile viewport with detail drawers. The real Directory connection is not currently ready for writes; policy preview correctly displays that reason. Its write switch was not changed.

The Playwright suites `organization-access.spec.ts` and `position-gpo-ui.spec.ts` use normal seeded HR/IT sessions. HR previews use real APIs. Destructive HR/Directory commands are intercepted so UI confirmation checks cannot change real placements or Directory membership. Backend tests use isolated databases and simulated providers for the actual transitions.

Both browser suites passed: **14 tests**, covering desktop/mobile, all supported drag destinations, unsupported targets, command and keyboard alternatives, search, old deep links, permission separation, and Preview → Confirm.

## Final automated results

| Check | Result |
| --- | --- |
| `bun run test:integration --` with the ten backend/regression files above | **186 passed**, 10 files |
| Playwright, the two browser suites above | **14 passed**, desktop and mobile |
| `bun run typecheck` | Passed |
| `bun run lint` | 0 errors; one existing unused `_props` warning in `service-navigation.tsx` |
| `bun run deploy:build` | Passed in an isolated output directory with pinned Node 24.20.0 and Bun 1.2.17 |
| Changed-file whitespace checks | Passed |

Final release: `.runtime/releases/2026-09-26T03-30-19-194Z`. The build retains the existing Turbopack dynamic-filesystem tracing warning in `src/server/runtime/env.ts`; the release was built successfully. The staging helper's read-only artifact/schema-checksum check passed for this release. Full staging database preflight and activation were not run.

## Environment

Dev migration `0320` and `0321` applied to all three `erpx_dev_h00*_write` databases using their existing dedicated migration roles. The general `db:migrate:local` provisioner was unable to administer the already-owned control database; no credential/bootstrap bypass was used. Dev web remains on port 3100 with the updated worker detached under `.scratch/dev/`.

Final curl health checks returned HTTP 200 for local dev (3100), [public dev](https://dev-erpx.jesadakorn.com/api/health), local staging (3103), [public staging](https://erpx.jesadakorn.com/api/health) and private production (3101). Unauthenticated affected-page requests on both public hostnames redirect to the normal console login and finish with HTTP 200; they do not count as UI acceptance. Python urllib requests received HTTP 403 at the public hosts, while curl and the normal dev browser session succeeded.

The updated dev worker was rebuilt and restarted after the final Directory source-retirement fix. Its detached process and the original dev web listener were rechecked after startup; no new worker error events appeared. Logs, test output, release build output and health results are retained under ignored `.scratch/dev/organization-access-*` and `.scratch/dev/jobs-organization-access.log`.

Staging UI verification was explicitly deferred by the user. The staging release has not been changed in this task, and its authenticated Organization & Access behavior remains unverified. Existing staging and private production services remain available.
