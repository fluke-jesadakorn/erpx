# Executive AI Live Canvas — implementation and acceptance

Implemented on 2026-09-18 in the existing Next.js workspace. The public hostname continues to serve development on port 3100; the production service on 3101 was not replaced.

Follow-up: [Freeform HTML/CSS, border resizing and organization-aware AI](executive-freeform-2026-09-18.md) extends this initial release. Statements below describe the initial checkpoint.

## Delivered journey

Executive appears in the sidebar with one feature, **AI Live Canvas**. The library separates owned and named shared reports. Creating a report makes it private; no peer executive can discover its contents merely by seniority.

Authors describe a report in a private report-scoped chat. The selected company AI connection generates a validated proposal. Users can apply the design, drag individual proposed widgets onto the canvas, or use Add to canvas. The assistant floats and can be dragged or docked on the right; mobile uses a bottom sheet. Text/Markdown/CSV briefs can be attached for design extraction. Generated labels, suggestions and guidance are English.

The canvas supports KPI, bar, line, donut and table widgets; three themes; five accents; density; sections/tabs; width presets; duplication; movement; title/section editing; undo/redo; autosave; optimistic concurrency; saved revision restoration; and presentation mode. Source definitions, scope, exact aggregate values and freshness are supplied by authorized APIs. Empty results are explicitly distinguished from unavailable metrics.

The model generates a strict report definition rendered as HTML/CSS and SVG by the application. It does not provide executable JavaScript, raw SQL, arbitrary external fetch URLs or invented business values. Live values refresh independently of AI inference.

## Access and sharing

- Active Level 1/2 engagements plus effective Executive permissions allow authoring; holding owners are eligible. An administrator can review the draft Executive author access package and eligible people from the library. No personal grants or position bindings were automatically assigned.
- The access package includes authoring and cross-page assistant capability, but does not automatically grant finance, payroll, people or commerce source access, or published-data distribution.
- Named recipient mode uses the recipient's own source permissions. Published-data mode requires the owner's distribution permission and exposes only the chosen pinned widgets and filters. Personal work metrics cannot use published-data mode.
- Named employees receive viewer access. Eligible executive editors can repeatedly change the approved design within the same metric/widget/filter bounds; other recipients remain on their pinned design. A later owner design must be explicitly republished before that editor resumes editing it.
- Expiry, revocation, publisher authority, recipient membership, current assignment, work-context revision and organization scope are checked server-side. Chat history and owner revision history are not shared.
- Sharing controls were inspected in the authenticated browser. No real employee grant was created for testing. Integration tests exercise publishing, recipient reads and revocation in isolated fixtures.

## Execution and live updates

Generation jobs persist in PostgreSQL with provider snapshots, usage accounting, rate/concurrency bounds, cancellation and leases. Interrupted inference fails visibly instead of automatically issuing another potentially paid request. Responses are proposals until applied. Authority, provider state and report revision are rechecked around inference.

Transactional source revision signals invalidate a report over an authenticated event stream, with 15-second polling reconciliation and reconnect support. Read-only authority checks avoid taking mutation locks during heartbeats. Temporary database errors reconnect instead of incorrectly clearing the page as revoked access. Genuine revocation clears inaccessible content. No five-second load/SLA claim has been established.

## Existing AI language changes

Workflow suggestions and generated assistant/request/expense/social guidance now request English. Prior Thai assistant answers receive a separately stored English display revision while retaining the original audit text. Original questions, names, quoted evidence and extracted business records retain their source language. This is a presentation change, not deletion of business data or model training.

## Verification

- 134 tests passed across Executive, workspace assistant, navigation, request AI, expense AI, providers and protocols. The expanded Executive suite then passed all 21 tests (four additional cases), bringing the covered total to 138 distinct passing tests.
- Coverage includes exact aggregates above list limits, currency/date/posting rules, private discovery denial, bounded sharing, expiry, revocation, scope suspension, C-level demotion, repeated editor saves, concurrent versions, cancelled/interrupted inference and revocation during generation.
- Typecheck and ESLint passed. Isolated production release build passed at `.runtime/releases/2026-09-17T23-13-18-657Z`; this was verification, not a production deployment.
- Actual authenticated public-browser checks: library/sidebar, private creation, a real OpenCode Go `muse-spark-1.3-contributor` report proposal, apply, floating chat movement, preview drag-to-canvas, Add metric, reorder/resize/undo, saved revisions, reopen, named-share controls, readable dark appearance and responsive 390px viewport.
- The private example **Executive Financial Pulse** contains four AI-designed financial widgets and a manually added live headcount widget. The observed headcount was 116; financial widgets correctly showed zero matching records for their selected period/currency. These are observed source results, not sample chart values.
- A transient-access bug discovered during browser testing was fixed and the report remained live across subsequent refreshes.
- Final availability checks: `http://127.0.0.1:3100/api/health` and `https://erpx.jesadakorn.com/api/health` both returned HTTP 200. Detached Next.js PID 78889 remained listening on 3100 from this workspace. The authenticated public report reopened at saved revision 8 and refreshed its values successfully.
- The existing organization-page assistant was also opened: its product suggestions and guidance were English. That authority-scoped conversation had no visible prior answers, so legacy translation was not exercised in that browser check.

## Metric coverage

Twelve fixed metric adapters currently cover the following module areas. Availability still depends on the selected role and scope. The registry has 32 module IDs; a registry entry does not imply an analytics adapter exists.

| Module | Executive adapter coverage |
| --- | --- |
| `brand` | No Executive metric adapter in this release. |
| `commercial` | No Executive metric adapter in this release. |
| `identity_access` | No Executive metric adapter in this release. |
| `organization` | No Executive metric adapter in this release. |
| `platform_administration` | No Executive metric adapter in this release. |
| `customer_360` | No Executive metric adapter in this release. |
| `customer_authority` | No Executive metric adapter in this release. |
| `support` | No Executive metric adapter in this release. |
| `finance` | Current open receivables/payables and monthly posted invoiced sales value. Invoice value is not recognized revenue or cash collection. |
| `pos` | Completed gross sales by business month; subsequent refunds are not netted. |
| `sales` | Confirmed/partially fulfilled/fulfilled sales order totals. |
| `assets` | No Executive metric adapter in this release. |
| `inventory` | Positive-stock product/warehouse pairs; no mixed-unit total or valuation. |
| `maintenance` | No Executive metric adapter in this release. |
| `operations` | No Executive metric adapter in this release. |
| `procurement` | Confirmed/partially fulfilled/fulfilled purchase order totals. |
| `quality` | No Executive metric adapter in this release. |
| `legal` | No Executive metric adapter in this release. |
| `people_payroll` | Distinct active headcount; no compensation/payroll analytics. |
| `crm` | No Executive metric adapter in this release. |
| `marketing` | No Executive metric adapter in this release. |
| `manufacturing` | No Executive metric adapter in this release. |
| `planning` | No Executive metric adapter in this release. |
| `projects` | Projects owned by or shared with the current reader. |
| `ecommerce` | No Executive metric adapter in this release. |
| `benefits` | No Executive metric adapter in this release. |
| `learning` | No Executive metric adapter in this release. |
| `talent` | No Executive metric adapter in this release. |
| `field_service` | No Executive metric adapter in this release. |
| `agent` | No Executive metric adapter in this release. |
| `collaboration` | Current reader open assigned tasks and published assigned routines. |
| `pms` | Configured room count; no occupancy, ADR or RevPAR. |

## Remaining plan scope

The usable core journey is delivered, but the broader design plan remains a roadmap. Outstanding items include more domain adapters, forecasts/comparisons and AI narratives grounded in metric snapshots; PDF/image report-brief extraction; cross-widget filtering and source-record drill-through; arbitrary custom component layouts beyond the validated renderer; independent desktop/mobile saved layouts; left docking; numerical recipient-preview tooling; ownership transfer; explicit data-classification/small-cohort policies; bounded query caches; and measured event/load performance targets. Recipients currently have locked filters. These capabilities must not be presented as implemented.

See the [implementation plan](../plan/2026-09-18-executive-live-canvas.md) for the full intended direction and [previous AI acceptance](ai-workspace-2026-09-18.md) for the earlier assistant/training work.
