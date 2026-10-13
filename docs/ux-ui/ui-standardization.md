# ERPX shared UI standardization

Status: implementation in progress, **not accepted as complete**. The [verification receipt](standard-verification.json) records passing checks, environment health and the deliberately failing completion gate. The generated [coverage manifest](standard-coverage.json) currently inventories 206 route files and 325 component files. Source inventory and browser smoke observations do not prove every permission, data, popup, print, and query state. The completion guard intentionally fails while those cases remain pending.

## Shared presentation

Use the existing shadcn `base-nova` / Base UI foundation and ERPX semantic tokens. Existing shared components default to `appearance="legacy"`; ordinary migrated callers explicitly use `appearance="standard"`. Specialized callers must retain legacy presentation, including portalled children. No theme tokens or generic table/card selectors are changed for this work.

- `PageContent` owns page padding and the 1536px maximum. Use `Stack` inside a padded page or panel; do not nest page padding.
- `Card` and `TableFrame` opt in explicitly. Tables remain server-rendered native tables, use 14px/20px text, headers at least 44px and rows at least 56px. Content may expand rows. Align money cells with `data-align="right"`.
- `FilterBar` / `ListFilters` retain native GET forms, hidden fields, query names and Enter submission. Search expands, filters have bounded widths and desktop action buttons size to their labels.
- `RecordList` / `RecordListItem` provide one divided surface for narrative items. Important metadata stays visible; separately expandable follow-up history remains available.
- `FormLayout` is a native form with refs, submitter and validation retained. Its grid layout is the default; use `layout="stack"` when the form contains nested field groups. `FormGrid` and `FormActions` supply the corresponding inner presentation.
- `FormDrawer` / `DraftFormDrawer` and `FormDisclosure` present long editors from the current page on the right. Dedicated route forms remain inline. Controlled editors supply `onDiscard` to reset their own values. A disclosure without a reset callback offers **Close without saving**, rather than claiming to erase controlled values it does not own.
- `Badge`, `InlineNotice`, `Menu`, `Popover`, `Combobox`, `Tooltip`, `LocalTabs` and `TabNav` provide shared status, feedback, popup, local-state and URL-navigation patterns.

## Implemented examples and checks

| Area | Current change | Browser evidence and limits |
| --- | --- | --- |
| Purchase orders | Shared register/table and natural-width Search |1440px: rows 56px, text 14px; earlier mobile observation. Full query/role matrix remains pending. |
| Warehouse transfers | Six-column register, one heading, complete purpose/value/status |1440px: text/header14px, multiline rows 79px; Enter search reached empty state and clearing restored 25 rows. Earlier mobile checks found only table-frame scrolling. |
| Price lists | Register includes every product, unit price, tax, customer/date/version/status and actions; create/edit drawers |390/768/1440px observations; price data stays visible in expandable-height rows. Dirty draft keep/discard checked without saving business data. |
| My actions | Shared divided list, readable metadata and separate follow-up disclosure |390px observation without page overflow; complete conditional work-item/action matrix remains pending. |
| Legal | Base UI local tabs and record editor drawer with preserved underlying page |All 9 accessible detail tabs observed on the fictional request; keyboard tab navigation, mobile layout, 44px form controls, keep/discard and opener focus restoration checked. Other register kinds, tools, approval states and print remain pending. |
| Account / currency controls | Standard routes opt in to Base UI popovers and searchable comboboxes |Nested role popup and THB search checked without changing role/currency preferences. Legacy defaults retained for excluded routes. |
| Ordinary module forms and feedback | Shared panels, controls, filters, long-form disclosures and error/status feedback |Broad source migration is implemented. Earlier smoke observations covered CRM, Customers, Marketing, Quality, RFQs, Finance, Benefits, Talent, Learning, Operations, Manufacturing, Maintenance, Support, Brand and Agents. These observations predate some later edits and are not current acceptance. |
| Auth and portals | Ordinary controls and feedback opt in |Login/reset/verification/access/invitation and customer/legal portal landing states observed. Valid/expired invitations, alternate portal permissions and document states still require evidence. |

Automated shared-pattern browser tests cover 390/768/1440px, light/dark/system, Thai input, native FormData/submitter/Enter, dirty confirmation, keyboard focus restoration, menus/tabs and comboboxes inside drawers. Register assertions check 14px cells, 44px headers, 56px minimum rows and no page overflow. These tests do not substitute for individual business-view acceptance.

## Boundaries

[Scope policy](../../scripts/ui/standard-scope.mjs), [shared consumer list](../../scripts/ui/shared-consumers.json) and [frozen source/style fingerprints](../../scripts/ui/frozen-boundaries.json) make exclusions reviewable. PMS/POS/front/booking/hospitality and the external marketing website stay deferred. People/IT organizational drag/drop, roster/calendar grids, Executive canvas/layout editor, rich-text internals and WBS graph retain specialized presentation. Ordinary ERPX Marketing and Finance reconciliation remain in scope.

Mixed pages retain the geometry around specialized children. In particular, the People directory widget explicitly retains legacy appearance and calendar route padding is frozen. Configurable saved-layout widgets retain their legacy renderer. Shared component defaults must not silently opt those consumers in. LoginForm also defaults to legacy for front-access; only the ordinary login page opts into the standard form. A front-access browser request on the console origin is blocked by its route policy, so front-host appearance is not claimed as browser-verified.

## Coverage and completion

Run with pinned Bun 1.4.3 / Node 26.11.1:

```sh
bun run ui:coverage
bun run ui:guard
bun run ui:check-complete
```

Coverage follows static/dynamic TSX imports, TS registries/re-exports, ancestor layouts, loading/error/not-found boundaries, CSS owners, literal query links, popup props, conditional expressions and authorization/permission calls. Route fingerprints include reachable sources and styles. The manifest records pending surfaces and state checklists; it does not infer successful acceptance from imports or shared classes.

`ui:guard` checks frozen boundaries, migrated imported control aliases, native table bypasses, stale source inventories and legacy presentation classes. Excluded-owner dependency traversal additionally detects ordinary migrated components that need an explicit legacy adapter. `ui:check-complete` additionally rejects pending route/component acceptance. These source checks supplement browser verification; they cannot prove appearance or drag/resize behavior.

Remaining acceptance work includes every route/query/pagination view under the proper role, conditionally opened components, disabled/read-only/error/loading states, per-editor controlled draft reset contracts, actual specialized interaction regression checks, and invoice/payslip/Thai print inspection. Some registers still need page-specific pattern review. No staging access or production deployment is authorized for this work.

Verification logs and source snapshots are retained under ignored `.scratch/ui-standard/`. The workspace contained extensive changes before this task; this migration preserves that starting snapshot rather than resetting unrelated work. The isolated release build uses `.runtime/releases/`, not the live dev output directory.
