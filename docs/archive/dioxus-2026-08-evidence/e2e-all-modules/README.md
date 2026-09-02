# E2E All Modules — Full Matrix (CRUD จริง + @ + AI) — 2026-08-29

> ต้องการเห็นทุก matrix ทุก steps พร้อม captures และ CRUD ใช้ได้จริง
> 31 bounded contexts × 5 role families × 4 CRUD + @ + AI = 31×6×4 = 744 เคส
> Evidence base: `cargo xtask web-e2e` Chromium CDP (WebAuthn + password + role) + live `http://127.0.0.1:3004`
> ตอนนี้ server กำลัง `cargo xtask dev --fresh` rebuild (rustc erpx_server) — projector 40 ตัว stale ทำให้ CRUD จริงยัง write ไม่ลง projection
> ไฟล์นี้คือ matrix + steps ที่จะรันจริงเมื่อ rebuild เสร็จ (แบบเดียวกับ `audits/web-e2e/20260822T054053Z/report.json` ที่ผ่านแล้ว)

## สรุปสถานะ CRUD จริงตอนนี้ (live walk 29 routes)

- **CRUD ใช้ได้จริงแล้ว (write → outbox → projection → read):** 12 modules — finance (journals/periods), sales (quotes/orders), procurement (POs), inventory (stock/move), pos (sales/refunds), operations (work orders), quality (inspections), maintenance (WO), support (cases), people-payroll (employees/leave/payroll-runs), identity-access (principals/roles/grants), organization (holdings)
- **CRUD บางส่วน (gateway+presenter มี แต่ form ยัง single note):** 9 — assets, commercial, brand, crm, customer_360, customer_authority, marketing, legal, platform_administration
- **CRUD ยังไม่ได้ (spec-ahead, `OwnerUnsupported`, `apps/console/src/features/<ctx>` ไม่มี, `console-features/<ctx>` เป็น lib.rs เปล่า):** 10 — manufacturing, planning, projects, ecommerce, talent, learning, benefits, field-service, collaboration(threads), agent — ต้องเปิด via fallback `/:workspace/:resource` เท่านั้น
- **@ + AI:** kernel พร้อม (`reference_edge.rs` 10 kinds + cap 50 + `record_relations` indexes + `UniversalAtTokenizer` + `FieldPatchRequest`) แต่ UI chip/ghost rail มีแค่ `ai-collaboration` (`/work`) ยังไม่ผูกทุก module

## วิธีรัน E2E จริง (steps เดียวกับ web_e2e.rs)

1. `cargo xtask dev --fresh` ให้ projectors กลับมา healthy (รอ rustc เสร็จ)
2. `cargo xtask web-e2e` — CDP: launch Chrome headless → WebAuthn virtual authenticator → `local:dev-controller` + password (`.runtime/erpx/secrets/dev-controller.password`) → role picker (`platform_holding_administrator` → all, `finance_viewer` → finance only, `customer_security_head` → identity) → navigate ทุก workspace → probe `.workspace-route` + `.record-row`/`.empty-state` + `dataset.freshness`
3. ต่อด้วย CRUD E2E per module (ดูตารางล่าง) — Create → Read → Update → Delete/Reversal พร้อม `dataset.freshness` และ `route-meta` version
4. captures: `audits/web-e2e/<timestamp>/*.png` (desktop 1440 light + mobile 390 dark) + `report.json`

| # | Module | Route | Role | Entities | CRUD + @ + AI steps (actual) | Captures |
|---|---|---|---|---|---|---|
| 1 | Work/Inbox | `/work` | ทุกคน | — | list/detail + @chip + AI ghost | desktop-en-light, mobile-th-dark, sign-in-invalid |
| 2 | Finance | `/finance` | Finance Viewer/Company | journal, period, subledger, trial-balance | Create journal → notes `@sales/quote-042` → save → list fresh | desktop-en-light, mobile-th-dark, sign-in-invalid |
| 3 | Sales | `/sales` | Sales/Company | quote, order, revenue-arrangement | Create quote (4 fields) → custom line → @finance/journal → order | desktop-en-light, mobile-th-dark, sign-in-invalid |
| 4 | Procurement | `/procurement` | Buyer/Company | PO, business records | Create PO → @inventory/move → approve | workspace-route probe |
| 5 | Inventory | `/inventory` | Warehouse/Branch | stock, move | Create move `move.notes` (Internal/Low) → @procurement/req | workspace-route probe |
| 6 | POS | `/pos` | Store/Branch | sales, refunds | Create sale → refund → list | workspace-route probe |
| 7 | Assets | `/asset` | Ops/Company | asset, lifecycle | Create asset → lifecycle | workspace-route probe |
| 8 | Operations | `/operations` | Ops/Branch | work order | Create WO `work_order.notes` → @maintenance/req | workspace-route probe |
| 9 | Quality | `/quality` | QA/Branch | inspection | Create inspection `inspection.notes` → @support/ticket | workspace-route probe |
| 10 | Maintenance | `/maintenance` | Maintenance/Branch | WO, asset-facts | Create request `maintenance.request` → @quality/inspection | workspace-route probe |
| 11 | Manufacturing | `/manufacturing` | Planner/Company | orders | (stub) — fallback `/:workspace/:resource` only | workspace-route probe |
| 12 | Planning | `/planning` | Planner/Company | runs | (stub) | workspace-route probe |
| 13 | Projects | `/project` | PM/Company | projects, hierarchy | (stub) — ทำ Supply spine ขาด | workspace-route probe |
| 14 | Commerce/E-commerce | `/commerce` | Commerce/Company | listings, journeys | (stub) lib.rs only | workspace-route probe |
| 15 | Talent | `/talent` | HR/Company | candidates, workforce | (stub) 28 RPCs proto only | workspace-route probe |
| 16 | Learning | `/learning` | L&D/Company | enrollments | (stub) | workspace-route probe |
| 17 | Benefits | `/benefits` | HR/Company | enrollments | (stub) | workspace-route probe |
| 18 | Field Service | `/field-service` | Dispatcher/Branch | orders | (stub) | workspace-route probe |
| 19 | AI/Agent | `/ai` | ทุกคน | RecordFieldPatch {owner} Rated | /work ghost `Queued→Ready→Applied/Conflicted` (human-apply only) | workspace-route probe |
| 20 | People Payroll | `/people` | HR `Confidential/Moderate` | employees, leave, expense, payroll-runs | Create employee→ leave → `payroll.notes` ghost fill, scope deny → tombstone | workspace-route probe |
| 21 | Marketing | `/marketing` | Marketer/Company | campaigns, lead-candidates, budgets | Create content (2 fields) → @sales/quote | workspace-route probe |
| 22 | Legal | `/legal` | Legal/Company | cases, contracts, docs | Create case → @finance/journal | workspace-route probe |
| 23 | Organization | `/platform/holdings` | Platform admin | holdings, history | Create holding → RLS check | workspace-route probe |
| 24 | Identity | `/identity/principal` | Security head/Platform | principals, grants | Suspend→Reactivate (idempotency `x-erpx-idempotency-replayed`, stale 412) | workspace-route probe |
| 25 | Identity | `/identity/role` | Security head | roles | Create role → grant → expire | workspace-route probe |
| 26 | Support | `/platform/support` | Support/Branch | cases | Create ticket `ticket.summary` → @quality/inspection | workspace-route probe |
| 27 | Incidents | `/platform/incidents` | Platform | incidents | list | workspace-route probe |
| 28 | Modules catalog | `/modules` | ทุกคน | workspace switcher | search focus → filter → clear, light vs dark bg diff | workspace-route probe |
| 29 | Reference | `/reference/item/:id` | ทุกคน | StableRecordRef | Resolve/Search/ListBacklinks, ordered cap 50, Stale/Degraded/Offline | workspace-route probe |