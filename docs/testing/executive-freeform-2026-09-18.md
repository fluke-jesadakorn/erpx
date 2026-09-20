# Executive freeform widgets — 2026-09-18

The user's clarification keeps the entire project on Next.js. AI-generated HTML/CSS applies only inside saved Executive report widgets. No Rust/Dioxus migration was performed.

## Delivered

- Real HTML and CSS fields in each custom widget, retained in report revisions, named shares, duplication and restoration. Authors can inspect/edit the template in Widget settings. Report appearance also supports typography, corners and surface presets.
- Live bindings for title, formatted total, scope, definition, snapshot time, table rows and bar/line/area/column/donut charts. Each widget binds to one authorized metric and grouping. Different widgets can query different groupings of the same metric without overwriting one another.
- Native charts now have coordinated colors, axes, readable legends, SVG tooltips/focus interactions, responsive layouts and explicit empty states. Large categorical series combine the remainder rather than silently omitting it from a donut.
- Mouse dragging on left/right/bottom edges and the corner changes saved width/height. Width uses a responsive 12-column grid; height is 240–900 px. One completed gesture is one undo step. Pointer cancellation restores the original size. Keyboard arrows, width presets and a numeric height field provide alternatives.
- Every AI generation receives current permitted organization nodes/positions, module descriptions, metric definitions and exact aggregate snapshots. Current company, role, branch/department, source permissions and executive eligibility are checked server-side. The model has no elevated service identity or raw database connection.
- Twelve metrics now expose 30 valid grouping combinations, including headcount by branch/department/level, orders by month/status/branch and finance by month/status. Headcount assigns a person with multiple effective engagements to one deterministic group so charts reconcile to the total.
- The chat includes an inspectable AI data context panel. For the actual holding-owner session it showed 12 metrics, 30 groupings, 22 organization nodes and 107 positions. Narrower roles receive only their own permitted projection; lack of People access removes organization detail and headcount entirely.
- Report generation allows up to 90 seconds; the first real freeform inference exceeded the former 25-second limit, then a manually initiated retry succeeded. Failed jobs remain visible with Edit and retry. There is no silent paid retry.

## HTML isolation and data boundary

The server sanitizes generated markup using `sanitize-html` 2.17.7, with an explicit tag/attribute list. The browser renders it in an iframe with an empty sandbox, opaque origin, no referrer and a restrictive document CSP. Generated code cannot run scripts, navigate, access the parent document, send requests, load remote assets or carry provider credentials. Application-owned APIs authorize and fetch data; only escaped values and chart markup enter the document. CSS is confined to that widget document.

The sanitizer project documents its tag/attribute model in its [official repository](https://github.com/apostrophecms/apostrophe/tree/main/packages/sanitize-html). The installed package's README was also inspected. Sandbox and CSP remain independent controls beyond sanitation.

A bounded 128-document cache keeps unchanged iframe content stable across heartbeats, preserving reading/scroll state. It is reached only after a freshly authorized source query; denied reads cannot fall back to cached content. The custom snapshot timestamp describes that rendered snapshot, while the outer source footer reports the latest successful read.

## Browser evidence

A real OpenCode Go `muse-spark-1.3-contributor` request added **Active Headcount by Branch — Editorial Composition** to the existing private report. The model supplied an ivory editorial HTML/CSS layout with donut, bars, table and live total. The actual API returned 116 people: Bangkok Operations 56, New York Commercial 56, and Unassigned 4. These were read from permitted data, not invented sample points.

The widget was added through the UI. Mouse border dragging changed height from 560 to 644 px and width from 12 to 10 columns; the Full preset restored 12 columns. Autosave reached revision 15. The rendered iframe and real graphs were inspected through the authenticated public hostname.

Reloading the public report retained all six widgets, saved revision 15, the custom HTML/CSS, 12-column width and 644 px height. At a 390 px mobile viewport, the document measured 375 px wide and widgets stacked without horizontal page overflow. The temporary viewport override was reset to the normal desktop size. The report remains private.

## Final verification

- 96 tests passed across `executive-canvas`, `ai-protocols`, `workplace-navigation` and `workspace-assistant`, including role/scope isolation, headcount reconciliation, template persistence, HTML isolation and cache authorization.
- `bun run lint`, `bun run typecheck` and `git diff --check` passed.
- The isolated `bun run deploy:build` passed and produced `.runtime/releases/2026-09-18T01-16-30-186Z`; the running development output was not used for the build.
- Both `http://127.0.0.1:3100/api/health` and `https://erpx.jesadakorn.com/api/health` returned HTTP 200. The existing detached ERPX development process (PID 78889, workspace working directory) remains listening on 3100. Production on 3101 was left intact.
- The authenticated [public report](https://erpx.jesadakorn.com/holdings/acme/executive/reports/report-e407b145-59e7-458a-ab73-08a6a7fc5162) reopened successfully, showed Live data and rendered the saved generated charts.

## Coverage limits

This is an authorized analytical projection, not unrestricted access to every table or all companies. All currently supported and permitted adapters are included. Module descriptions do not manufacture analytics for unsupported domains. Payroll, profit, occupancy, forecasting and additional source adapters remain unavailable until their authorized metric contracts are implemented. Organization context is bounded at 500 nodes/positions with a truncation flag; AI row samples are limited to 20 per grouping with exact totals and an explicit truncation flag. Saved custom widgets contain HTML/CSS and live bindings; arbitrary generated JavaScript and arbitrary network requests are not supported.

## Follow-up: adaptive light and dark widgets

Report color mode now supports **Match workspace**, **Light** and **Dark** independently of the Midnight/Ivory/Ocean palette. Match workspace responds immediately to the viewer's appearance preference and to OS changes when the workspace uses System. The mode is saved with the report and pinned into named shares. Existing definitions without this field use Match workspace.

Generated documents expose semantic `--widget-*` surface/text/border/shadow/status tokens, `--accent` and seven chart colors. The designer prompt requires new/refined HTML to use these tokens or paired `light-dark()` colors. Both native and custom charts use darker accents on light backgrounds and brighter accents on dark backgrounds. Changing mode does not regenerate content or query the model. The iframe retains its sandbox and source authorization, and unchanged live data retains the same document/cache entry.

Browser testing caught the CSS compiler lowering `color-scheme: inherit` to `normal`. The report now resolves the workspace mode explicitly, and the iframe receives inline inheritance so the embedded document follows the actual selected scheme. This behavior follows the [CSS Color Adjustment specification for embedded documents](https://drafts.csswg.org/css-color-adjust-1/#color-scheme-prop) and [paired light/dark colors](https://drafts.csswg.org/css-color-5/#light-dark).

The user's report had independently advanced to revision 23 with one widget before this follow-up. Its current contents and 900 px height were preserved. An actual Muse Spark refinement first returned an invalid proposal (no saved changes), then a shorter manually submitted request succeeded. Applying that proposal updated the existing widget, without adding a duplicate. Its ivory/charcoal surfaces, readable labels, donut and bars were visually verified in both modes. The same live total remained 116 with groups 56/56/4.

Explicit Light and Dark overrides were checked while the workspace stayed light. Match workspace was then checked by switching workspace appearance to dark and back to light: the computed canvas and iframe schemes followed immediately. The report reopened on Match workspace at revision 29, with the latest workspace preference restored to light and the 900 px widget height retained. Previously saved fixed-color templates elsewhere require refinement to adopt the theme tokens; saved HTML/CSS is not silently rewritten.

The Executive suite passes 28 tests, including persistence and immutable share projection of color mode, adaptive document/chart tokens, accent-aware caching and unchanged access controls.

Final lint, typecheck and diff checks passed. The isolated build produced `.runtime/releases/2026-09-18T01-31-09-691Z`. Local and public health endpoints both returned HTTP 200, the authenticated report reopened successfully, and the existing detached development process remained on 3100.
