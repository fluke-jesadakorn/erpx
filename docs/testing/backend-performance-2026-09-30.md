# Backend and development performance research — 2026-09-30

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

This is the source audit, operational history and subsequent implementation evidence for the backend/API refactor section in [the sole active delivery plan](../production-plan.md). The original investigation changed no application code; the follow-on implementation checkpoint below records the measured changes and their limits. Development verification must use `https://console-dev-erpx.jesadakorn.com`; staging is outside the authorized scope. [Scope and environment rules](../../AGENTS.md)

**Current status:** Work, People and PMS query-reader pilots are implemented, with 145 targeted tests passing and a final isolated release build passing. Four aligned bundler/heap variants retain 1,204 validated HTTP observations. The provisional development default is webpack with 8 GiB old space after an aged 4 GiB worker restarted at Next's heap threshold and two Work requests returned 502; a short 8 GiB repeat had no observed restart. Optional People panels are split from initial code. Authenticated browser checks cover Work, People and PMS settings, while Front verification uses API/tests per the user's current scope. Final local/public development and private production health checks returned 200. Overall browser-navigation and fast-HMR goals are **not proved**. The sections labelled source audit and operational observations retain the earlier failures; final implementation evidence follows below.

## Source audit before implementation

| Evidence | Performance implication |
| --- | --- |
| Next **16.3.4**, React **19.2.8**, `pg` **8.23.0**, Bun **1.2.17**, Node **24.20.0** are pinned. Bun runs the launcher, which starts Next with the pinned Node executable. | Evaluate the installed Next documentation and actual Node process, rather than assuming Bun serves HTTP or applying older Next defaults. [Dependencies](../../package.json), [launcher](../../scripts/local/dev.ts) |
| At the initial source audit, the 3100 listener was PID 41935 with this workspace as its working directory; its parent PID 41929 executed pinned Node with `next dev -p 3100 --webpack`. | Webpack was running while the launcher then followed Next's Turbopack default unless flags overrode it. The later comparison and launcher changes are recorded below; this row describes the audit baseline. [Launcher](../../scripts/local/dev.ts), [installed CLI guide](../../node_modules/next/dist/docs/01-app/03-api-reference/06-cli/next.md) |
| Existing metrics cover named session/account/workspace phases, Work sources and statement-adapter calls. Direct `pool.query` calls and Better Auth/Drizzle queries do not automatically pass through the statement adapter; nested `measureReads` scopes have their own counters. | Treat current query counts as scoped observations, not complete request totals. Add one root request trace and instrument raw-driver paths before claiming an end-to-end SQL reduction. Aggregate concurrent SQL/wait milliseconds can exceed elapsed request time. [Metrics](../../src/server/runtime/read-metrics.ts), [PostgreSQL adapter](../../src/server/runtime/postgres.ts), [auth adapter](../../src/server/auth.ts) |
| Runtime and Redis clients survive development HMR. Pools belong to database bindings and process roles, with holding web development pools defaulting to six; production/jobserver holding bindings currently default to one. | A new pool per request would undo existing ownership. Size the combined binding/process budget using measured connection wait and database capacity. Six is a current default, not an optimum established by this investigation. [Runtime](../../src/server/runtime/env.ts), [Redis](../../src/server/runtime/redis-cache.ts) |
| Work already uses a complete actor-scoped snapshot and revision-fenced development Redis cache. Lists already have a shared scoped keyset pagination utility. PMS still loads all `pms_records` for a property in its state reader. | Reuse these boundaries and improve expensive readers. Initial pagination does not reduce a complete Work snapshot calculation; narrowing a PMS read must not narrow the locked state used by commands. [Work cache](../../src/server/work-cache.ts), [pagination](../../src/server/list-page.ts), [PMS](../../src/server/pms.ts) |

The dated [September 22 report](dev-performance-2026-09-22.md) recorded 144 ms local complete HTML / 561 ms public full-load medians for warm Work, and a 10.291 s → 4.102 s restart comparison. It also recorded 3,889 → 345 Work snapshot statements, plus 502/network outliers. Those are historical measurements with their documented data, actor and methodology; they do not establish current latency. That report used Turbopack then, whereas the current inspected process uses Webpack. Its former public hostname is also historical. [Earlier list audit](list-performance-2026-09-18.md), [Work/Home benchmark and limitations](work-home-performance.md)

## Backend boundaries supported by the evidence

Keep a modular backend inside the existing Next/Node application first: HTTP and Server Component adapters → authorized request context → domain query/command services → scoped SQL repositories/runtime. Browser API calls and Server Components should call the same domain service with the same authorization rules. Server Components should call services directly; routing their server-side reads through ERPX's own public API adds an HTTP round trip. Next explicitly describes this cost. Existing Server Components alone do not solve excessive reads. [Installed BFF guide](../../node_modules/next/dist/docs/01-app/02-guides/backend-for-frontend.md), [official Next BFF guide](https://nextjs.org/docs/app/guides/backend-for-frontend), [list audit](list-performance-2026-09-18.md)

Separate bounded **query models** from complete **command state**. Queries should return the active section, scoped aggregates, a cursor page or selected detail. Commands must retain current authorization, idempotency/version checks, complete required state, transaction ownership and domain invariants. Work's global action counts must preserve the complete existing snapshot; deriving them from a 30-row page would lose actions. Existing source limits still apply, so this does not mean an unbounded database count. PMS calendar reads should use property/date-overlap bounds; accounting/availability commands still need their authoritative locked state. These are repository-derived design recommendations, not claims that a separate API process or microservices would improve this workload. [List audit](list-performance-2026-09-18.md), [PMS reader/commands](../../src/server/pms.ts), [Work readers](../../src/server/work-items.ts), [command receipts](../../src/server/commands.ts)

Existing PostgreSQL jobs remain the home for exports, document/OCR work, large reports and projection refreshes that need durable completion. Removing work from a request must preserve enqueue/commit semantics and avoid moving interactive authorization into a stale projection. The current runtime already distinguishes application/jobs database credentials. [Runtime ownership](../../src/server/runtime/env.ts), [durable jobs](../../src/server/runtime/jobs.ts)

## What to measure before changing architecture

Use the same authenticated account, assignment, company, filters, data volume and source revision for both variants. Record expected page content and API result parity as well as latency. Preserve failures and outliers. Split observations into:

1. **Cold compiler:** first route after a deliberately empty development compiler cache.
2. **Warm compiler after restart:** same source/cache, fresh process, first route in a fixed route order.
3. **Warm application:** repeated HTML/API reads with compiler warm; record persistent-cache hit/miss separately.
4. **Concurrency:** one, five and ten reads; record pool queue wait and database connection usage. Verify revocation, role/company switch and mutation parity before enabling cache reuse.
5. **Browser versus loopback versus public:** document TTFB/full-response, navigation/FCP/interactive behavior, request queue time, transferred JavaScript and public-network overhead separately. An unauthenticated health/login response does not benchmark a protected business page.

These controls follow the repository's prior measurement limitations; the expanded scenarios are proposed acceptance methodology. At least ten sequential samples give a practical initial median; use a larger retained sample for p95 and avoid calling a three-run restart result a stable tail estimate. [September 22 methodology](dev-performance-2026-09-22.md), [browser queue observations](work-home-performance.md)

For the application path, extend the current structured timing logs with a request ID and spans for authentication/context, query services, pool acquisition, SQL, serialization and external dependencies. Record SQL fingerprints/counts without parameter values, actor names or secrets. Next supports `instrumentation.ts` at the project root or `src/`, `register()` startup and OpenTelemetry spans; the installed guide explains Node-only initialization and optional verbose spans. Adding an exporter is an implementation choice, not necessary for the initial local baseline. [Installed instrumentation guide](../../node_modules/next/dist/docs/01-app/02-guides/instrumentation.md), [installed OpenTelemetry guide](../../node_modules/next/dist/docs/01-app/02-guides/open-telemetry.md)

For database profiling, first inspect whether `pg_stat_statements` is already available, then use before/after deltas restricted to the authorized development database/user. Do not reset shared statistics or restart PostgreSQL just to enable it: the extension's shared preload requirement may require a restart and its statistics cover the cluster. If unavailable, use existing request counters plus selected query plans. [PostgreSQL 18 `pg_stat_statements`](https://www.postgresql.org/docs/18/pgstatstatements.html)

Use `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` for selected development **reads**, with representative parameter values and current statistics, then inspect rows, loops, sorts, buffer reads and estimate errors. `ANALYZE` actually executes the query and adds profiling overhead; avoid using it as a mutation benchmark against live business data. Normal HTTP timings remain the acceptance measurement. [PostgreSQL 18 EXPLAIN](https://www.postgresql.org/docs/18/sql-explain.html), [using EXPLAIN](https://www.postgresql.org/docs/18/using-explain.html)

## Webpack and Turbopack comparison

The installed Next 16.3.4 guide says Turbopack is the default and enables filesystem caching for development and builds. Its development cache is `.next/dev/cache/turbopack`; its build cache is `.next/cache/turbopack`. The installed Webpack implementation places its filesystem cache under the effective output directory's `cache/webpack`. Cache reuse makes later sessions faster, so repeatedly cleaning caches can cause the very slowness under investigation. [Installed filesystem-cache guide](../../node_modules/next/dist/docs/01-app/03-api-reference/05-config/01-next-config-js/turbopackFileSystemCache.md), [installed Webpack config](../../node_modules/next/dist/build/webpack-config.js)

At the source-audit stage, the proposed A/B experiment used the existing launcher and pinned tools, with independent ignored output directories through `ERPX_BUILD_DIR`. The later implementation found that custom output could confound Webpack watching and moved the valid comparison to canonical `.next` output in contained source snapshots; that evidence is recorded below. Preserve source/session/data controls and report first-hit routes, warm responses, real hot edits/reverts and peak memory/CPU separately. Restore the authorized 3100 development server after each sequential experiment; do not use reserved or staging ports. Trace Turbopack separately with `--internal-trace` when investigating its compilation work; the installed guide documents `.next-profiles/trace-turbopack.bin`. Webpack/V8 profiling should follow the installed CLI's supported options. [Launcher](../../scripts/local/dev.ts), [installed Turbopack guide](../../node_modules/next/dist/docs/01-app/03-api-reference/08-turbopack.md), [installed CLI guide](../../node_modules/next/dist/docs/01-app/03-api-reference/06-cli/next.md), [port rules](../../AGENTS.md)

## Pool and memoization rules

- Keep one owned pool per database binding/process, reused across requests/HMR. Check aggregate web/jobs/binding connection demand rather than increasing every pool. Measure `totalCount`, `idleCount`, `waitingCount` and request acquisition wait before tuning. Always release leased clients; a transaction uses one client for its entire lifetime. A single client executes queries serially, so unbounded `Promise.all` does not create safe database throughput. [Current ownership](../../src/server/runtime/env.ts), [current transactions](../../src/server/runtime/postgres.ts), [node-postgres pools](https://node-postgres.com/apis/pool), [pooling](https://node-postgres.com/features/pooling), [sizing](https://node-postgres.com/guides/pool-sizing)
- Use React `cache()` only for Server Component render/request deduplication. It is invalidated across server requests; do not assume it memoizes Route Handler execution or worker jobs. For those paths, pass an explicit request-local context/memo map through query services and keep it isolated by request/actor/database. Never promote a request-local authorization result into a global cache. [React cache](https://react.dev/reference/react/cache), [installed deduplication guide](../../node_modules/next/dist/docs/01-app/02-guides/caching-without-cache-components.md), [current authorization memoization](../../src/server/authorization.ts)
- Avoid blanket Next persistent caching for protected data. The installed `unstable_cache` guide says the API has been replaced by `use cache` in Next 16 and cannot read cookies/headers inside its cache scope. ERPX does not currently enable `cacheComponents`; adopting it needs its own compatibility and authorization review. Start by retaining current request deduplication and the established fenced Work cache. [Next config](../../next.config.ts), [installed unstable_cache guide](../../node_modules/next/dist/docs/01-app/03-api-reference/04-functions/unstable_cache.md)

## Tenant-safe persistent cache and cleanup scope

ERPX's existing Work key includes environment/deployment/database, holding/company, user/assignment/role/scope, authority/IAM versions and business revision. Reads check revisions before and after lookup/fill; committed writes and jobs update revisions through database triggers; rollback rolls those changes back. Incomplete trigger coverage bypasses persistent caching. TTL is at most 30 seconds and is shortened at authority/time boundaries; TTL alone is not the consistency mechanism. Redis failure falls back to authorized PostgreSQL reads. Preserve these properties for any new cache; narrower revisions are only safe after dependency completeness is proven. Commands and fresh authentication decisions remain uncached. [Work cache](../../src/server/work-cache.ts), [revision migration](../../migrations/postgres/holding-write/0049_work_cache_revision.sql), [coverage migration](../../migrations/postgres/holding-write/0270_work_cache_coverage.sql), [Redis fallback](../../src/server/runtime/redis-cache.ts), [cache correctness evidence](work-home-performance.md)

The requested cleanup should target only reproducible development output: stop the identified ERPX dev process cleanly, clear its `.next` output and optional TypeScript build info, then restart with the same pinned tooling and current bundler flags. Verify the listener survives and local/public dev health returns 200. This is a one-time stale-output reset, not a periodic optimization. Keep `.scratch/dev` logs/PIDs, `.runtime`, installed Node, `node_modules`, environment files, databases, private uploads, public prepared assets and release artifacts. No staging directories/services or shared caches belong in this cleanup. [Repository rules](../../AGENTS.md), [ignored generated files](../../.gitignore), [prepared asset step](../../package.json), [isolated release builder](../../scripts/local/build-release.ts)

Do not run Redis `FLUSHDB`/`FLUSHALL`. Existing ERPX entries expire quickly; ordinarily no Redis deletion is needed. A reproducible cold Work benchmark may delete only the exact current actor/database/revision key with the already-scoped ERPX account. The repository documents that account's `erpx:dev:*` restriction and the earlier benchmark's exact-key deletion. Browser/CDN caches are separate from `.next`; current development static chunks revalidate in browsers and declare CDN no-store, with content-hashed media handled separately. Clearing `.next` neither logs users out nor clears their browser cache. [Existing Redis operation](work-home-performance.md), [headers](../../next.config.ts)

## Operational observations from the main task

The user-authorized cache reset was subsequently completed: `.next/dev`, `.next/cache`, root `tsconfig.tsbuildinfo` and `tests/tsconfig.tsbuildinfo` were removed after verifying and stopping only the workspace's dev process. Shared Redis, dependencies, databases, environment files, durable storage, releases and staging were not changed. Development was restored with `bun run dev --webpack`, pinned Node 24.20.0/Bun 1.2.17, detached launcher PID 43121 (parent PID 1), listener PID 43147 and the correct repository working directory. Ignored logs/PID are `.scratch/dev/performance-dev-2026-09-30.{log,pid}`. [Raw operational evidence](evidence/2026-09-30-dev-cache-reset.json)

| Anonymous probe | Before reset, three complete-body samples | After reset, three complete-body samples |
| --- | --- | --- |
| Loopback `/api/health` | 18,157 / 1,122 / 71 ms, all 200 | 18,642 / 904 / 18 ms, all 200 |
| Loopback `/login` | 10,639 / 1,267 / 1,953 ms, all 200 | 1,207 / 38 / 50 ms, all 200 |

These samples used Python urllib and did not control process age, compiler warmness, background requests, workload or other source activity. A separate initial post-restart health probe was 200 / 2,132 ms. Consequently this is **not** a causal before/after performance comparison, and the 18-second samples do not by themselves identify compilation versus queueing versus application work. The remaining cold/first-sample delay confirms that clearing caches does not establish a performance fix. [Raw operational evidence](evidence/2026-09-30-dev-cache-reset.json)

An investigative filesystem size scan also ran during these probes and was stopped after it was observed using 66.6% CPU. At that observation the Next process used 58.0% CPU and approximately 4.76 GiB RSS. This added resource contention is a further reason not to infer an existing application bottleneck or a cleanup benefit from these timings. Future baseline runs must exclude the scan. [Raw operational evidence](evidence/2026-09-30-dev-cache-reset.json)

After restart, public development health returned 200 / 232 ms and login 200 / 840 ms in the Python probes; private production health returned 200 / 24 ms. A later curl public health check returned 200 / 3,824 ms. A prior public curl probe returned 502 and another timed out after 15 seconds; those failures are retained in the evidence. Public delivery is variable and its cause is not established. The shared Tunnel reports a running service, but its log is not readable by the current user; no shared Tunnel restart or configuration change was attempted. [Raw operational evidence](evidence/2026-09-30-dev-cache-reset.json)

At the cache-reset stage, existing browser tabs were signed out and new navigation/tab control timed out in the browser bridge. That stage did not verify authenticated business pages or browser load/hydration. Later normal-session browser checks are recorded in the implementation evidence below; no login was bypassed. [Execution plan](../production-plan.md)

Further Python checks returned a local 20-second timeout and public 403 responses. Subsequent curl health probes recovered to local 200 / 7,232 ms and public 200 / 315 ms. The final curl checks, including the anonymous affected-page redirect, are retained under `latest_curl_checks` in the evidence file. Passing health probes do not resolve the intermittent latency/errors or establish protected-page performance. [Raw operational evidence](evidence/2026-09-30-dev-cache-reset.json)

Those checks subsequently timed out locally and publicly again. The Next process reached 254.9% CPU / 5,433,152 KiB RSS; the 48 GiB machine reported 59% system memory free. A separate pinned-Node probe reported a default V8 heap limit of 4,288 MiB. RSS includes memory outside V8, so these observations suggest a heap/compilation-pressure hypothesis without proving it. The dev process was restarted once more with retained compiler caches and `NODE_OPTIONS=--max-old-space-size=8192`, scoped only to the detached development launcher (PID 48670). No tracked application/configuration code or production/staging settings were changed. Current listener and post-restart probes are recorded under `dev_heap_adjustment` and `post_heap_checks`. This temporary process budget needs controlled heap/GC profiling before it becomes a permanent recommendation. [Raw operational evidence](evidence/2026-09-30-dev-cache-reset.json), [installed Next memory guide](../../node_modules/next/dist/docs/01-app/02-guides/memory-usage.md)

After this restart, two initial loopback health probes still timed out at 25 seconds. Public health then returned 200 / 6,650 ms, public login 200 / 2,448 ms and private production health 200 / 44 ms. Thus the heap-budget change has **not** established a fix for startup/request stalls. The current detached listener is PID 48688. Retained failures and the final availability recheck remain in the evidence. [Raw operational evidence](evidence/2026-09-30-dev-cache-reset.json)

The final availability recheck returned local development health **200 / 1,692 ms** and public development health **200 / 17,299 ms**. Development remains running, but the public delay and earlier timeout observations remain unresolved; no performance target is marked passed. [Raw operational evidence](evidence/2026-09-30-dev-cache-reset.json)

## Implementation checkpoint — 2026-09-30

The following measurements use isolated PostgreSQL test databases and the pinned Bun 1.2.17 / Node 24.20.0 toolchain. They establish improvements in specific authorized readers under unchanged fixture data. They do not measure initial Next compilation, HTTP serialization, public delivery or browser navigation, and do not close the overall local-page/public-navigation/API latency criteria.

### Work task query and page projection

`getStaffWork` now calls the shared `taskWorkState` on each row already authorized by `listTasks`, including source-post visibility. The same state evaluator remains in `taskDetail`; Work omits the repeated task read, attachment list and delegation audit history that it does not display. Approval routing, eligible reviewers, blocked reasons and permitted actions still use the existing policy services. Task detail retains its complete response, while commands, transaction boundaries, idempotency, version checks and audit/outbox behavior are unchanged by this extraction. [Work query](../../src/server/domain-work.ts), [shared task state and detail](../../src/server/staff-tasks.ts), [parity tests](../../tests/staff-work.test.ts)

The fixed fixture contains 12 completed tasks, one attachment and one expired delegation audit record per task. Before the service edit, the original reader was measured for **20 sequential warm rounds**. After the edit, another **20 sequential warm rounds** used the same fixture definition; every optimized WorkItem array equalled the original authorized `listTasks` → `taskDetail` projection. Every round reads all 12 tasks, and the recordings retain every timing/failure observation, including outliers. p95 below is nearest-rank observation 19 of 20. [Raw fixture measurements](evidence/2026-09-30-work-task-reads.json), [benchmark/parity test](../../tests/work-read-model.test.ts)

| Work task-source measurement | Original reader | Shared state reader |
| --- | --- | --- |
| SQL driver calls per round | 61 | 25 |
| Warm rounds / failed rounds | 20 / 0 | 20 / 0 |
| Median elapsed | 19.54 ms | 6.11 ms |
| p95 elapsed | 21.44 ms | 6.65 ms |
| Minimum / maximum elapsed | 18.94 / 21.44 ms | 4.99 / 6.82 ms |

This is a **59.02% reduction in SQL driver calls for the completed-task source fixture**, rather than an end-to-end Work-page result. Active approval/routing states have a different policy-query cost. The broader Work reader still aggregates all applicable sources to preserve global actions and relationships; a 30-row response alone does not bound the initial source calculation. The new pure `projectWorkPage` filters type/search/responsibility once, counts each view without sorting its rows, and sorts/slices the selected list. Global action count and relationship options still come from the complete existing authorized snapshot, including pre-existing source caps such as the task reader's 250-row limit. No unbounded database count is claimed. Existing paged and unpaged `/api/v1/.../work/summary` response shapes and defaults remain compatible. [Complete source aggregation](../../src/server/work-items.ts), [page projection](../../src/lib/work-page.ts), [summary adapters](../../src/server/work-summary.ts), [pagination/count parity](../../tests/work-page.test.ts)

The revision fence additionally expires at active task-delegation `starts_at` and `expires_at` boundaries. A no-write test observes authority change at both boundaries with the same database revision. Actor/company/assignment/authority cache partitioning, commit fencing, rollback, incomplete-source bypass and the mocked no-Redis fallback remain covered. The physical Redis connection-failure/recovery test is separate and conditional, not established by the mocked cache suite. This closes a time-boundary gap; it does not replace current authorization or transactional command checks. [Cache fence](../../src/server/work-cache.ts), [cache correctness tests](../../tests/work-cache.test.ts), [conditional Redis integration](../../tests/redis-cache.test.ts)

Targeted verification passed **48 tests** across staff work, Work cache and task read-model suites, plus **23 tests** across page projection, views, sources and cards. Scoped ESLint passed. These are implementation checks; final broad verification and authenticated page/browser evidence are recorded separately.

To reproduce the current task-source comparison and retain 20 optimized rounds:

```bash
ERPX_WORK_READ_BENCHMARK=1 ERPX_WORK_READ_VARIANT=current bun run test -- tests/work-read-model.test.ts --reporter=verbose
```

The command also reads the original complete detail projection as its parity/query-count reference and writes `.scratch/dev/work-task-read-current.json`. The 20-round original-source recording was taken before the service edit; its retained source files are `.scratch/dev/work-task-read-baseline.json` and `.scratch/dev/work-task-read-optimized.json`, with both recordings copied into [the reviewable evidence file](evidence/2026-09-30-work-task-reads.json). It does not reproduce a cold compiler run.

### People selected detail and opened sections

The public directory SQL now resides in a domain repository. `holdingDirectorySelection` checks the fresh Holding binding, selected Company, role, assignment/principal activity and validity before loading the selected record using the existing directory visibility rules. Explorer detail retains its tab authorization, exact Engagement lookup, Company HR/privacy restrictions and response envelope. Ordinary public-directory access still exposes the same public Holding-wide graph; archived targets still require HR permission and the actor's current Company/branch/department scope. No persistent cache, index, command or transaction change was introduced by this reader extraction. [Directory repository](../../src/server/people/directory-repository.ts), [authorized services](../../src/server/holding-people-directory.ts), [detail service](../../src/server/people-explorer-detail.ts), [target/privacy parity tests](../../tests/people-directory-selection.test.ts)

The ordinary-user Profile fixture compares the previous full-Holding directory lookup against selected-record resolution. The latest retained trial contains **20 paired sequential rounds**, with original and targeted readers using the same fixture in each round and returning identical **280-byte** JSON. It retains the initial pool-acquisition/startup observation and every outlier. This is a service/query comparison, not 20 browser navigations or three cold compiler samples. [Retained profile and section observations](evidence/2026-09-30-people-read-models.json), [comparison test](../../tests/people-read-performance.test.ts)

| Ordinary People Profile service | Full-directory reference | Selected-record reader |
| --- | --- | --- |
| SQL driver calls per round | 6 | 2 |
| Paired rounds / failed rounds | 20 / 0 | 20 / 0 |
| JSON bytes | 280 | 280 |
| Median elapsed | 0.963 ms | 0.686 ms |
| p95 elapsed | 1.550 ms | 1.426 ms |
| Minimum / maximum elapsed | 0.873 / 8.210 ms | 0.587 / 3.796 ms |

The **66.67% query reduction** is stable across those 20 pairs. Tail timings varied between exploratory trials: an earlier reported p95 was **1.835 → 3.882 ms**, before the latest **1.550 → 1.426 ms** trial. That earlier comparison's raw file was overwritten before timestamped filenames were added, so its summary cannot support a reproducible latency claim. The separate initial 20-round reference and latest paired trial remain in the evidence; no service or page latency SLA is marked passed from these runs.

`peopleWorkspaceData` loads archived rows only when HR opens `?status=archived`, and loads the private arrangement graph only for Company-level structure managers who can use it. The current public graph remains complete for the established diagram, search and counts. Company structure HR still enters the existing arrangement experience immediately. Single-observation section fixtures show **12 → 10 SQL calls** and **14,881 → 14,501 JSON bytes** for the current Company manager view with one archived person; opening Archived preserves **12 calls / 14,881 bytes**. A scoped HR fixture that cannot arrange structure skips that graph: **12 → 6 calls**, **16,295 → 12,778 bytes (21.6% lower)**. These section bytes exclude page HTML, RSC framing and JavaScript, and these individual observations are not a 20-round page benchmark. [Workspace service](../../src/server/people-workspace.ts), [section/revocation tests](../../tests/people-workspace-reads.test.ts)

Existing `/api/v1/people-public-directory` defaults and full response are preserved. Initial People graph materialization is still required by the current diagram/search/count contract; this implementation narrows selected details and unused private sections rather than claiming that the initial Holding-wide graph is now bounded. Targeted directory/selection/privacy/panel and workspace regression suites, scoped lint and typecheck passed. Optional `people.directory`, `people.archived` and `people.arrangement` spans allow the remaining costs to be measured separately.

Optional HR, editor, payroll, lifecycle, branding and roster panels now use `next/dynamic` in the three existing organization/structure/draft workspaces. Initial graph/search/count/draft behavior, panel props and permission conditions are retained. The observed webpack development People main chunk fell **7,398,402 → 4,430,773 raw bytes (40.1%)**. A production-manifest comparison found **23 → 17 initial chunks**, **1,646,712 → 1,334,318 raw bytes (18.97%)**, and computed gzip-9 estimates **437,213 → 358,587 bytes**; polyfill bytes are separate and unchanged. The earlier production snapshot also predates concurrent non-import changes, so this is an artifact comparison, not a frozen causal experiment. Emitted/manifest bytes are not actual browser transfer, CPU or navigation measurements. The late panel phase passed the six-file 20-test People suite, typecheck and focused lint. Recordings are `.scratch/dev/people-lazy-panels-2026-09-30T07-45-10.482Z/` and the production comparison in `.scratch/dev/backend-final-lazy-build-2026-09-30T08-11-30.877Z/summary.json`.

Reproduce the paired Profile comparison and section checks with:

```bash
bun run test -- tests/people-read-performance.test.ts tests/people-workspace-reads.test.ts
```

Profile raw recordings are `.scratch/performance/people-profile-baseline.json` and `people-profile-comparison.json`; future comparison runs also retain `people-profile-comparison-<timestamp>.json`. Section observations are `people-workspace-sections.json` and `people-workspace-scoped-sections.json` in the same directory. The [reviewable evidence file](evidence/2026-09-30-people-read-models.json) copies the retained observations before further test runs can replace the latest aliases.

### PMS dashboard, calendar and selected detail

PMS now exposes opt-in `view=dashboard`, `view=calendar&from=...&to=...` and `view=detail&id=...` query models in the existing workspace envelope. Calls without `view` retain the complete default response. Calendar windows use exclusive date overlap and reject invalid/reversed ranges or windows over 93 days; detail reads are scoped to the selected property and required related records. Dashboard SSR and polling use the bounded model in both ERP and Front. The existing reservation/calendar UI still uses complete default state for its global lists; the new calendar/detail backend reads are available without a claim that those entire client flows have migrated. [PMS service](../../src/server/pms.ts), [read selection validation](../../src/lib/pms-read-selection.ts), [query-model tests](../../tests/pms-read-model.test.ts)

The same isolated fixture contains one room/type/rate and 200 historical folio entries with 400-character history values. **20 paired sequential rounds** compare legacy Front workspace reads against the dashboard model. Every measured payload stayed at **127,086 → 2,219 serialized bytes**, a **98.25% reduction**, with readiness parity and zero failed rounds. These are serialized service DTO bytes; they exclude HTTP/RSC framing, transferred scripts and browser navigation. p95 uses nearest-rank observation 19 of 20. [Raw fixture comparison](evidence/2026-09-30-pms-dashboard-reads.json)

| PMS service fixture | Complete legacy Front workspace | Dashboard model |
| --- | --- | --- |
| SQL driver calls per round | 8 | 6 |
| Paired rounds / failed rounds | 20 / 0 | 20 / 0 |
| Serialized bytes | 127,086 | 2,219 |
| Median elapsed | 1.638 ms | 0.751 ms |
| p95 elapsed | 1.972 ms | 0.917 ms |
| Minimum / maximum elapsed | 1.402 / 2.586 ms | 0.616 / 1.089 ms |

The payload criterion is demonstrated for this fixture; it is not a before/after authenticated PMS-page result. No persistent PMS cache was added. Commands retain complete locked state: the reader rejects an attempt to combine a narrowed model with command locking. Accounting, approvals, idempotency, optimistic versions and transactional audit/outbox remain on the existing command path. Final focused verification passed **42 PMS tests**: seven read-model cases cover readiness/totals/privacy, exclusive calendar overlap, availability parity, selected detail references, Front ownership redaction and locked-state rejection; the 35 existing PMS cases cover command invariants.

A follow-on parity review caught a bounded calendar/detail readiness regression: omitted room/rate setup records had made a configured open hotel look unready. Those two views now read lightweight property setup facts inside the same load transaction and use them for readiness; the facts are removed before client serialization. The added test checks an open configured property's detail and a calendar window outside its active rate dates, requiring readiness and `frontReady` to match full state. Dashboard/default reads and the recorded dashboard fixture metrics are unchanged by this correction. [Readiness regression test](../../tests/pms-read-model.test.ts)

Front selected-detail reads additionally validate the target after ownership redaction. Another cashier's shift/refund and a credit return 404, while the reader's own shifts/refunds remain visible. This closes an existence disclosure introduced by selected reads without changing the complete default response. [Front detail regression test](../../tests/pms-read-model.test.ts)

```bash
bun run test -- tests/pms-read-model.test.ts tests/pms.test.ts
```

The command retains all 20 pairs in `.scratch/dev/pms-dashboard-read-comparison.json`; the current recording is copied into [reviewable evidence](evidence/2026-09-30-pms-dashboard-reads.json).

### Tested policy and transaction boundaries

Final focused verification passed **145 tests in 18 files, with zero failures or skips**: Work 71, People 20, PMS 42, request/SQL metrics 10, and Redis 2. The run combines new reader/parity cases with existing domain and instrumentation tests. It is targeted verification, not a fresh full integration-suite or every-source/browser acceptance claim. The retained logs, JSON result and command summary are in `.scratch/dev/backend-final-verification-2026-09-30T06-54-00.002Z/`.

| Requirement | Evidence actually covered | Remaining boundary |
| --- | --- | --- |
| Company, role and revoked authority | New People selection tests reject Holding/selected-Company/role mismatches and revoked assignments. New Work state parity removes actions after reviewer revocation; cache tests partition actor/company/role/assignment and invalidate on revocation. PMS read-model tests reject foreign Company/branch and insufficient roles; existing PMS commands reject revoked actors without business writes. | These service/transaction fixtures do not establish real browser Company/role switching, multi-tab behavior, or exhaustive revocation parity for every Work source. |
| Idempotency and versions | Existing staff-work tests replay decisions/delegation revocation, reject stale versions and roll back authority revoked after preparation. Existing PMS payment replay produces no duplicate revenue, and stale property versions are rejected. Narrowed PMS state is rejected for locked commands. | New query models do not change command ownership; no new proof is claimed for all approval/posting/sales modules or external integrations. |
| Concurrent writes and readers | Existing staff decisions permit one competing decision; PMS permits one competing last-room booking and serializes night workers. Work cache tests cover concurrent revision increments, rollback isolation, five coalesced fills and fills crossing commits/time boundaries. Driver tests isolate concurrent/nested scopes. Separate finite authenticated read bursts exercised concurrency one/five/ten and retain the 4 GiB restart/two 502s alongside the 8 GiB repeat. | The finite single-user bursts do not establish sustained multi-user capacity, long-session memory stability or an optimum aggregate pool budget. |
| Cache unavailable or incomplete | Mocked no-Redis reads remain authorized and coalesce without persistence; incomplete sources/trigger coverage bypass persistence and commits invalidate revisions. The configured Redis integration passed a real connection failure/recovery through its own ephemeral proxy and unique short-lived test key. It did not stop Redis or touch another actor's keys. | The proxy test establishes client fallback/reconnection, rather than a shared Redis-service outage drill; no new PMS/People persistent cache was added. |
| Pagination and all-source actions | New pure Work projection tests keep complete action counts across more than 30 rows and multiple item types, match every view/legacy filter and clamp offsets. Existing source tests preserve relationships, separate overrides, priorities and partial-source failures. | Source failures are mocked in the aggregation test. The new performance fixture covers task-source reads; exhaustive live approval visibility across every source remains open. |

The test files behind these distinctions are [Work page/count parity](../../tests/work-page.test.ts), [source aggregation](../../tests/work-sources.test.ts), [staff transactions and shared-state parity](../../tests/staff-work.test.ts), [cache fencing](../../tests/work-cache.test.ts), [People selection/privacy](../../tests/people-directory-selection.test.ts), [PMS commands](../../tests/pms.test.ts), and [SQL metrics](../../tests/read-metrics.test.ts). Existing browser/context-switch test files are not treated as executed acceptance evidence for this performance task.

### SQL timing and scope

Driver instrumentation now counts each leased PostgreSQL `client.query` dispatch once, including statement-adapter calls, raw Pool/client reads, Drizzle/Better Auth calls and transaction-control dispatches. `queries` means driver calls: one SQL string containing multiple statements is one call. `queryMs` sums driver elapsed time, including work queued on a client; `poolWaitMs` sums acquisitions, including failed acquisitions. `queryErrors` and `poolErrors` retain failures without recording SQL, parameters, actor identifiers or error messages. Nested measurement scopes increment their ancestors, while concurrent sibling scopes stay separate. Pool callbacks restore the dispatching scope because a queued callback can execute in another request's release context. [Driver instrumentation](../../src/server/runtime/postgres.ts), [measurement scopes](../../src/server/runtime/read-metrics.ts)

A measurement covers only work awaited inside its span. RSC phase labels alone do not prove complete streaming-render or request totals, and none of these SQL/service timings establishes browser navigation performance. Summed concurrent SQL/pool times can exceed elapsed wall time. Seven instrumentation integration tests passed, covering raw/config/event callbacks, Drizzle without double counting, nested/concurrent scopes, rollback, failed phases, pool timeout/recovery and an HMR-retained pool.

### Authenticated warm HTTP observations and verification

A normal authenticated development session with the active owner assignment was measured through loopback and `https://console-dev-erpx.jesadakorn.com`. The existing webpack process used the temporary 8 GiB heap budget. The run retained one warm-up per target/origin, then **20 measured complete-body HTTP rounds** per target/origin. Selected document/API observations below returned HTTP 200 in every measured round. This run has no source-before HTTP variant and does not establish a causal speedup from the reader changes. These are document/API response timings, not client navigation, hydration or interaction timings. [Retained HTTP observations](evidence/2026-09-30-warm-http-reads.json)

| Actual protected document | Loopback median / p95 | Public development median / p95 |
| --- | --- | --- |
| `/holdings/acme/work` | 163.6 / 297.8 ms | 254.1 / 274.5 ms |
| `/holdings/acme/people` | 165.5 / 182.5 ms | 252.5 / 324.6 ms |
| `/holdings/acme/pms/settings` | 168.3 / 177.3 ms | 266.8 / 342.7 ms |

| Read API | Loopback p95 | Public development p95 |
| --- | --- | --- |
| Work summary `?paged=1` | 81.6 ms | 181.2 ms |
| People public directory | 51.1 ms | 239.6 ms |
| PMS `?view=dashboard` | 42.2 ms | 270.7 ms |
| PMS complete default | 49.5 ms | 191.3 ms |

Outliers remain in the recordings: the maximum loopback Work document was **1,148.1 ms**, and the maximum public PMS settings document was **728.9 ms**. The earlier attempt aborted before collecting page samples because normal sign-in returned **502**; it is retained at `.scratch/dev/http-benchmark-2026-09-30T05-29-39-464Z/evidence.json`. The successful run is `.scratch/dev/http-benchmark-2026-09-30T05-32-03-467Z/evidence.json`, with both copied into the reviewable HTTP evidence.

The `/holdings/acme/pms` root samples are **excluded** from rendered-page conclusions. That route emitted a streamed Next redirect/meta refresh inside an HTTP 200 shell; the older helper labelled these rows successful from status alone. Their original rows remain as redirect controls, while the actual protected PMS settings document is the page reported above. Future HTTP captures classify streamed redirects/error fallbacks separately. A 200 shell alone is not protected-page verification.

Reproduce normal-session HTTP observations against the running development server with an existing cookie header stored in the ignored private development directory:

```bash
bun scripts/local/benchmark-http.ts --cookie-file .scratch/dev/performance-session.cookie --rounds 20 --warmup-rounds 1 --label current-warm
```

The helper restricts origins to development, stores status/body-byte/timing/error observations without credentials or response bodies, and does not stop services, clear compiler output or prove compiler coldness.

### Reusable development-loop comparison

The comparison launcher supports explicit webpack/Turbopack and 4/8 GiB old-space variants, source-hash guards, three cold plus 20 warm rounds by default, and normal-session protected paths. Valid runs use contained source snapshots with canonical `.next` output, archiving/pruning only that run's owned output between cold rounds. Dependencies are cloned into the snapshot because Turbopack cannot resolve an external symlinked project dependency root. It retains HTTP failures/streamed redirects, pool/request headers, process-tree RSS/CPU observations and a health-route server HMR edit/restore probe. The probe measures server hot compilation, separately from browser Fast Refresh. Dependency sizes/static references are candidates, not measured compile costs. [Comparison launcher](../../scripts/local/benchmark-development.ts)

```bash
bun scripts/local/benchmark-development.ts --inspect
bun scripts/local/benchmark-development.ts --run --snapshot --cookie-file .scratch/dev/performance-session.cookie --paths '/holdings/acme/work,/holdings/acme/people,/holdings/acme/pms/settings,/api/v1/holdings/acme/work/summary?paged=1,/api/v1/holdings/acme/people-public-directory,/api/v1/holdings/acme/pms?view=dashboard' --cold-rounds 3 --warm-rounds 20 --restore-bundler webpack --restore-heap 8192
```

Inspection is read-only. A run requires port 3100 to be empty after the identified workspace process has been stopped; it refuses to adopt or stop an existing listener. It signals only detached groups created by that run and restores a detached server using the original repository working directory, with local/public health and listener-persistence rechecks. The helper now records safe `paged=1|true` and `view=dashboard` selectors, omitting other query values. Protected session cookies and response bodies are not written into reviewable evidence.

### Retained invalid, interrupted and supplemental experiments

Every attempt remains in the [sanitized development-loop evidence](evidence/2026-09-30-development-loop.json). Earlier failures are excluded from aligned variant ranking, rather than removed or counted as successful runs.

| Raw run under `.scratch/dev/` | Classification and retained boundary |
| --- | --- |
| `bundler-2026-09-30T05-36-46-190Z/evidence.json` | Source changed after the first webpack/4 GiB cold iteration. Work timed out at 30 s; People/PMS settings took 22.5/12.7 s; 61 background GETs accompanied seven measured requests. The older helper recorded five unvalidated 200s, one timeout and one health 503. No complete comparison or HMR result exists. |
| `bundler-2026-09-30T06-02-56-090Z/evidence.json` | Custom `.scratch` output comparison was interrupted after manifest races: 315 HTTP attempts, 98 failures comprising 83 HTTP500, eight HTTP502 and seven timeouts. Installed Webpack ignores canonical `.next` output; an output/watch exclusion mismatch is the likely confound, without a direct watcher trace proving causation. Races disappeared after canonical output was used. |
| `bundler-2026-09-30T06-21-53-338Z/evidence.json` | Completed webpack4 and Turbo8 variants are selected. Seven extra Turbo4 cold-round-one observations remain separate. ENOSPC during its next cold round prevented complete duration/status/restoration recording; those missing values cannot be reconstructed. |
| `bundler-2026-09-30T06-42-26-633Z/evidence.json` | Complete Turbo4 is selected; interruption occurred before webpack8. The old helper did not record signal provenance, so its sender cannot be established. A restored webpack8 TCP-ready process is not a measured variant. |
| `bundler-2026-09-30T06-59-15-052Z/evidence.json` | Supplemental webpack8 omitted Work's `paged=1` through an invocation error: 518,362 full-response bytes versus 25,555 paged bytes. All 301 observations remain, but the run is excluded from aligned comparison. This does not establish live data growth. |
| `bundler-2026-09-30T07-20-20-968Z/evidence.json` | Corrected webpack8 preserves the same paged Work/dashboard PMS selectors and supplies the selected final variant. |

The first attempt's restored development server initially returned local/public health **503** because required scanner configuration was absent, rather than because PostgreSQL was unavailable. Restoring the existing local clamd socket environment and Next reload recovered local **200 / 457 ms**, public **200 / 222 ms**, and private production **200 / 5.4 ms**. No scanner safeguard was disabled. These are historical recovery checks, separate from final availability.

### Aligned four-variant HTTP comparison

Each selected variant completed **three cold local rounds** and **20 warm rounds per local/public origin** for seven targets: Work, People, PMS settings, paged Work API, People directory API, dashboard PMS API and health. That is **301 observations per variant / 1,204 total**, with no validated HTTP failures in the selected samples. p95 is nearest rank: the 19th sorted value of 20; for three cold samples it is the maximum and is not a stable tail estimate. Full-body fetches use manual redirects and validate API envelopes/streamed redirect/error fallbacks. All samples and outliers remain in [the evidence](evidence/2026-09-30-development-loop.json).

The source hash and selected API queries match across variants. Work/People/PMS API sizes are consistently **25,555 / 318,889 / 3,066 bytes**. The live database, jobs, other browser requests, CPU workloads and operating-system file cache were not isolated; byte equality does not prove data identity. Routes are visited in a fixed Work→People→PMS order, allowing later routes to reuse shared compilation. Public first-hit cold timings were not collected. The matrix also predates the optional People panel split.

The document cells below are **Work / People / PMS settings**, in milliseconds. Warm cells give median (p95).

| Bundler / old-space budget | Cold local median | Warm local median (p95) | Warm public median (p95) | Peak summed process-tree RSS |
| --- | --- | --- | --- | --- |
| webpack / 4 GiB | 13,733 / 6,074 / 3,078 | 164 (721) / 161 (266) / 154 (168) | 251 (326) / 257 (303) / 247 (292) | 5.37 GiB |
| Turbopack / 8 GiB | 12,503 / 1,860 / 989 | 162 (173) / 159 (174) / 155 (175) | 254 (380) / 258 (328) / 249 (276) | 6.43 GiB |
| Turbopack / 4 GiB | 15,423 / 1,626 / 1,127 | 193 (218) / 197 (234) / 199 (254) | 304 (459) / 305 (380) / 309 (382) | 9.36 GiB |
| webpack / 8 GiB, corrected selectors | 14,476 / 4,729 / 3,080 | 166 (679) / 163 (280) / 154 (396) | 258 (**2,525**) / 262 (478) / 257 (345) | 6.76 GiB |

| Bundler / old-space budget | Warm local read-API p95: Work / People / PMS | Warm public read-API p95: Work / People / PMS |
| --- | --- | --- |
| webpack / 4 GiB | 85.8 / 47.1 / 43.9 ms | 157.9 / 320.7 / 154.3 ms |
| Turbopack / 8 GiB | 61.0 / 49.7 / 38.2 ms | 197.5 / 195.3 / 148.8 ms |
| Turbopack / 4 GiB | 208.4 / 72.3 / 65.6 ms | 242.9 / 218.9 / 171.0 ms |
| webpack / 8 GiB, corrected selectors | 389.8 / 50.7 / 55.4 ms | 234.2 / 185.2 / 183.3 ms |

Selected local HTTP-document medians are below one second and selected API p95 values below 500 ms. Those observations do not establish all ordinary API workloads or client-navigation acceptance. Corrected webpack8 public Work p95 is **2,524.535 ms**, maximum **8,216.319 ms**, despite its 257.960 ms median. Typical API driver counts are 24/25/24; Work cache-rebuild observations reach 385 calls and retain large summed SQL/pool waits. RSS is summed across descendant processes, whereas old-space applies per Node process; CPU peaks of 1,144–1,512% reflect multi-core process activity. These controls do not establish heap causality or a universal fastest bundler.

### Development HMR and selected launcher defaults

Frozen-snapshot health-header HMR edit probes timed out after about **90 s** on both Turbopack budgets. Webpack4's edit took **2.905 s**; its immediate restore check was stale and cannot measure restore latency. Corrected webpack8's polling probe passed edit/restore in **2.846 / 2.578 s**. A Turbo restore showing the unchanged header after a failed edit is not a successful edit cycle.

Original-workspace route probes provide a separate check under background activity. Turbopack failed three edits at **45 s** and two restores at **45 s**; the initial sub-millisecond edit fields measure file writes only. Webpack passed three edit and three restore cycles in **7.6–8.3 s**, returning correct headers and restoring the exact original file. Raw records are `.scratch/dev/original-turbo-hmr.json` and `original-webpack-hmr.json`.

Actual Work component probes also retain their limitations. The 4 GiB webpack marker appeared with unchanged `NavigationStart` and a **13.168 s** client refresh interval; a later restore observation had a different navigation start and cannot prove a no-reload restore latency. The final single 8 GiB cycle observed marker appearance/removal with unchanged navigation start and exact source restoration: observation upper bounds **10.262 / 8.303 s**, with client event windows **7.773 / 7.848 s**. Tool round trips, multiple rebuilding events and background traffic are included. Short printed sub-rebuild times do not describe the whole edit latency. Records are `.scratch/dev/final-webpack-component-hmr.json` and `final-heap8-component-hmr.json`. Fast development-loop latency remains unresolved.

The launcher now selects **webpack / 8 GiB old space** by default. Webpack is supported by the route HMR reliability observations; 8 GiB supplies operational headroom after the aged 4 GiB worker's memory-threshold restart during finite reads. The short 8 GiB repeat had no observed restart, but does not prove long-session stability or an optimum permanent budget. Explicit `--webpack`, `--turbopack`/`--turbo`, `ERPX_DEV_HEAP_MIB=4096|8192` and an existing `NODE_OPTIONS` old-space setting remain honored. Next's installed development server intentionally restarts when used V8 heap exceeds **80% of heap size limit**; this event is distinct from an OS out-of-memory diagnosis or aggregate RSS. [Launcher](../../scripts/local/dev.ts), [installed memory threshold](../../node_modules/next/dist/server/lib/utils.js), [installed request/restart handling](../../node_modules/next/dist/server/lib/start-server.js)

### Authenticated browser and finite read bursts

Normal existing Console-session checks rendered Work actions/counts, People Draft/Tray and on-demand Archived/Current views, and PMS settings room/rate/readiness controls through the public development URL. Front browser sign-in uses a separate session; the user subsequently selected **API/tests for Front**, so no Front browser verification is claimed or required for this implementation scope.

The retained browser timings are **full reloads and CDP DOMContentLoaded**, not SPA transitions, INP or isolated hydration. Control readiness is an upper bound including browser-bridge round trips. The [sanitized browser/concurrency evidence](evidence/2026-09-30-browser-concurrency.json) retains **85 attempts / 83 timing captures / 76 verified controls / nine unverified controls**, including five exploratory Work reloads before readiness waiting. It records 46 known bridge errors and 25 attempts whose error count was not recorded. Every attempted round, missing metric and unsuccessful control probe remains; missing fields are not zeroes or successes. Concurrent source/body changes and differing control probes prevent a causal before/after claim. The raw recording is `.scratch/dev/browser-final-observations.json`.

| Browser observation | Attempts / DCL captures | DCL median / p95 / max | Control-ready upper-bound median / p95 | Failed control probes / bridge errors |
| --- | --- | --- | --- | --- |
| Work baseline | 20 / 20 | 509 / 1,332 / 6,881 ms | 1,232 / 1,666 ms | 0 / not recorded |
| People before optional panel split | 20 / 18 | 923 / 1,965 / 1,965 ms | 5,078 / 10,497 ms | 3 / 19 |
| People after optional panel split | 20 / 20 | 1,200 / 3,377 / 6,222 ms | 6,773 / 10,944 ms | 3 / 26 |
| PMS settings | 20 / 20 | 1,298 / 4,576 / 14,812 ms | 1,619 / 5,049 ms | 0 / 1 |

PMS's first ten observations overlap source/check activity; its final ten were collected after checks. People probe failures/bridge timeouts do not themselves establish an application regression, and the smaller emitted code does not establish faster navigation. The public-navigation target remains open.

Finite reads used the same normal authenticated user/live dataset, **20 requests per API/origin/concurrency group** across Work, People and dashboard PMS, loopback/public, and concurrency **1/5/10**: **360 attempts at each heap budget**. The aged 4 GiB worker restarted from PID90022 to95956 at 08:15:42 UTC after the explicit Next memory-threshold log; that run retains **358 HTTP200 and two public Work HTTP502**. The fresh 8 GiB repeat retained **360 HTTP200 with no observed restart**. Not every 4 GiB round was warm after the restart, and aged/fresh processes are not a controlled latency comparison. Validation checks HTTP200 and a JSON `data`/no-`error` envelope; it does not establish complete semantic parity. Raw evidence is `.scratch/dev/concurrent-reads-2026-09-30T08-15-11-731Z/evidence.json` and `concurrent-reads-2026-09-30T08-23-39-834Z/evidence.json`; [sanitized evidence](evidence/2026-09-30-browser-concurrency.json) preserves all **720 attempts, including both failures**, and SQL/pool observations.

| 8 GiB finite burst concurrency | Local p95: Work / People / PMS | Public p95: Work / People / PMS |
| --- | --- | --- |
| 1 | **2,459.2** / 47.6 / 41.6 ms | 286.3 / 207.8 / 178.0 ms |
| 5 | 75.2 / 66.3 / 61.4 ms | 488.8 / 306.6 / 154.9 ms |
| 10 | 133.1 / 98.3 / 87.5 ms | 293.7 / **518.7** / 184.8 ms |

First-hit compiler/cache work remains in these samples: the 8 GiB local Work maximum was **2,480.4 ms**, and the public People concurrency-ten p95 exceeded 500 ms. The unchanged listener/no-restart observation supports operational headroom during this short repeat, rather than sustained multi-user capacity, long-session memory stability or a general API latency pass.

### Final verification and remaining acceptance

The 18-file focused command passed **145 tests / zero failures / zero skips**. After the optional panel import change, the People six-file suite passed **20/20** again. No command/transaction behavior was changed by that phase or the launcher-budget change. Full lint reported **0 errors / 3 pre-existing warnings** (`_props` in `service-navigation.tsx`, `bankVersions` in `demo-payroll-runs.spec.ts`, and `state` in `thai-hotel-v1-hr-lifecycle.spec.ts`). Final typecheck, scoped launcher lint and isolated `deploy:build` passed without warnings; the chosen-launcher release is `.runtime/releases/2026-09-30T08-28-31-629Z`, software version `release-a497a19a531999bb9c3c6fc965e0df1ea15069c8548e2f878825b3afca0f1ad6`, with **no deployment**.

The final copied build matches all **1,366 checked source/script/config files** (builder-generated release metadata excluded), and business source matches the prior post-panel build. Temporary HMR source probes were restored. Final logs/source identity are under `.scratch/dev/backend-final-8gib-build-2026-09-30T08-28-31.516Z/`; the broad focused-test recording remains `.scratch/dev/backend-final-verification-2026-09-30T06-54-00.002Z/`.

```bash
bun run test -- tests/staff-work.test.ts tests/work-cache.test.ts tests/work-read-model.test.ts tests/work-page.test.ts tests/work-views.test.ts tests/work-sources.test.ts tests/work-cards.test.ts tests/people-read-performance.test.ts tests/people-directory-selection.test.ts tests/holding-people-directory.test.ts tests/people-api-privacy.test.ts tests/people-explorer-panels.test.ts tests/people-workspace-reads.test.ts tests/pms-read-model.test.ts tests/pms.test.ts tests/read-metrics.test.ts tests/read-request.test.ts tests/redis-cache.test.ts
```

A selected small live PMS dashboard plan already used the existing booking-reservation index: three result rows, two shared-buffer hits/no reads, planning **0.153 ms**, execution **0.038 ms**, recorded in `.scratch/dev/backend-with-plan.json`. That observation does not justify a new index or establish large-volume capacity. No persistent People/PMS cache, connection-pool increase, transaction rewrite or worker replacement was introduced.

The **≥50% query/payload criterion is demonstrated on fixed service fixtures**. The initial People graph, Work's complete capped source snapshot and PMS calendar/reservation client migration remain bounded-read opportunities. General browser-navigation/HMR latency, sustained pool/memory capacity, live Company/role switching and exhaustive approval visibility across every source remain unproved. Healthy HTTP responses and smaller service payloads do not close those acceptance items.

Final availability was rechecked after the chosen-launcher build and actual public UI checks. The first set retains **local/public development 200 / 1,596.8 / 1,643.1 ms**, private production **200 / 28.4 ms**; the subsequent recheck was **local 200 / 14.9 ms**, **public 200 / 156.1 ms**, **private production 200 / 4.4 ms**. PostgreSQL on 127.0.0.1:5432 was accepting connections. Detached launcher PID96558 (parent1) and listener PID96565 remained with the repository working directory, pinned Node24.20.0/Bun1.2.17 and 8 GiB old space; the listener was rechecked almost 12 minutes after startup, with zero memory-restart/OOM log matches so far. Ignored log/PID records are `.scratch/dev/performance-heap8-2026-09-30.{log,pid}`. Fresh normal-session Work search/count six, People Current/HR controls and the preserved one-change saved Draft, plus PMS setup/readiness/rooms/rates/accounting controls rendered without business writes. These are availability/behavior checks, not navigation timing acceptance. [Final sanitized availability](evidence/2026-09-30-browser-concurrency.json)

An additional end-of-work recheck at **08:47:29 UTC** returned local development **200 / 18.7 ms**, public development **200 / 681.5 ms**, and private production **200 / 4.8 ms**. The same detached launcher/listener remained after 25 minutes, with no memory-restart/OOM log events. These additional observations are retained in the sanitized evidence; they do not establish long-session reliability.

Development remains available at **https://console-dev-erpx.jesadakorn.com/**. Private production/shared PostgreSQL remained available; staging was not accessed.
