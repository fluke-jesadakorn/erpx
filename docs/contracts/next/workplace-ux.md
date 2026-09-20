# Workplace navigation and work queues

Daily navigation is Home, Work, Inbox and Calendar. On desktop and tablet, the 260px business-workspace sidebar then renders permission-aware app links under five presentation categories in stable order: People & organization; Operations; Finance; Management; and System. Empty categories are omitted. Categories organize discovery only and do not become domain parents, routes or breadcrumb segments. Deep destinations such as Roster, Payroll, Ledger, Stock counts, Reservations and Directory & GPO remain in the owning app's local tabs and destination catalog instead of being repeated in the sidebar. Platform navigation is separate and unchanged.

The full sidebar is a single-open accordion. The current app's category opens automatically. It can collapse to a 72px icon rail, whose category buttons open keyboard-accessible flyouts; both the rail state and the last manually opened category are stored per workspace in local storage. Focus returns to a flyout trigger on Escape and interactive targets are at least 44px. Phones expose exactly five bottom destinations: Home, Work, Inbox, Calendar and Apps.

Apps opens the App launcher. Its default view contains only app cards under the same permission-filtered categories. Searching expands into deep destinations and shows owning-app context such as `Finance › Bank reconciliation`; the launcher and `⌘K` consume the same URL-deduplicated destination catalog. Favorites, saved pins and the navigation-preference API are no longer active. Historical `navigation_preferences` migrations and rows remain intact for data preservation but are ignored by this navigation model.

## Work and Home

Work opens with `view=actions`, with Waiting, My requests, Team and Completed tabs and All work under More. Legacy `queue`, `responsibility`, `type` and `q` URLs continue to work. Domain adapters supply `canAct`, operational/setup category and a setup reason; this presentation does not confer command authority. Due items sort oldest deadline first, then undated work. Setup without an owner appears grouped in Team. Explicitly assigned eligible IT work remains in My actions. Partial sources are labelled and never presented as a complete zero count.

Home's three priorities use the same actionable work source and ordering. Work's badge counts My actions. Home remains a team news/conversation feed. Inbox separates Messages, Teams and Notifications; notifications resolve the authorized source record and retain its conversation, without a second approval queue.

## Record and creation patterns

Record pages for requests, expenses, orders, invoices, journals, payroll, tasks, stock counts and transfers expose a shared status/responsible-person/next-action summary before detailed sections and history. The existing domain controls retain their independent approvals and state machines. Work return links preserve whitelisted filter parameters and workspace context.

New work groups personal requests and operational work by current creation capabilities, using intent labels and examples. Requests and expenses support evidence in the creation flow. A request stores the exact idempotent draft command before upload; network recovery reuses the command, locks the saved fields and retains failed uploads for retry. On reload, the draft ID is recovered and the user is told to reselect files the browser cannot restore. Existing edit forms, source permissions and optional AI review remain authoritative.

People, Approvals, POS and PMS share a readiness checklist with missing items, responsible roles and links to resolve them. Projects, Routines, Handbook, Learn, Billing and Administration reuse page headers, filters, empty/error states and the accessible Dialog. Preview capabilities retain their existing restrictions.

## People views and layouts

Standard People pages keep People directory, Roster, Organization & positions and authorized Payroll access in the feature tab bar. Company handbook, Learn and authorized People settings remain at their existing URLs and in global navigation, but appear under **People tools** inside the **People views and layout** drawer. Beta People retains its existing three-tab navigation. Both lists are derived from the same permission-filtered navigation model; moving a destination into the drawer never grants access.

The drawer also edits the current page's permission-filtered widgets. Layouts remain scoped to the user, Company, workspace base and page key. A personal layout is compared in the client with the reconciled **Organization layout** baseline for visibility, relative order, width and height. This comparison is presentation data and is not persisted. Reset widget restores that widget's organization visibility, geometry and ordinal position without discarding other personal changes. Reset all removes the personal override. Authorized organization managers may publish a new Organization layout; existing personal layouts remain unchanged until their owners reset them.

Layout changes preview on the page before Save and retain pointer/keyboard movement, resizing, hidden-widget restoration and legacy widget links. Cancel, drawer close and in-app navigation request confirmation when a draft is dirty. The modal drawer traps focus, restores the trigger, closes with Escape or backdrop, and prevents background scrolling. It is a fixed right drawer above 650px and a safe-area-aware bottom sheet at 650px and below.

**Organization layout** refers only to the Company-wide widget baseline. Windows Group Policy/GPO belongs to Technology → Groups & policies and is not part of page-layout comparison or publishing.

Finance exposes To collect, To pay, To post and To reconcile queues linked to the existing invoice, ledger and bank screens. POS readiness leads to register/shift setup while cashier and kitchen stay separate operational views. PMS preserves the booking and reservation/stay/folio workflows and their original records.

## Verification

Focused tests cover route-to-app/category mapping, URL uniqueness, active state, breadcrumbs, local tabs, permission filtering, workspace-scoped rail persistence and launcher discovery; request upload recovery; lifecycle/retirement; Calendar/Roster; partial receiving/payment; and POS/PMS retry/handoff behavior. Browser acceptance uses the normal authenticated public session and checks responsive layouts, light/dark, keyboard dialogs and navigation history. See the [implementation verification record](../../plan/2026-09-17-ux-retirement-verification.md) for earlier run results and limitations.
