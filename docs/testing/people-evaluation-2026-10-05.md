# People evaluation development verification — 2026-10-05

The [People owner contract](../contracts/next/people-evaluation.md) implements Skills, Performance & Compensation in People, with [primary-source research](workforce-compensation-research-2026-10-05.md). Product weights and thresholds require Company confirmation. This is development verification; it does not record a production or staging deployment.

## Delivered behavior

Annual cycles pin confirmed policy versions. Skill behaviors, role profiles and rating references retain exact versions; revising a rubric cannot qualify an old score against a new target rubric. Recorded position/level changes require distinct role periods, and revised draft dates and workflow actors remain in immutable history. Unknown skills/performance and missing compensation comparison bases remain explicit.

The shared filtered dataset powers individuals, the paginated comparison table, selected-person skill heatmap and all three quadrant charts. Graph pagination is independent of table presentation. Long editors, source evidence, criteria and wage review use right drawers.

Company criteria and imported CSV benchmarks have typed validation, version/reason history, source/date retention and explicit confirmation. Approved wages use dated HR FTE/hour/day attestations. Independent result/pay approvals, immutable original/proposed pay snapshots, optimistic versions and command replay guards protect the next-period atomic interval replacement. Withdrawal preserves an unapproved proposal for renewed human review. Proposal and approval/withdrawal reasons remain separate. Issued payroll remains unchanged.

Read, write, graph, evidence/file and AI paths enforce Company/team/IAM boundaries. Compensation and inferred salary fields are absent without the explicit permission. AI uses existing provider/jobs infrastructure, validates employee-specific citations, rechecks authority/evidence before and after generation, and leaves manual review available on failure or staleness. Provider responses in tests are synthetic; no employee data was sent to an external AI provider during verification.

## Browser checks

The actual [development evaluation page](https://console-dev-erpx.jesadakorn.com/holdings/acme/people/evaluation) was inspected using the existing authenticated Chrome session, including after recovery from the disk-space interruption. The authorized Company contains 128 matching employees, with no evaluation criteria/cycle confirmed by this work. Empty graph and configuration states correctly request HR setup. No employee scores, pay recommendations or approvals were submitted through that session.

A temporary, development-only, authenticated read-only preview used [36 fictional employees](../../fixtures/people-evaluation/browser.ts) to exercise the actual console components without persisting business records. The preview page/API were removed before the final release build.

- Comparison table: 25 rows on page 1 and 11 on page 2.
- All three graphs: 35 complete pairs from all 36 matching rows, plus one clearly identified missing pair, including employees beyond the current table page.
- Skills graph: reference 80%/3; compensation graph: 100%/3; work graph: responsibility bubble size and switchable responsibility X axis.
- A keyboard-activated graph point opened the individual drawer, retained evidence and history. The evidence inspector displayed the frozen source/description and evaluator/date.
- Two selected employees produced the expected demonstrated/required heatmap and critical-skill markers.
- Scored editor retained integer component scores and pinned skill behaviors. Goals, outcomes, actual work and development fields were inspected.
- At 390×844, drawers filled the available viewport, their header stayed at y=1 while the body scrolled, and there was no horizontal page overflow. The graph scrolled inside its own container (307px frame / 720px graph), with distinct responsibility radii. Viewport overrides were reset.
- Final actual-page browser console had no errors. The restored session required no authentication bypass.

Screenshots are retained locally under ignored `.scratch/people-evaluation/`: `dev-module-final.png`, `dev-mobile-drawer-final.png`, `desktop-populated-graph.png`, `mobile-editor.png` and `mobile-graph.png`. Populated screenshots are explicitly synthetic.

## Automated verification

Pinned Bun 1.2.17 and Node 24.20.0 were used. Final command receipts are retained in ignored `.scratch/evaluation-*.log`.

| Check | Result |
| --- | --- |
| People evaluation, payroll, exact attachment policies, sensitive file access and navigation integration | 115 cases passed across five files; includes 15 evaluation cases |
| Full integration run | 2,476 passed, three skipped; two exact attachment-inventory expectations initially failed after adding the new purpose and were corrected, then passed in the targeted rerun |
| Typecheck | Passed |
| Lint | No errors; four existing warnings outside this module |
| Documentation links | Passed: 238 documents and 1,189 links |
| UI guard | Existing frozen PMS/booking/Executive changes and three Finance FormDisclosure appearance omissions prevent the workspace-wide guard from passing; no evaluation-owned appearance violation reported |
| Isolated release build | Passed: `.runtime/releases/2026-10-05T13-04-54-993Z`, Node 24.20.0 standalone with web/jobs bundles |

Evaluation cases cover independent dimensions, unknown/critical evidence, exact rubric versions, monthly/daily/hourly/FTE normalization, missing/mismatched/expired market data, CSV parsing and atomic import, team/Company/revoked access, protected evidence, role-period splitting, independent confirmation and history actors, duplicate/stale drafts, self/generic approval rejection, changed wage/hours, component money snapshots, proposal/review reasons, issued payroll retention, valid/invalid AI citations, changed evidence and revocation during AI processing.

## Environment availability and recovery

The first isolated build encountered ENOSPC. Its failed temporary build, two old generated build working copies, a completed duplicate working copy and the development Webpack cache were removed; installed release artifacts and production/PostgreSQL data were preserved. Development web/jobs were restored in detached processes with logs/PIDs in ignored `.scratch/dev/evaluation-{dev,jobs}.{log,pid}`. The build used isolated `.runtime/build` output throughout. No staging visit, health check, migration, route change or deployment was performed.

Final local dev `3100`, public dev `console-dev-erpx.jesadakorn.com/api/health`, and private production `3101` health checks returned HTTP 200. PostgreSQL remained listening on `127.0.0.1:5432`. Dev web/jobs survived their startup command and were rechecked afterward. Final generated build working copies were removed after their standalone release manifests were verified; installed/release artifacts were retained.
