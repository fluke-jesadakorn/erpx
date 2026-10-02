# PMS Today, Finance trace and cutover preview — development evidence

Checked on 2026-09-30 in this workspace with Bun 1.2.17, Node 24.20.0 and the existing development server on 3100. Owner coordinates initial rehearsal as selected by the user. Production activation remains pending.

| Check | Actual result | Local evidence |
| --- | --- | --- |
| Full integration | 223 files passed, 2 skipped; 1,660 tests passed, 5 skipped; exit 0 | `.scratch/dev/pms-integration-verified.log` |
| Final task/access checks | 112 tests passed: PMS, bounded reads, Handover, navigation | `.scratch/dev/pms-final-task-tests.log` |
| Final event/source checks | 96 tests passed: PMS, Social, Handover | `.scratch/dev/pms-final-event-tests.log` |
| Typecheck | Exit 0 | `.scratch/dev/pms-typecheck.log` |
| Full lint | Exit 0; zero errors, four existing unused-variable warnings | `.scratch/dev/pms-full-lint.log` |
| Isolated release build | Exit 0; `.runtime/releases/2026-09-30T14-23-44-015Z`; no deployment | `.scratch/dev/pms-release-build.log` |
| PMS migration receipts | `0908_pms_social_sources`, `0910_pms_cutover_import`, `0911_pms_opening_trace` applied in all three development Holdings | `.scratch/dev/pms-migration-check.json` |

The full suite preceded the final refund-source mapping adjustment; the final focused run verifies that adjustment and its shared Social/Handover boundary. Fixture regressions from new posted-journal protections were corrected by adding lines while draft and then posting. No accounting guard was disabled. PostgreSQL query-concurrency deprecation warnings remain outside this change.

Integration covers hotel/Finance journeys, concurrent last-room booking, replay/conflict and stale versions, revoked/cross-scope access, independent bank evidence and duplicate use, partial refunds/corrections, Night Audit recovery, shared source/post privacy and acknowledgement, outbox deduplication/coalescing and notifications. Cutover checks include preview errors without hotel writes, independent approval, stale versions, exact opening GL/subledger links, atomic replay, partial collection/refund and two consecutive Night Audits without historical revenue reposting.

Authenticated browser inspection used the normal existing Owner session at [PMS Today](https://dev-pms.jesadakorn.com/acme/acme-co/holding) and [Finance hotels](https://console-dev-erpx.jesadakorn.com/holdings/acme/finance/hotels). Today showed the shared ERPX feed, hotel/business-date overview, empty tasks, disabled empty handover publication and company comparison. Light/dark appearance and Thai/English were inspected. A 390×844 mobile viewport override reported equal 375 px document/client widths; the header branding/controls no longer overlap. Language, appearance and viewport were restored. Screenshots: `.scratch/dev/pms-today-light.png`, `pms-today-mobile-th.png`, `pms-finance-hotels.png`.

The browser saved one fictional `browser-rehearsal-preview-only` batch with a missing room-type reference and displayed the exact row error. A database read confirmed state `preview`, hotel version still 59 and zero persisted fictional guests. No import approval, hotel records, cash or revenue were applied. Evidence: `.scratch/dev/pms-browser-preview-db-check.json`, `pms-import-preview.png`.

Stale ClamAV definitions were refreshed and the scanner reloaded. Development jobs were rebuilt/restarted for the event adapter. Final checks at 21:37 Asia/Bangkok returned HTTP 200 from `http://127.0.0.1:3100/api/health`, `https://console-dev-erpx.jesadakorn.com/api/health` and `http://127.0.0.1:3101/api/health`. Development scanner, preview and jobs were ready, with a fresh worker heartbeat after restart. Responses are retained in `.scratch/dev/pms-health-{local,public,private}.json`; PostgreSQL and the detached development server remain available. The authenticated Today page was reloaded and verified at 21:35, with final screenshot `.scratch/dev/pms-today-final.png`. Staging was not accessed.

Attachment audit was read-only: unknown ownership, unavailable sources, cross-company links, missing objects/ledger and quota delta were zero, but h001 reported 271 orphan objects. No frozen checksum final pass or global gate opening was performed. Scanner/jobs readiness does not establish photo-upload browser acceptance. Evidence: `.scratch/dev/pms-attachment-audit.json`, `pms-attachment-audit-summary.json`.

Real-company acceptance still needs actual source data and cutover date, Finance acceptance of chart/tax/bank/document configuration and balances, hotel signoff, and browser rehearsal with operating/independent reviewer roles after attachment readiness. No real-company cutover or production activation was performed.
