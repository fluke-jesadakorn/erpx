# List loading audit and implementation — 2026-09-18

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

The investigation found that the main list routes already use Next.js Server Components. The expensive part is their data loading: whole-workspace reads, options for unopened editors, per-record lookups, and client polling. Moving another component to the server alone does not bound those costs.

## Implemented

| Area | Current behavior |
| --- | --- |
| People directory | SQL search/filter and 30-row keyset pages; selected person/account resolved independently of the page; account-to-person lookup batched instead of one query per account. |
| People auxiliary data | Payroll only loads in its view. Person editor options load on opening the form. IAM Explorer and lifecycle data load only when requested. Editor/lifecycle JavaScript uses dynamic imports. |
| Payroll, Handbook | SQL pages of 30; Handbook search runs before pagination. |
| Home feed | Post-related data batched across the feed; two comments initially per post; detail starts with 20 and can fetch older comments. Total comment counts remain separate. Linked task authorization remains enforced. |
| Invoices, journal list, bank statements/transactions | SQL pages of 30. Bank lists have independent cursors. Invoice/import/matching options load when opened. Finance overview uses aggregate counts independently of its short recent lists. Print reads remain complete. |
| Commerce | Reads the active list instead of every tab; orders, products, parties, warehouses, stock and movement history use SQL pages. Editors load their options on demand. |
| Stock counts/transfers | SQL search/status filtering and 30-row pages; preparation options load on demand. |
| Work | Filters and slices the complete authorized snapshot on the server, sending 30 items. Global counts/cards still use the complete snapshot. This uses offset navigation over the merged snapshot, not SQL pagination. |
| Inbox updates | Notification candidates read in 30-row pages only on Updates; record-level visibility checks still apply, so a page can have fewer visible rows. |
| POS | Workspace reads depend on the active tab; sale history uses 30-row keyset pages. Stale refresh responses are discarded. |
| PMS | Polling no longer overlaps; settings, reports and guest views do not poll every eight seconds. Full snapshot loading remains. |
| Shared layout | Workspace choices stream behind Suspense. List routes have loading boundaries. Navigation counts no longer refetch solely because the pathname changes within the same workspace. |
| Generic records API | Opt-in `paged=1` supports scoped SQL pagination. Existing unpaged consumers are unchanged. |

`src/server/modules/work_projection/public/list-page.ts` requests 31 rows to determine whether another 30-row page exists. Ordering includes a unique ID tie-breaker. Cursors bind to scope, filters, direction and page size; they do not grant authorization. Every base query must independently restrict the actor's scope. Page navigation preserves filters; a first-page link allows recovery if a later page becomes empty after deletion.

## Remaining expensive paths, in priority order

These are identified follow-up work, **not completed optimizations**:

1. **Work source readers and navigation counts** — `src/server/modules/work_projection/public/work-items.ts`, `work-summary.ts`, and `work-cache.ts` still build a complete policy snapshot. Current pagination reduces client payload/rendering, not cold snapshot SQL. Replace each source with an authorized summary/list reader, with parity tests for independent approvals, reassignment and revocation. Do not derive global counts from 30 rows.
2. **PMS** — `src/server/modules/pms/public/pms.ts:loadPms` reads every property record, while `pmsWorkspace` also loads configuration options, documents and ledger information. Introduce section-specific read models and date-overlap queries for reservation/room calendars, then separate guest account detail and guest search. Commands must keep complete, locked domain state; filtering their snapshot would break availability and accounting invariants.
3. **People Explorer/lifecycle** — initial loading is deferred, but opening Explorer still reads the full tree; detail can still load the full lifecycle set. Fetch children by parent and selected-case details independently. Directory account metadata is still batched for the complete company on the server.
4. **Expenses and approvals** — `src/server/modules/people_payroll/public/expenses.ts:listExpenses` still reads up to 250 and performs per-record current-authority/workflow checks. Search currently happens after this cap on the list page. Build a paged candidate reader with authoritative visibility and preserve legacy workflow synchronization. Never paginate first and infer that the rest contain no pending approvals.
5. **Billing** — `src/server/modules/billing/public/billing.ts` reads all periods, costs and invoices. Add SQL aggregate totals before paging history: the UI currently derives current cost, outstanding and unbilled amounts from complete arrays. Audit history is capped at 100 without older-page navigation.
6. **POS secondary lists** — refunds, shifts, cash movements and report outputs still have fixed limits; catalog/customer options remain full lists on Sale. Add independent cursors, remote product/customer search, and explicit report date ranges. Sale history pagination does not fix those limits.
7. **Roster/calendar** — `src/server/modules/people_payroll/public/people-roster.ts` still supplies a wider date window for shift-swap options and caps request/history lists. Separate visible-week data from searchable swap targets before shrinking the range.
8. **Detail histories and legacy API clients** — payroll compensation revisions, Handbook revisions, record transitions, other audit histories and unpaged records consumers need their own pagination. Small, intentionally bounded recent-activity previews should link to a complete paged history.

After those read models stabilize, use `EXPLAIN (ANALYZE, BUFFERS)` on representative large fixtures to select indexes. No new index or production timing improvement is claimed here. Several ordering keys currently use text expressions and may still require a sort even though only one page crosses the server/client boundary.

## Verification

- People, Social and generic cursor regression suite: 54 tests passed. Includes 65-person navigation/search/deep-link isolation, equal sort keys, reverse navigation, 55-comment traversal and unauthorized post access.
- POS, staff Work, Work views/refresh/cards, organization, expense workflow and workspace regression suite: 78 tests passed.
- Two additional targeted POS/read-model tests passed: traverse 65 sales without duplicates, return to the previous page, omit unrelated tab data, and execute each new Commerce/Invoice/Bank/stock list query against isolated PostgreSQL fixtures.
- Lint, TypeScript and isolated production release build passed. The release build does not replace the running production service.
- Authenticated public browser: People first/next pages show 30; lazy Add person form opens; Bank first/next pages show 30; Invoices show 30; Stock movement history shows 30 and separate navigation; Work Team exposes next-page navigation; POS History renders successfully with the current empty fixture.
- Browser checks are functional checks, not a before/after Core Web Vitals benchmark. Earlier Work/Home timings in `work-home-performance.md` belong to a separate change and are not measurements of this change.
- Final availability checks: `http://127.0.0.1:3100/api/health` HTTP 200 and `https://erpx.jesadakorn.com/api/health` HTTP 200. The existing Node listener on 3100 has this repository as its working directory and remains running.

Public development remains `https://erpx.jesadakorn.com` on port 3100; production port 3101 is unchanged.
