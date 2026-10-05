# Record drawer development verification

Recorded on 2026-10-05 (Asia/Bangkok). This is a development verification receipt, not production rollout approval or complete populated acceptance of every module.

The generated [inventory](record-drawer-inventory.json) covers 126 tables in 82 files and all 33 module entries, with no table classified as a record/source register missing a trigger. Input, comparison and retained line tables have documented exceptions. Source coverage does not establish that every adapter has been exercised with populated development data.

## Actual browser checks

Checks used the existing human-authenticated session on [development Console](https://console-dev-erpx.jesadakorn.com/), retaining the current role and Company. No authentication bypass or production business writes were used.

| Surface | Verified behavior |
| --- | --- |
| Purchase/Sales/Quotes | Actual documents, line items, receipts/invoices, status, audit and Flow owners in the drawer; original full-page and print links |
| Products, Suppliers, Warehouses, Stock | Read detail, original inline editor, original owner relationships and stock quantities |
| Products | Reversible name edit saved and restored; detail/register agree; later whitespace-only save retained the original name; Back/Keep editing retained unsaved input; successful save cleared that form's dirty state |
| Stock counts/transfers, routines, legal | Populated owner detail and original authorized actions; routine editor remained inside the same drawer |
| Payroll | Actual run/payslip records and accounting context |
| Finance | Invoice/journal detail and financial report source accounts; owner predicates tested separately below |
| PMS Console | Actual approval evidence, settings read-first detail, inline editor, dirty traversal Keep/Discard handling; folio table/detail both display THB 1,800.00, with original reversal action |
| IAM, Usage, Agents | Assignment reader, existing role permissions disclosure, measured Company source group/individual evidence, redacted invocation and selected session detail |
| People | Existing populated person owner panel and scope breadcrumbs; no HR changes submitted |
| Platform | Populated operation metadata drawer using the original human Platform role; no provisioning or migration commands submitted |
| Executive | Existing warehouse report bucket opened exact source cohort; 54 reported stock locations produced 30 + 24 source records across two pages; selected date/currency/bucket retained; individual source opened original stock owner detail |
| Shared interaction | Purchase drawer Back/Forward/reload, keyboard Escape, focus returned to the original document link, table filter/cursor retention; mobile viewport showed full-width drawer with fixed header and independently scrolling body |

Executive tables inside fixed-height widgets were corrected to avoid flex shrinking the table to zero height. Purchase order registers retain one detail action per row. Browser screenshots and detailed observations are retained in ignored `.scratch/record-drawers/`, including `purchase-drawer-final.png` and `purchase-drawer-mobile-final.png`.

Several registers have no populated records in the authorized Acme scope, including CRM/follow-ups, RFQs, stock transit, projects/assets/maintenance/field service/service desk, planning/manufacturing, Talent/Benefits and learning. Their empty states were inspected and their owner services have isolated test coverage; this does not verify populated browser editors for those modules. Native configuration/disclosure views are not treated as proof of a populated record drawer.

PMS Front redirected to its independent Staff sign-in at `dev-pms.jesadakorn.com`. A manual sign-in handoff is pending. Populated Front interaction, including POS Front, remains unverified in the human session. Console authentication does not establish Front authentication.

## Automated checks

| Check | Result |
| --- | --- |
| Typecheck | Passed after drawer and shared-form changes |
| Lint | No errors; five existing unused-variable warnings |
| UI guard | Passed; reviewed frozen specialized boundaries retain their presentation and owner controls |
| Drawer routing, record readers, work-fetch/refresh | 36 tests passed |
| Final drawer/readers/work-fetch/refresh/group-editions/Executive run | 114 tests passed across six files |
| Product persistence/version rejection | 11 tests passed across two files |
| Group report editions/retained source journals | 9 tests passed, including direct IDs, exact PL interval and denied Company/Holding access |
| Instrument/asset-policy detail | 6 tests passed |
| Agent governance | 9 tests passed, including redaction and other-user session denial |
| Final targeted business/record/Executive/Planning/PMS run | 135 tests passed across six files |
| Public development auth smoke | 14 passed, 2 skipped because remote seeded-auth credentials were not configured |
| Full integration run | 2,365 passed, 2 failed, 3 skipped; the two failures (stale actual scanner definitions and a routing case running against a previous transform) passed targeted verification after correction; the entire suite was not rerun |
| Isolated release build | Passed using pinned Node 24.20.0/Bun 1.2.17, separate from the development output directory; verified snapshot `2026-10-04T20-16-14-071Z`, source fingerprint `0186a2bb25a89f16e5b5b16f03ae80fe966ddef79ad14328bf3dd0c42499987e`; release snapshots remain under ignored `.runtime/releases/` |

Owner tests cover direct IDs outside the current cursor, stale versions, retry/read failures, source pagination and Company/Branch/Department isolation. They complement the actual browser checks and do not replace missing populated acceptance.

Development remains detached on port 3100. Local development, public development and private production health returned HTTP 200 in end-of-work checks. One late PMS navigation hit HTTP 502 during Next development's automatic memory-threshold restart; the existing detached launcher recovered, local/public health returned HTTP 200, and the PMS page/folio drawer were verified again in the browser. No staging access or production deployment was performed. Concurrent unrelated Finance work changed the checkout after the verified isolated build (business contracts, Finance API dispatch and a zero-usage billing migration); the release manifest identifies the compiled snapshot rather than claiming an exact match to those subsequent changes.
