# ERPX UI migration verification 2 October 2026

> Dated UI migration evidence. Current component counts are regenerated in [the component inventory](component-inventory.md); these results do not reverify later source revisions.

The UI migration introduces a shared shadcn Base UI foundation with Tailwind v4 across console, platform, authentication, marketing, POS, PMS, booking and portal source files. The seven measured production pages download less data than the captured baseline: their combined median transfer is 14.3% lower, and compressed CSS is 11.7% smaller. Typecheck, lint, source detection, isolated production measurements and print inspection pass. All 432 selected route checks are verified across the full sweep and a targeted retry. The public Home page is loaded and interactive, and all required health endpoints return HTTP 200. This record establishes the results and limits below; it does not change the scope or release gates in the [current production plan](../production-plan.md).

Work proceeded concurrently with another task that changed Home, Work, People and navigation. The initial workspace snapshot preserved that task's existing changes. Later snapshots include further concurrent changes, so these comparisons describe the resulting workspace and do not isolate the cause of every size or timing difference. No staging or production deployment forms part of this migration.

## Shared components and retained behavior

The shared [UI components](../../src/components/ui) provide `Button`, `Field`, `TextInput`, `Select`, `TextArea`, `Table`, `TableFrame` and controlled `Dialog`. The automated control pass replaced 5,678 native control occurrences in 283 source files; that count measures source migration, not completed business journeys. A final source audit found no native button, input, select, textarea, dialog or table JSX outside the central UI components. The [generated inventory](component-inventory.md) covers 192 page routes.

`Button` uses Base UI with centralized primary, secondary, ghost, danger and plain variants. Native form adapters retain HTML names, values, validation, events, refs and server rendering. Existing implicit submit buttons receive an explicit `type="submit"`; ordinary actions use `type="button"`. This preserves `FormData`, submitter values and Enter submission. `Field` uses a native wrapping label, and `TableFrame` provides a keyboard-focusable, named scroll region.

The controlled Base UI dialog owns focus trapping, focus restoration and scroll locking. It calls the existing close handler while retaining control of open state, allowing dirty-draft confirmation to keep the editor open. Busy dialogs can disable dismissal. Calendar forms, roster, messaging, move trays and destination choosers use the shared dialog while retaining their caller contracts.

Console and front shells now own their page's single `main` landmark through [MainContent](../../src/components/ui/main.tsx). Recovery and access states render as sections when embedded in that landmark, while standalone recovery pages retain their own main. This fixes absent landmarks without creating nested main regions.

The [token sheet](../../src/components/ui/tokens.css) consolidates 57 existing semantic tokens under `--ui-*` and preserves light, dark and system appearance. Default controls are 44px, POS controls 48px, and explicitly compact controls 36px. ERPX blue, thin borders and restrained shadows are shared centrally. CSS Modules remain for calendar geometry, organization charts, executive canvas, specialized page layouts and print. Geometry handles keep their original hit areas through the layout size option. The responsive organization detail panel and non-modal account and mention popovers retain their specialized interaction code.

## Pinned sources and download changes

All dependency versions in [package.json](../../package.json) are exact, with Bun 1.2.17 and Node.js 24.20.0 pinned. Relevant versions are Next.js 16.3.4, React 19.2.8, Tailwind and its PostCSS plugin 4.3.3, Base UI 1.8.0, class-variance-authority 0.7.1, clsx 2.1.1 and tailwind-merge 3.7.0. The [shadcn source lock](../../vendor/shadcn/source-lock.json) retains the MIT-licensed `base-nova` button and dialog registry snapshots, their SHA-256 checksums and upstream commit `d75a96ab781f3d659be1ad287347d5887ce9f2fc`.

All 36 runtime and 20 development dependency pins match installed versions. Both vendor snapshot checksums, the upstream commit format and MIT license were verified. Stale root dependency ranges in Bun lock metadata were corrected to the exact manifest versions; a Bun 1.2.17 frozen install then passed with 911 installs and no lock changes.

Tailwind disables automatic source discovery and explicitly scans only `src` through `source(none)` and `@source ".."` in [globals.css](../../src/app/globals.css). The [source-detection verification](../../scripts/ui/verify-source-detection.mjs) passed: minified global CSS was byte-identical between the workspace and a copied source tree without Git metadata, and sentinel utility classes placed in tests, docs and vendor files were excluded. The existing Next.js CSS chunking configuration remains unchanged. Isolated release builds succeeded without sharing the live development output directory.

Browser Thai text uses a 44,912-byte WOFF2 subset. A separate 63,912-byte subset preserves the original font's other scripts for stored rich text that explicitly selects Noto Sans Thai. Unicode ranges load each subset when required. Geist Mono is no longer preloaded; monospace uses system fonts. The original 218,652-byte TTF remains available for PDF generation. [The font preparation script](../../scripts/ui/prepare-thai-font.py) preserves glyph coverage and disables timestamp recalculation; its [requirements file](../../scripts/ui/font-requirements.txt) pins fonttools 4.63.0 and Brotli 1.2.0. The font's OFL license is included under `public/fonts/`.

Two font regenerations produced byte-identical WOFF2 files. The PDF TTF matches the captured baseline SHA-256 `5a1c559bb539583c8a1fd99d1c5b9491e5e14478c9cd2bd0970d5c3096cc9ef8`.

Pure editor state helpers moved out of the Tiptap component, and compose/edit dynamically load Tiptap on opening. Authenticated attachment images use the existing preview variant, reserved dimensions and lazy loading while retaining the original download link and authorization path. POS thumbnails load near the viewport through the authenticated request helper, with cancellation and object URL cleanup.

## Production measurements

Three isolated production builds represent the captured baseline, download optimizations before the broad migration, and the completed UI including landmark and print changes. The page measurements use the successful isolated release `2026-10-02T11-10-58-662Z`. Both later verified releases, including the latest pinned build `2026-10-02T12-12-28-494Z`, have identical paths, sizes and SHA-256 checksums for all 282 checked assets: 279 emitted static files, the original TTF and both public Thai WOFF2 fonts. Their follow-up changes concern three other detail-page error states and dependency lock metadata; the seven measured routes are unchanged. Asset totals therefore match, while LCP, CLS and transfer samples remain observations of the measured release rather than newly collected timings for the later builds.

[Asset measurement](../../scripts/ui/measure-assets.mjs) totals every emitted static CSS/JS file and font file; these totals are available asset inventory, not bytes loaded by one page. Each cell below is gzip / Brotli KiB, with 1 KiB = 1,024 bytes.

| Asset inventory | Baseline | Optimized | Completed UI |
| --- | ---: | ---: | ---: |
| CSS | 153.8 / 129.8 | 135.1 / 115.0 | 135.7 / 115.5 |
| JavaScript | 2,023.2 / 1,734.4 | 2,024.8 / 1,736.1 | 2,043.3 / 1,751.2 |
| WOFF2 fonts | 143.3 / 143.0 | 118.1 / 118.0 | 180.6 / 180.5 |
| TTF for PDF | 130.1 / 109.1 | 130.1 / 109.1 | 130.1 / 109.1 |

The completed build has 38 CSS files, 236 JavaScript files, seven WOFF2 files and one TTF. CSS gzip falls from 157,470 to 138,998 bytes; total JavaScript gzip rises from 2,071,719 to 2,092,294 bytes, or 0.99%. The larger WOFF2 inventory includes the additional script subset, which loads only when required. Moving editor code to a later load reduces initial page transfer without requiring the total emitted JavaScript inventory to shrink.

[Page measurement](../../scripts/ui/measure-page-loads.mjs) used three cold Chromium contexts per route, disabled cache, a 1440 × 900 viewport, the normal sign-in endpoint and an isolated production server on loopback with development data. Each observation waited six seconds after DOM content loaded. All 63 measured navigations returned HTTP 200 at the intended path. Transfer is Chromium's encoded completed-response total during that window, including document, scripts, styles, images and data requests. Each cell below is median transfer KiB / median request count.

| Route | Baseline | Optimized | Completed UI | Transfer change from baseline |
| --- | ---: | ---: | ---: | ---: |
| `/login` | 322.1 / 23 | 279.6 / 21 | 306.2 / 24 | −4.9% |
| `/holdings/acme/home` | 877.0 / 89 | 599.0 / 86 | 701.1 / 85 | −20.1% |
| `/holdings/acme/work` | 657.3 / 46 | 485.5 / 44 | 568.2 / 46 | −13.5% |
| `/holdings/acme/people` | 726.5 / 49 | 554.8 / 46 | 635.7 / 49 | −12.5% |
| `/holdings/acme/finance` | 657.9 / 46 | 484.6 / 43 | 565.3 / 46 | −14.1% |
| `/holdings/acme/pos` | 643.4 / 44 | 470.9 / 41 | 551.1 / 44 | −14.3% |
| `/holdings/acme/pms` | 642.7 / 44 | 471.0 / 41 | 551.1 / 44 | −14.3% |

The sum of the seven route medians is 4,635,641 bytes at baseline and 3,971,899 bytes after migration, a 14.32% reduction. Each sampled page remains below its baseline. Request count increases by one on login, falls by four on Home and returns to baseline on the other routes. POS and PMS measurements cover their console entry pages, not the complete front applications or catalog.

The following cells show median Largest Contentful Paint in milliseconds / median Cumulative Layout Shift. CLS has no unit.

| Route | Baseline | Optimized | Completed UI |
| --- | ---: | ---: | ---: |
| Login | 36 / 0.0000 | 32 / 0.0000 | 32 / 0.0000 |
| Home | 216 / 0.0153 | 260 / 0.0153 | 320 / 0.0155 |
| Work | 140 / 0.0011 | 196 / 0.0013 | 228 / 0.0015 |
| People | 120 / 0.0010 | 128 / 0.0010 | 124 / 0.0010 |
| Finance | 120 / 0.0010 | 116 / 0.0012 | 104 / 0.0012 |
| POS | 116 / 0.0010 | 120 / 0.0012 | 100 / 0.0010 |
| PMS | 96 / 0.0012 | 116 / 0.0010 | 96 / 0.0012 |

These runs establish smaller sampled downloads and mixed LCP results. Median LCP increased on Home, Work and People, improved on login, Finance and POS, and remained unchanged on PMS. Three local runs with simultaneous work and changing development data cannot establish public-network performance or attribute timing changes to a specific change. Median CLS remains at or below 0.0155; the largest individual completed Home sample is 0.0473. These are six-second observations rather than full-session field measurements.

## Verification results

| Check | Observed result | Scope and limits |
| --- | --- | --- |
| Typecheck | `bun run typecheck` passed | Final source including the detail-page, lock metadata and test-adapter follow-ups. |
| Lint | 0 errors and 4 existing warnings | Remaining warnings concern service navigation, PMS and two unrelated browser suites. |
| Isolated release builds | Baseline, optimized, migrated and final verified builds succeeded | All seven sampled routes returned HTTP 200 at the intended path in 21 runs; later release client assets match exactly. |
| Dependency and source pins | Passed | 56 direct pins match installed versions, vendor checksums and license verified, frozen Bun install passed without changes. |
| Tailwind source detection | Passed after the final print change | Workspace and isolated source produce identical 42,283-byte minified global CSS; tests, docs and vendor sentinel utilities are excluded. |
| Font reproducibility | Passed | Two WOFF2 regenerations match byte-for-byte; PDF TTF matches baseline checksum. |
| UI foundation and contracts | 10 passed | Desktop/mobile, 390–1440px widths, light/dark/system, Thai form values, Enter submit, submitter, non-submit actions, dirty confirmation, focus trap/restoration, keyboard and reduced motion. |
| Migrated module tests | 278 passed across 16 files | Editor helpers, social rich content, calendar, organization drafts/chart, POS, PMS, booking, attachment behavior and supporting registries. |
| PDF and attachment module tests | 20 passed across 3 files | Legal PDF creation/release routes and attachment formats. |
| Additional legal suite | 33 passed in 1 file | Separate full `tests/integration/legal/legal.test.ts` run. |
| Rendered print and PDF | Invoice HTTP 200, two A4 pages inspected | Final rerender is readable without clipping, including the skip-link fix; application chrome is hidden. The unchanged Thai TTF sample is also readable. |
| Calendar and organization browser tests | 29 passed, 7 skipped across the suite and targeted retry | All 20 calendar cases passed. Nine organization cases passed, including responsive, keyboard/touch moves, private drafts, Undo, review, retries, restricted HR reads and the unchanged working-time/roster/compensation case on frozen source. Mobile exclusions are explicit suite skips. |
| Complete route matrix | All 432 selected cases verified across sweep and retry | The full sweep passed 416 cases; all 16 previously failed cases passed the targeted retry in 1.2 minutes. This is not one uninterrupted 432-pass run. All 192 route templates are covered on desktop and mobile with no recorded horizontal overflow. |
| Public console browser | Home, Work, People, Finance and UI gallery inspected | Actual session at [the development console](https://console-dev-erpx.jesadakorn.com/); desktop pages and mobile People checked, Finance in dark mode, compose opened the lazy editor, and dirty dialog behavior inspected. Available previews were observed earlier; unavailable/quarantined assets show the fallback. Home's main content and actions are fully hydrated. |
| Public marketing browser | Desktop system and 390px mobile light inspected | [Development marketing](https://dev-erpx.jesadakorn.com/); system appearance restored afterward. |
| Public PMS browser | Authenticated holding page inspected | [Development PMS](https://dev-pms.jesadakorn.com/acme/acme-co/holding) used the existing session, with one main landmark, Thai feed and no page overflow. |
| Public POS browser | Login inspected; sale screen unverified | [Development POS login](https://dev-pos.jesadakorn.com/login) has one main landmark and no page overflow. The sale screen requires POS login. |
| Environment availability | Local dev, public dev and private production HTTP 200 | Development remains detached on port 3100 with pinned Node/Bun; PostgreSQL on 5432 remains available. |

The organization case “working time, roster requirements and effective compensation use their existing workflows” initially timed out while finding the weekly roster checkbox after compensation save and position navigation. The captured pre-migration source reproduced that timeout. The unchanged case later passed in 16.9 seconds with frozen development source, bringing the aggregate suite result to 29 passed and seven explicit mobile skips. No assertion was removed to obtain that pass.

The route sweep used disposable fixture databases and normal browser authentication. The fixture server reported degraded readiness because its environment did not enable all attachment-scanner and job readiness settings; that status was distinct from the workspace development server. HTTP success, an accessible page landmark and visible content were checked separately. Fresh warmed checks of expense and handbook missing-record fallbacks and the Learn page passed without browser errors. Installed Next.js can return HTTP 200 for a streamed not-found page, so its visible fallback was verified separately from HTTP status. Missing-record and permission states establish only their stated access behavior, not a populated detail journey.

The source inventory now classifies public marketing correctly. Legacy platform routes are verified against their existing redirect to `/holdings/erpx/manage-erpx` and the visible HTTP 403 permission panel for users without platform access. Three detail pages now present expected missing-record recovery. The final inventory fingerprint is `7b5b89338fd089409ce79859ca4eb74a3a40195974f7fc67332439e6068c02a0`. Before fixture cleanup, all 1,368 source files in the frozen browser app matched the current workspace byte-for-byte.

The full frozen-source sweep finished with 416 passes and 16 failures in 39.1 minutes. Its 430 page evidence rows cover 192 desktop and 192 mobile route templates with no recorded horizontal overflow. Eight failures followed restarts or temporary loader errors in the owned webpack test server; eight used obsolete expectations for access recovery. After restoring the isolated port 3104 server with 8GB memory headroom and correcting the adapters, all 16 failed cases passed a targeted retry in 1.2 minutes. The retry produced 16 evidence rows without navigation or browser errors, and Playwright's final failed-test list is empty. The aggregate verifies all 432 selected cases across two runs.

Reconciliation replaces the 16 original failed page captures with their retry results, leaving 430 browser captures plus two inventory guards. All 384 route-template/viewport pairs are covered. The reconciled results contain no remaining navigation errors, browser errors, HTTP 500 responses, generic failure screens, stuck loading states, missing or duplicated main landmarks, or horizontal overflow. Front results are matched by host as well as path to distinguish PMS and POS routes.

The adapters recognize the canonical forbidden panel as a permission state and the employee People/settings redirect as the read-only explorer, asserting that HR settings and “+ Add” controls are absent. HTTP 500, navigation-error, browser-error and permission assertions remain in place. A new guard also requires signed-in route tests to stay out of the login page, so an authentication redirect cannot count as a successful business page.

The final rendered invoice export hides application chrome and the skip-to-content link while retaining its document layout. Both A4 pages were visually inspected as clear and unclipped after the fix. The Thai sample used the unchanged TTF through the legal PDF font adapter.

The long-running development server later produced Home response delays of 2.6–3.6 minutes, database pool/timer delays and an RSC stream closure. Only the verified workspace development service on port 3100 was restarted, using detached Bun 1.2.17 and Node.js 24.20.0 with logs and PID information under `.scratch/dev/`. Private production and PostgreSQL were preserved.

The public Home page was then inspected again in the actual browser: the “Today” heading, one main landmark, Needs You, My Day content and the loaded Daily Plan state were present. Navigation actions were enabled, with no error heading. The final check recorded document width 1,713px within a 1,728px viewport, confirming no horizontal overflow.

The final combined health check at 12:31:12 UTC returned HTTP 200 from local development at `http://127.0.0.1:3100/api/health` in 0.011 seconds, [public development](https://console-dev-erpx.jesadakorn.com/api/health) in 0.105 seconds and private production at `http://127.0.0.1:3101/api/health` in 0.014 seconds. The detached Bun supervisor and Node listener remained alive for more than 28 minutes after startup, with the listener working directory matching the workspace. PostgreSQL on port 5432 remained available. These checks establish end-of-work availability, not an uptime guarantee.

The isolated simulation was destroyed after verification: its six owned fixture databases and exact run directory were removed through the simulation helper. Temporary ports 3104, 3105 and 3106 have no remaining listeners. Development on 3100, private production and shared PostgreSQL remain available.

## Evidence and verification limits

Raw measurements and logs remain local and ignored under `.scratch/ui-refactor/`: `baseline-assets.json`, `optimized-assets.json`, `completed-assets.json`, `verified-assets.json`, the corresponding `*-pages.json`, `ui-final-e2e.log`, `module-tests.log`, `pdf-module-tests.log`, `legal-pdf-tests.log`, `business-ui-final-e2e.log`, `working-time-baseline.log`, `route-audit-working.log`, `source-detection-complete.log`, `fonts-reproducible.log`, `typecheck-final.log`, `lint-final.log`, `release-pinned-verified.log`, `print-pdf-complete.log`, `route-sweep-verified.log`, `route-results-verified.jsonl`, `route-sweep-stable-retry.log`, `route-results-stable-retry.jsonl`, `route-reconciliation-final.json`, `source-fingerprint-final.json`, `simulation-cleanup-final.log` and `end-health-final.json`. Those local files contain the measured build paths and individual samples. They are not durable release evidence after checkout. The [route inventory](../testing/uat-route-inventory.md) is a source coverage map, not browser acceptance.

Route coverage includes the permission and missing-record states described above; it does not claim every populated business journey has been accepted. The authenticated public POS sale screen remains unverified because it requires POS login. Two Home preview requests returned HTTP 423 through the unchanged [attachment availability gate](../../src/server/modules/files/public/attachments/service.ts), which rejects content access when `source_state !== "available"`. Their “Image preview unavailable” fallback preserves the original download link and reserved dimensions. This is the expected blocked-asset state; verification does not establish availability of every post attachment. LCP results remain mixed under local measurement conditions. Staging and production deployment remain outside this work's authorized scope.
