# Development performance — 2026-09-22

Implemented on the existing dirty workspace, preserving its business changes and HTTP contracts. Public development remains https://erpx.jesadakorn.com → port 3100. Production 3101 and Tunnel rules were not changed.

## Final measurements

Existing authenticated Acme holding-owner Chrome session; 213 Work items and five My actions. Ten consecutive warm samples per page, including slow samples:

| Page | Local complete HTML response median | Public full document load median | Public restart median before → after | Restart reduction |
| --- | ---: | ---: | ---: | ---: |
| Work | 144 ms | 561 ms | 10,291 → 4,102 ms | 60% |
| Supplier invoices | 136 ms | 548 ms | 1,536 → 1,159 ms | 25% |
| PMS dashboard | 123 ms | 613 ms | 1,646 → 1,078 ms | 35% |

The ≤1 s local HTML and ≤1.5 s public full-load median targets pass. Work exceeds the 50% first-open improvement target. Supplier invoices and PMS improve on restart, but do **not** reach a 50% reduction individually. Local HTML timing excludes browser assets and hydration; it is not a separate local browser full-load measurement. These distinctions must remain visible when citing these results.

The complete Work snapshot fell from **3,889 to 345 queries**, with the same 213 items. The remaining common-page bottleneck was repeated IAM role lookup: account access fell from **43 to 8 queries**, and workspace choices from **120 to 15 queries**. Warm Work summary function median was 27.86 ms in the separate backend benchmark; that number is not page load time.

## Changes

- Additive migration `0270_work_cache_coverage.sql` installs missing revision rows and row/truncate triggers. The runner checks coverage after every holding-write migration batch, including already migrated databases. Original migrations are unchanged. Applied locally; Acme cache coverage is complete.
- Cache reads validate revision rows and enabled triggers. Logs identify bypass reasons. Fills coalesce only behind the database, actor and authority fence. Navigation uses the same snapshot and derives My actions from it. Redis outage keeps fresh authorized database reads available.
- Orders, invoices, journals and payroll use minimal batch readers with the same module permission/scope helpers and equivalent detail joins. Request-local flow data and fulfillment readiness are shared. Commands retain fresh reads.
- Assignment role definitions are fetched once per company, using the same active published role constraints as individual IAM reads. Session and workspace-company reads use React request-local memoization, without persisting authentication decisions across requests.
- Work form preparation is dynamically imported when a form is opened. Directory session checks load provider/synchronization dependencies only when needed.
- Dev directly launches Node 24.20.0, checks Bun 1.2.17 and forwards trace flags. Asset deployment IDs are derived from source, dependency/configuration files and public build environment: unchanged-source restarts can reuse revalidated assets; source changes retain Next version-skew protection.
- Dev assets use `private, no-cache, must-revalidate` in the browser; both CDN directives remain `no-store`. An unchanged public JavaScript request returned **304**, with its ETag and Cloudflare **BYPASS**. Turbopack and filesystem caching remain enabled. The installed Next guides were read before changes.
- Optional `ERPX_PERF_LOG=1` now reports session, account-access, work-context, layout and workspace-choice read phases without actor identifiers or SQL parameters.

## Verification

- Final typecheck and lint pass; lint has two existing unused-variable warnings in `service-navigation.tsx` and `pms/workspace.tsx`.
- IAM/Work/cache integration suite: 52 passed. Affected flow/cache suite after fulfillment batching: 27 passed. IAM, workspace-choice and active-context suite after role batching: 41 passed. Redis TCP failure/recovery: two passed, using an isolated proxy without stopping shared Redis.
- Coverage includes commit/rollback, concurrent writes during fills, expiry, user/company/role/database separation, IAM revocation, new tables after earlier migrations, missing-trigger repair, Redis failure and role publication/archive. Batched flow parity covers owner, employee, finance and branch contexts.
- Live comparison matched all 213 Work items, ordering, fields and actions; all 59 commercial/financial flow items matched the detail-authorized reader. Its 2,887 queries / 3.480 s became 185 queries / 0.881 s in the same-run comparison. Both paths share the final fulfillment reader, so this isolates document detail-loading overhead.
- Browser Work showed 213 total, My actions five, sidebar five. New expense form opened and returned without saving. Supplier invoices and PMS rendered under the same existing authenticated session. This Acme scope has no supplier matching records and no configured PMS hotel; these are not populated-hotel benchmarks.
- Actual hot reload verified by temporarily changing the Work description in source, observing the open browser update without reload, then restoring the exact original file and observing the original text return.
- Final isolated release build: `.runtime/releases/2026-09-22T11-40-49-730Z`. It used a separate output directory and was not deployed. Generated capability artifacts were refreshed with the repository generator because its consistency check required it; no acceptance gates were manually promoted.
- The temporary authenticated loopback measurement route was removed before the final build. No permanent HTTP API was added.

## Method and limitations

Raw samples, including earlier rounds and failures, are in [dev-performance-2026-09-22.json](dev-performance-2026-09-22.json). Public times use PerformanceNavigationTiming through loadEventEnd; responseEnd is recorded separately. Loopback timing used a temporary development-only allowlisted harness that forwarded the existing browser Cookie unchanged to port 3100 and read the full HTML body. It checked normal authentication and expected page content for every response. No session was created, modified or bypassed.

For the restart comparison, seven pre-optimization performance files were replayed from `.runtime/build/2026-09-22T06-04-05-771Z` within the current workspace. This reproduced the original 3,889-query / 213-item path and a 10.29 s first Work opening. The repaired database coverage and current business modules were retained for both variants; this is a controlled module replay, not a rollback of the database. Final source files were backed up and restored with SHA-256 guards.

Three complete restart runs per variant use the same session, data and route order (Work, Supplier invoices, PMS). Browser/filesystem caches were retained; Work was first. Other open tabs and processes were not stopped, and may reconnect or compete for resources. The source-derived deployment ID may also change when other workspace work changes source files. This is a normal dev-restart benchmark, not a clean-cache compiler benchmark.

Two extra final runs encountered public 502 responses. They are retained separately in JSON, rather than silently dropped; complete runs were repeated. Shared Tunnel logs recorded simultaneous QUIC stream/network timeouts while loopback remained 200. Public health and authenticated pages recovered without changing any Tunnel rules or other services. A median target does not eliminate network or cold-snapshot outliers.

Final availability: local health **200 / 7 ms**, public health **200 / 150 ms**. Authenticated Work, Supplier invoices and PMS were checked again on this process; client navigation from Supplier invoices back to Work also passed. Detached `bun run dev` launcher **2746** is parented by PID 1; listener **2770** has the repository working directory and the pinned Node binary. The removed measurement endpoint returns 404. Logs and PID remain in ignored `.scratch/dev/speed-dev.log` and `.scratch/dev/speed-dev.pid`.

A final restart initially met a draining previous Next process that still held its dev lock after closing its listener. The failed duplicate exited, the previous process finished, and the final server was restarted and repeatedly verified. The ignored restart helper now waits for complete old-process exit, not just a free port. Rapid earlier restart samples may include draining-request contention; they are not strict process-isolated benchmarks.
