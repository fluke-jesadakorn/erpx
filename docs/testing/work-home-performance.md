# Work/Home performance and native development Redis — 2026-09-17

Work and Home share a complete, permission-scoped `WorkSnapshot`. PostgreSQL remains authoritative. Production Redis is not enabled. Public development continues at https://erpx.jesadakorn.com → 127.0.0.1:3100; the installed production service on 3101 is unchanged.

## Backend measurements

Measured on pinned Node 24.20.0, PostgreSQL 18.6 and native Redis 8.10.1 against the current Acme holding context, using its current IAM authority. These are backend function timings, excluding HTTP authentication, transport, JavaScript download and hydration. They are measurements, not latency guarantees.

Before each cold run the benchmark deleted only the exact ERPX key calculated for that actor and database revision. Each cold read was followed by a warm read. The concurrent batch began with that key absent. No database/server-wide Redis flush was used.

| Summary | Mode | Minimum (s) | Median (s) | Maximum (s) |
| --- | --- | ---: | ---: | ---: |
| Work | cold (10 requests) | 1.133 | 1.198 | 1.352 |
| Work | warm (10 requests) | 0.016 | 0.018 | 0.047 |
| Work | concurrent (5 requests) | 1.029 | 1.032 | 1.034 |
| Home | cold (10 requests) | 0.982 | 1.221 | 1.327 |
| Home | warm (10 requests) | 0.011 | 0.019 | 0.050 |
| Home | concurrent (5 requests) | 1.204 | 1.207 | 1.209 |

All 10 cold/warm comparisons matched, including every field, item order, counts and actions. The original unmemoized reader also matched the optimized snapshot: 178 items. That reference reader issued 2,071 SQL statements; the optimized cold reader issued 639. The unmemoized reference is a parity/control run, not a claim that the previous implementation always issued exactly 2,071 statements. Earlier page-server observations in this task put Work at approximately 12–18 seconds.

Raw final backend samples: [work-home-performance-results.json](work-home-performance-results.json). Both summaries met the requested cold ≤3 s and warm ≤1 s targets for these 10-round measurements. Five concurrent cold reads also met the cold target.

## Implementation and correctness

- Request-scoped staff, candidate, principal and workflow caches; routes/defaults/history read in batches. Existing domain read policies and write-time authorization remain in force.
- Development web holding pools default to six connections (`ERPX_DEV_PG_POOL_MAX`, 1–16). Pools and in-flight fills survive HMR. Transactions still use their original single connection and isolation/retry rules.
- Work reuses its SSR summary. Home streams My day in an independent Suspense boundary while the feed remains available, then reuses the streamed summary after hydration. Mutation/focus/visibility bursts are debounced; old context responses are rejected.
- Desktop/mobile share navigation preferences. Navigation counts use the same snapshot. Home's existing API JSON shape is unchanged; `X-ERPX-Work-Count` carries internal count metadata.
- Work editors, the social document/task dialog and the account-switching SDK use dynamic imports. PDF rendering was already dynamically imported. Home no longer imports the calendar validation/schema graph just to format today's date.
- Metrics (`ERPX_PERF_LOG=1`) emit source duration, snapshot statement count, aggregate SQL duration, aggregate connection wait, item count and cache hit/miss. They contain no SQL parameters, credentials or personal identifiers. Aggregate SQL/wait times can exceed wall time because reads run concurrently.

Migrations 0049/0050 maintain transactional per-table revisions, including direct SQL and jobs. Reads check the current revision before cache lookup and recheck after calculation or a hit. A fill spanning a commit is discarded/retried. In-flight fills retain their original time deadline; a request after expiry cannot join an earlier computation. Rollback rolls back both the data and revision. Repeated concurrent changes cause bounded retries and an uncached read, never publication of the rejected fill.

Keys include cache schema version, environment, deployment, database identity, holding/company/user/assignment, workspace scope, role, authority and IAM versions. Only complete Work snapshots are stored. Authentication and command authorization are not served from Redis.

TTL is at most 30 seconds, further bounded by assignment starts/expiry, directory identity freshness, emergency expiry, lifecycle activation, cutoff/restoration, policy candidate expiry, overtime boundaries and the next minute boundary (including business-date changes). Redis TTL is cleanup, not the consistency mechanism. Adding a time-dependent source requires auditing and extending these deadlines before caching it. Unknown table coverage causes a persistent-cache bypass.

Audit/idempotency receipts, read cursors and non-business worker coordination are excluded. POS job exceptions remain tracked because Work displays them; lease-only updates do not invalidate. New business tables must register revision triggers in their migration; otherwise coverage detection bypasses Redis until they do.

## Native Redis operation

The existing launchd service `com.localdev.redis` continues to own 127.0.0.1:6379. No Docker, replacement installation, service restart or changes to other projects' accounts/data were needed.

`erpx_dev` is restricted to `erpx:dev:*`, with only GET/SET/DEL/UNLINK/PING and client connection commands. ACL persistence, PING, namespace reads/writes and denial outside the prefix were verified. The URL/password is stored in ignored, mode-0600 `.env.local`; no credentials belong in Git.

Configuration:

```dotenv
ERPX_WORK_CACHE=redis
ERPX_REDIS_URL=redis://erpx_dev:<local-secret>@127.0.0.1:6379/0
ERPX_DEV_PG_POOL_MAX=6
ERPX_PERF_LOG=1
```

`bun scripts/local/dev-cache.ts` configures only this ERPX user on the existing local service using the existing local administrator credentials file. It is not a Redis installer. `ERPX_REDIS_ADMIN_CREDENTIALS` can select that file. `bun run db:migrate:local` applies revisions. Restart development after environment changes; preserve the existing Tunnel origin and the production service.

The Node `redis` client is shared per process, disables its offline queue, bounds connect/read waits, and reconnects with backoff. A missing/unavailable Redis service falls back to PostgreSQL. Setting `ERPX_WORK_CACHE=off` disables persistent caching. Production ignores this dev-only cache configuration.

## Verification

Regression coverage includes cold/warm parity; five concurrent fills; committed writes, submit/approve/reassignment, IAM revocation, rollback, concurrent writers, truncate, unfinished fills across commit and expiry, tenant/role/scope isolation, incomplete sources and future untracked tables. A disposable ERPX TCP proxy simulated failure/recovery without stopping the shared Redis service. Refresh tests cover coalescing, mutation events and stale responses after context/DOM reuse. A rendering test verifies the feed streams before My day resolves.

Use pinned Node in PATH and explicitly load `.env.local` for integration tests (`bun --env-file=.env.local run test:integration ...`). Tests create isolated `erpx_test_*` databases; the native Redis recovery probe uses expiring, random ERPX-only test keys. The release build is isolated with `bun run deploy:build` while dev remains available.

## Browser observations

Authenticated public Work and Home were inspected in the existing Chrome session. Work showed My actions 0, Waiting 1, My requests 2, Team 161 and Completed 17; navigation eventually showed the same action count. The account switcher opened successfully. The lazy physical-count editor displayed warehouse/date/product controls and was closed without saving.

Representative foreground observations during acceptance:

| Observation | Time |
| --- | ---: |
| Work, warmed page, first contentful paint | 1.06 s |
| Work, warmed page, HTML data received | 0.664 s |
| Home with streaming, first contentful paint | 1.46 s |
| Home with streaming, complete HTML/My day payload received | 3.75 s |
| Work summary HTTP, 10 sequential authenticated requests | 0.319–0.992 s |
| Home summary HTTP, 10 sequential authenticated requests | 0.221–0.326 s |
| Work summary HTTP, 5 concurrent requests | 2.079–2.177 s |

These browser timings include transport and authentication; unlike the backend benchmark, browser cache state was not forced before every HTTP request. No immediate `/self-service/summary` request followed the streamed Home payload; navigation preferences loaded through the single shared provider. Navigation still reads its own small endpoint for unread Home counts and can reuse the Redis Work snapshot.

There is a separate intermittent browser/network queue delay: Resource Timing showed roughly 5–40 seconds between fetch initiation and actual request dispatch on some background/transition requests (with some complete requests taking about 70 seconds), while server logs showed much shorter processing. This was also present before optimization. The cause of that queueing has not been established, and whole-page interactive time is not claimed to meet the backend target. Streaming My day removes its dependency on that initial browser-side summary fetch. Development compilation and large dev-runtime scripts also remain, because the requested public dev origin was preserved.

Validation completed: 153 tests across 13 relevant integration files passed; subsequent targeted UI/streaming/navigation tests passed (52 tests), as did the expanded real-workflow Redis-outage fallback test. Typecheck, lint and isolated release build passed. Local and public `/api/health` returned HTTP 200; authenticated Work/Home returned HTTP 200. Detached dev remains on 3100 using Node 24.20.0/Bun 1.2.17; logs/PID are under ignored `.scratch/dev/`. The current owner account has pre-existing Schedule/Overtime setup warnings because it lacks an active linked staff engagement; these are not cache errors and were not hidden or cached as a complete self-service result.
