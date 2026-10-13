# Home lazy loading and authority-scoped cache — 2026-10-09

Implemented in the existing dirty workspace without deploying production or accessing staging. Installed toolchain: Next.js 16.3.4, Bun 1.2.17, Node 24.20.0. The installed lazy-loading guide was read before implementation; the [official Next.js guide](https://nextjs.org/docs/app/guides/lazy-loading) was also checked. Lazy imports live in Client wrappers and mount only after viewport or user intent.

## Result

- `SocialHomeShell` renders the heading, ordinary links, Start actions and page structure on the server. Mobile controls and navigation-count consumers remain small Client islands. Navigation, Needs you and feed keep independent streaming boundaries; the first eight conversation posts remain server rendered.
- My day and milestones mount once through `IntersectionObserver` with a 400px margin; unsupported browsers activate immediately. Reserved space is 260px and 80px respectively. Milestones requests and visible-tab polling start only after activation. The initial deferred request is scheduled and cancelled during React Strict Mode's trial mount, avoiding duplicate development requests.
- Composer, handover, Daily Plan, assistant content and expense/leave/request editors have separate intent-driven imports. Record drawer route imports now reference small editor wrappers rather than pulling editor bodies into the initial route. Opened Home tools stay mounted to preserve drafts; the existing drawers handle focus and dismissal.
- Hash intent is captured before Next Link prevents the click default. Existing hash links reopen an already-loaded drawer. Initial hashes and the existing tool events activate imports immediately and replay intent through `initialOpen`.
- Feed, audience and My day readers share `work_projection`'s new Home cache. API and Server Components use the same reader. Request authorization still precedes the reader. Cache identity contains user, holding/company, assignment/version, scope/branch/department, role, modules, effective IAM permissions and revisions, and authority; the cache key also contains environment, database identity, deployment, namespace/version, revision and canonical filters/pagination.
- Home memory results share a 128-entry / 8MiB bound. Redis uses the existing optional adapter. Maximum age is 30 seconds, capped by IAM/assignment deadlines, existing database time boundaries, minute boundaries and My day's shift end. PostgreSQL revision checks fence both hits and fills; changed revisions retry, incomplete results and failures are not retained, concurrent fills are coalesced, and Redis failures fall back to the database. My day obtains Work through its existing revision reader on each fill/retry, so it cannot reuse an externally supplied pre-commit snapshot.
- Opaque authority `contextKey` values key the Home tree. A committed context-change event immediately unmounts old lazy children while the new RSC payload arrives. Existing `workFetch` generation checks and additional feed-selection/unmount guards discard stale responses. Home unread cursors and mark-read remain uncached. API payloads and `private, no-store` are unchanged; My day retains `X-ERPX-Work-Count`.

## Verification

| Check | Result |
| --- | --- |
| Home cache/lazy loading/navigation unit suite | 5 files, 22 tests passed |
| PostgreSQL Work cache and self-service integration | 2 files, 45 tests passed |
| Work fetch context/change/completion suite | 28 tests passed |
| Typecheck | Passed, including engineering tests; final release TypeScript also passed |
| Lint | Passed with three existing unused-variable warnings in PMS refunds / Work fetch tests |
| Architecture + Guest Account terminology | Passed, zero legacy boundary edges |
| Scoped `git diff --check` | Passed |
| Isolated release build | Passed: `.runtime/releases/2026-10-09T06-23-12-261Z` |

The release build reports the existing `src/server/modules/runtime/adapters/runtime/env.ts` dynamic filesystem tracing warning. It builds against an isolated output directory; the development server remains running. No deployment was performed.

New Home tests exercise authority/scope/user isolation (including users with the same role), permission/identity/assignment revisions, changing revisions during fills and hits, concurrent result ownership, deadlines, unavailable sources, retry after failures, Redis failure/recovery and global entry/byte bounds. The PostgreSQL Home integration test uses the same transactional fence and proves rollback remains warm and commit invalidates. Existing Work integration tests additionally cover IAM revocation, assignment expiry, delegations, trigger coverage, direct SQL/job writes and non-recomputation on warm Work reads. Work fetch tests cover response rejection after context changes.

Authenticated browser verification used `https://console-dev-erpx.jesadakorn.com/holdings/acme/branches/bangkok/home` with the existing session:

- Desktop menu, Needs you and eight initial posts render. Mobile at 390×844 renders the shell, counts and tools; both deferred regions are inactive and no `_app-pages-browser` dynamic component chunks are requested initially.
- Opening mobile Plan my day reveals the rail and loads Daily Plan/My day. Daily Plan uses a viewport-height drawer and focuses its date input. Keyboard Enter opens handover; hash opens handover, event opens composer, and the same `#compose` link reopens a closed composer. Composer receives editor focus, retains an unsent draft across close/reopen, and returns focus to its summary. Test text was cleared; nothing was published or submitted.
- Assistant loads only after launcher intent. Expense and purchase-request drawers load their editors on demand. Leave displays the existing HR setup requirement for this account, so leave form submission was not exercised.
- Before scrolling, milestones has no chunk, request or poll. Scrolling to the bottom activates it, displays eight milestones and sends exactly one activation GET after the Strict Mode fix; its component import occurs once. Polling remains intentionally enabled after activation.
- Dispatching the context-change event removes the old Home and shows the refreshing status; reloading restores the authorized tree. Actual permission mutations were exercised by isolated tests rather than changing the authenticated user's grants.
- Initial navigation used the server seed: zero initial navigation GETs, with the expected mark-read POST. Attention polling GETs start later at the existing interval.
- My day API returned 200, `private, no-store`, Work count `10`, and no unavailable sources on repeated reads. Milestones returned eight rows with the same private header.

Screenshots and raw runtime logs remain local under ignored `.scratch/dev/`: `home-desktop-final.jpg`, `home-mobile-final.jpg`, `home-lazy-final-{unit,integration,typecheck,lint,architecture,build}.log`, `home-context-fetch-tests.log`, `home-chunk-audit.json`, `home-revision-samples.json`. Temporary browser timing instrumentation was removed from the root layout; viewport overrides were reset.

## Measurements and limits

Production build comparison uses the existing pre-change isolated release `2026-10-09T05-22-41-253Z` and the final release, counting the JavaScript chunks referenced by the SocialHome client dependency graph once. This includes shared chunks and is **not the entire route's network transfer size**.

| Graph measurement | Before | After | Change |
| --- | ---: | ---: | ---: |
| JavaScript file bytes | 1,443,465 | 1,187,171 | −17.8% |
| Sum of per-file gzip bytes | 398,897 | 326,949 | −18.0% |

Handover/Daily Plan draft logic, handover selection copy, expense/request editor copy and assistant conversation content are present in the old eager graph and absent in the new eager graph. Development network inspection confirms the corresponding closed component imports are deferred. This is build-graph evidence rather than an authenticated production-runtime wire benchmark.

One warmed development reload recorded menu DOM at 2,277ms, feed DOM at 2,282ms, Needs you count at 3,931ms and the header search becoming enabled at 5,636ms. Expanding My work took 43.6ms from click to its expanded DOM. Compilation warmup was performed separately. These are diagnostic observations from the public development origin, **not a controlled before/after latency comparison**; they include network, streaming, hydration and concurrent local workloads. The prior-agent before-timing capture was lost during interruption, so no latency improvement percentage is claimed.

A warmed milestones sequence, excluding compilation, recorded public request durations 283 / 161 / 200ms. Server logs identify miss / memory hit / memory hit respectively, with application request durations 156 / 61 / 57ms. The two revision SQL checks consumed 13.6 / 24.3 / 11.0ms of SQL time and 0.24 / 0.58 / 0.21ms connection wait, respectively. Authentication remains part of every request. A Home feed miss logged 17 SQL queries versus five on an earlier warm hit; this is supporting evidence, not a matched workload comparison.

Across 18 recent individual revision-check observations, median SQL time was 10.92ms, median pool wait 0.11ms and median elapsed time 12.31ms. The range was 3.97–134.49ms elapsed under concurrent activity. Each normal Home hit performs two revision checks, so the check overhead remains material. Nested phase times must not be added to parent metrics, and concurrent cumulative SQL/pool times are not wall-clock page latency.

Remaining measurement limits: no controlled before/after browser latency series, no authenticated standalone production wire benchmark, and no forced slow-feed browser run in this implementation pass. The independent Suspense structure and server-reader concurrency are covered by unit checks; actual browser verification used the available feed. Leave form completion is limited by the account's existing HR prerequisites. Redis outages were simulated at the cache-port boundary rather than stopping the shared Redis service.

## Availability

Final health checks returned HTTP 200 from local development `http://127.0.0.1:3100/api/health`, public development `https://console-dev-erpx.jesadakorn.com/api/health` and private production `http://127.0.0.1:3101/api/health`. The authenticated Home was inspected through the public development origin. The existing detached pinned-toolchain development process remains on 3100. Staging was not accessed and production was not deployed or restarted.
