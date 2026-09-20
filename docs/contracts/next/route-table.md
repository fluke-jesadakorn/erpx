# docs/contracts/next — Route Table (Living, Next.js)

> Living counterpart to `docs/plan/traceability.md` (552 Rust RPCs archived on `backup/rust-2026-09-02`, see `docs/plan/ARCHIVE.md` + `CUTOVER.md`). Grows as `src/app/api/*` lands.

## Implemented foundation routes

Directory & access at scoped /people/access owns the People, Groups, Access policies and Custom attributes views. Its authorized /people/access/management read and /iam/user-management/{preview|save} commands are specified in [Local user management](user-management.md). No additional sidebar destinations or module catalog routes are introduced.

| Route | Method | Context | Contract | Status |
|---|---|---|---|---|
| `/api/auth/[...all]` | GET/POST | identity | Better Auth handler with device multi-session account switching | Implemented |
| `/api/v1/platform/assignments/active` | POST | platform identity/access | Select an active Platform-only assignment | Implemented |
| `/api/v1/holdings/{holdingSlug}/assignments/active` | POST | identity/access | Select an active assignment for the authenticated account; D1 eligibility is rechecked | Implemented |
| `/api/v1/holdings/{holdingSlug}/companies/active` | POST | organization | Select a validated Company in a Holding workspace; Local workspaces reject overrides | Implemented |
| `/api/v1/holdings/{holdingSlug}/leave-requests/{id}` | GET | people/payroll | Authorized owner read | Implemented |
| `/api/v1/holdings/{holdingSlug}/leave-requests/{id}` | PATCH | people/payroll | Submit draft; expected version and `Idempotency-Key` | Implemented |
| `/api/v1/holdings/{holdingSlug}/leave-requests/{id}/approve` | POST | people/payroll | Current named department or independent HR decision | Implemented |
| `/api/v1/holdings/{holdingSlug}/uploads` | POST | files | Authorized private leave/expense upload intent | Implemented |
| `/api/v1/holdings/{holdingSlug}/uploads/{id}/content` | PUT | files | Bounded upload, atomic finalization and replay | Implemented |
| `/api/v1/holdings/{holdingSlug}/attachments/{id}/content` | GET | files | Owner-scoped private download/range | Implemented |
| `/api/v1/holdings/{holdingSlug}/feed` | GET | collaboration | Scoped feed with all, following, team and record filters | Implemented |
| `/api/v1/holdings/{holdingSlug}/feed` | POST | collaboration | Publish scoped update with topics and optional authorized record link | Implemented |
| `/api/v1/holdings/{holdingSlug}/posts/{id}/comments` | POST | collaboration | Publish comment on a visible update | Implemented |
| `/api/v1/holdings/{holdingSlug}/feed` and `/posts/{id}/comments` | POST | collaboration | Persist validated typed person/record mention edges supplied by the `@` picker | Implemented |
| `/api/v1/holdings/{holdingSlug}/posts/{id}/reaction` | PUT | collaboration | Idempotently set appreciation state | Implemented |
| `/api/v1/holdings/{holdingSlug}/posts/{id}/follow` | PUT | collaboration | Idempotently set post follow state | Implemented |
| `/api/v1/holdings/{holdingSlug}/stories` | GET/POST | collaboration | List or publish persistent scoped Stories | Implemented |
| `/api/v1/holdings/{holdingSlug}/stories/{id}/archive` | POST | collaboration | Author-only versioned Story archive | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/{module}/{id}` | GET/PATCH | universal record | Scoped read and optimistic typed-field update | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/{module}` | GET/POST | universal record | List a section or create a Draft with its documented field schema | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/{module}/{id}` | DELETE | universal record | Version-checked recoverable archive | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/{module}/{id}/transition` | POST | universal workflow | Submit, independent review, completion, or reopen | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/search?q=@...` | GET | reference | Assignment-authorized record-reference search | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/{module}/{id}/references` | POST | reference | Append a typed, deduplicated, role-labelled edge | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/{module}/{id}/mentions` | POST | collaboration | Attach a typed, server-resolved person mention from the current Holding | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/{module}/{id}/suggestions` | POST | agent | Create an evidence-pinned development-provider suggestion | Implemented |
| `/api/v1/holdings/{holdingSlug}/records/{module}/{id}/suggestions/{suggestionId}` | POST | agent | Human apply/reject with host and evidence version checks | Implemented |
| `/api/v1/holdings/{holdingSlug}/people/*` | GET/POST | people/payroll | People directory, engagements, compensation, pay groups, time entries, country-pack lifecycle, payroll previews and legacy payment export/confirmation; production payroll gated | Partial domain implementation |
| `/api/v1/holdings/{holdingSlug}/finance/{...path}` | GET/POST | finance | Chart, accounts, periods, journals, posting/reversal, AP/AR invoices and aging, payments, bank CSV/reconciliation, reports; remaining tax/asset/budget/lease/loan/revenue APIs require domain hardening | Partial domain implementation |

Expense routes (also available through authorized branch/department bases):

| Route | Method | Contract |
| --- | --- | --- |
| `/api/v1/holdings/{holdingSlug}/expenses` | GET/POST | Scoped claims/options; create a typed claim linked to an active employee |
| `/api/v1/holdings/{holdingSlug}/expenses/{id}` | GET/POST | Detail with receipts, decisions and payments; edit own unsubmitted claim |
| `/api/v1/holdings/{holdingSlug}/expenses/{id}/{submit,approve,reject,cancel}` | POST | Versioned lifecycle, mandatory receipt, independent review, durable decision reasons |
| `/api/v1/holdings/{holdingSlug}/expenses/{id}/{post,reverse}` | POST | Finance posting or unpaid reversal; AP and ledger update atomically |
| `/api/v1/holdings/{holdingSlug}/expenses/{id}/attachments/{attachmentId}/revoke` | POST | Remove evidence from an editable claim while retaining history |
| `/api/v1/holdings/{holdingSlug}/commerce/invoices/{id}/settle` | POST | Record a reimbursement against the expense payable; update balance, status, allocation and cash journal together |

All expense mutations require `Idempotency-Key`; decisions and corrections require `expectedVersion`. Legacy `/people/expenses` and `/finance/expenses` mutations return `410 EXPENSE_API_MOVED`.

Stock-count routes (holding and authorized branch bases; department assignments are not count operators):

| Route | Method | Contract |
| --- | --- | --- |
| `/api/v1/holdings/{holdingSlug}/stock-counts` | GET/POST | Scoped count register/options; save physical quantities and a versioned stock snapshot |
| `/api/v1/holdings/{holdingSlug}/stock-counts/{id}` | GET/POST | Read quantity/value differences, immutable decisions and history; preparer edits draft/returned count |
| `/api/v1/holdings/{holdingSlug}/stock-counts/{id}/{submit,reject,cancel,post}` | POST | Versioned lifecycle; independent holding finance review; stock delta and balanced journal commit together |

Mutations require `Idempotency-Key`; updates and transitions require `expectedVersion`. A changed product/stock snapshot returns a conflict and requires a recount. Posting requires an open period and active inventory/variance accounts. A count cannot predate a later inventory movement; commerce fulfillment applies the same historical-date guard.

Warehouse-transfer routes:

| Route | Method | Contract |
| --- | --- | --- |
| `/api/v1/holdings/{holdingSlug}/stock-transfers` | GET/POST | Scoped source/receiver register and options; prepare same-company transfer quantities at source cost |
| `/api/v1/holdings/{holdingSlug}/stock-transfers/{id}` | GET/POST | Detail with immutable receipt decisions and history; source preparer edits draft/returned transfer |
| `/api/v1/holdings/{holdingSlug}/stock-transfers/{id}/{submit,reject,cancel,post}` | POST | Versioned independent receipt review; equal source-out and destination-in movements commit atomically |

Transfer commands require `Idempotency-Key` and versioned updates. Branch preparers must own the source; a branch reviewer must own the destination and have an inventory management role. Holding finance approvers may review across branches. Self-review, insufficient or changed source stock, inactive references, closed periods, changed routes and backdated movements are denied. Direct transfers preserve total company quantity/value; transit and partial receiving are not implemented.

## Enterprise workplace views

Every `{destination}` below is available under these canonical workspace bases. APIs mirror the same bases below `/api/v1`:

- `/platform`
- `/holdings/{holdingSlug}`
- `/holdings/{holdingSlug}/branches/{branchSlug}`
- `/holdings/{holdingSlug}/departments/{departmentSlug}`
- `/holdings/{holdingSlug}/branches/{branchSlug}/departments/{departmentSlug}`

Slugs are immutable and unique within their route parent. Opaque IDs do not appear in public URLs. The removed short-form namespace has no redirect or compatibility alias.

| Route | Source and behavior |
|---|---|
| `/login` | Live sign-in with existing device accounts |
| `/holdings/{holdingSlug}/home` | Authorized leave, expense, physical-count and transfer priorities/totals and team updates |
| `/holdings/{holdingSlug}/work?queue=todo\|waiting\|completed\|all&q=...` | Role-specific leave, expense, count and transfer queues sharing status/search filters |
| `/holdings/{holdingSlug}/stock-counts` and `/stock-counts/{id}` | Physical-count register, typed preparation, correction and independent posting |
| `/holdings/{holdingSlug}/stock-transfers` and `/stock-transfers/{id}` | Warehouse transfer register, source preparation and independent receipt confirmation |
| `/holdings/{holdingSlug}/finance` | IFRS finance dashboard with chart, journal, balance and close navigation |
| `/holdings/{holdingSlug}/tasks/people_payroll/{id}` | Live leave details, evidence, submission review and independent approval |
| `/holdings/{holdingSlug}/feed` | Live updates, comments, reactions, follows and Stories |
| `/holdings/{holdingSlug}/learn` | Captioned sample learning videos |
| `/holdings/{holdingSlug}/inbox` | Explicit notification preview |

The former holding and platform module catalogs, sample-record previews, and
generic record workbench were development UI and are intentionally not public
routes. They return the standard unavailable/not-found response without a
redirect. The universal record APIs above remain for backend compatibility and
historical data access.

Authenticated business-workspace discovery uses one role- and scope-aware
destination catalog. Home, Work, Inbox, and Calendar are primary destinations.
The sidebar exposes app-level links under presentation-only People &
organization, Operations, Finance, Management, and System categories. Deep
pages remain in app-local tabs and become launcher or command-search results
only when searched. Redirect aliases, creation states, and individual records
inherit their app parent for active navigation and `App › Page` breadcrumbs but
are not separate default launcher cards. Platform keeps its existing flat
navigation contract.

## Planned business routes

| Route | Method | Context | Spec (light → deep) | Proto (archived) | Status | Implemented |
|---|---|---|---|---|---|---|
| `/api/sales/quotes` | POST | sales | `docs/plan/02-capabilities/03-finance-sales-pos/sales.md:9 CreateQuote` → `docs/plan/03-cross-cutting/@-and-ai/sales.md:15` | `sales/service.proto:CreateQuote` | Living (TBD) | — |
| `/api/sales/quotes/:id/send` | POST | sales | `02/sales.md:9 SendQuote` → `03/sales.md:15` | `sales/service.proto:SendQuote` | Living (TBD) | — |
| `/api/sales/quotes/:id/accept` | POST | sales | `02/sales.md:9 AcceptQuote` → `03/sales.md:15` | `sales/service.proto:AcceptQuote` | Living (TBD) | — |
| `/api/sales/orders` | POST | sales | `02/sales.md:9 CreateSalesOrder` → `03/sales.md:15` | `sales/service.proto:CreateSalesOrder` | Living (TBD) | — |
| `/api/v1/holdings/{holdingSlug}/finance/journals` | GET/POST | finance | Create, list, balance and persist exact multi-currency journals | `finance/service.proto:CreateJournal` | Living | Implemented |
| `/api/v1/holdings/{holdingSlug}/finance/journals/:id/post` | POST | finance | Immutable posting with period gate, audit and outbox | `finance/service.proto:PostJournal` | Living | Implemented |
| `/api/v1/holdings/{holdingSlug}/finance/periods/:id/close` | POST | finance | Human proof, separation of duties and controlled period freeze | `finance/service.proto:ClosePeriod` (HumanDecisionProof) | Living | Implemented |
| `/api/pos/sales` | POST | pos | `02/pos.md:9 CreateSale` → `03/pos.md:15` | `pos/service.proto:CreateSale` | Living (TBD) | — |
| `/api/commercial/catalog-plans` | POST | commercial | `02/commercial.md:9 CreateCatalogPlan` → `03/commercial.md:15` | `commercial/service.proto:CreateCatalogPlan` | Living (TBD) | — |
| `/today` | GET | collaboration | `04-social-paradigm.md` digest | `collaboration/service.proto:ListMyDay` | Living (TBD) | — |
| `/tags/:tag` | GET | collaboration | `04-social-paradigm.md` hashtags | `collaboration/service.proto:ListHashtagTopics` | Living (TBD) | — |
| `/posts/:id` | GET | collaboration | `04-social-paradigm.md` Post | `collaboration/service.proto:GetFeedItem` | Living (TBD) | — |
| `/notifications` | GET | collaboration | `04-social-paradigm.md` inbox | `collaboration/service.proto:ListMentionInbox` | Living (TBD) | — |

Add rows as `src/app/api/<ctx>/*` handlers land. Keep this file in sync with `docs/plan/MAP.md` and `docs/plan/traceability.md` living section.

## Conventions

- **Idempotency:** every command carries `Idempotency-Key` + authoritative tenant/holding scope (from `CONTRACT_POLICY.md`).
- **Versioned namespace:** `/api/v1/platform/...` and the canonical Holding/Local workspace prefixes below `/api/v1/holdings/{holdingSlug}/...` — never reuse a route or field with different meaning; removed fields stay reserved.
- **Verification:** `bun run lint`, `bun run build`, `bun run build:cloudflare`, and `bun run test:integration` pass. Archived Rust gates stay on `backup/rust-2026-09-02`.

## Links

- Archive RPC table: `docs/plan/traceability.md` (banner Archived — Rust era)
- Spec hub: `docs/TOC.md` → `docs/plan/MAP.md` (31 contexts) → `docs/plan/01-vision-and-scope.md` (spine)
- Policy: `CONTRACT_POLICY.md` + `docs/adr/ADR-020-nextjs-stack.md` + `docs/adr/ADR-022-cloudflare-runtime.md` + `docs/adr/ADR-023-three-level-workspace-routing.md`


## Staff work and approval routes

See [Staff requests, levels and work](staff-work.md) for versioned request bodies, participant policies, routine defaults and release order. Workspace-prefixed UI routes: `/staff-tasks`, `/staff-tasks/{id}`, `/projects`, `/projects/{id}`, `/routine-templates`, `/routine-templates/{id}`, `/staff-settings`. New staff APIs use the same workspace prefix and retain legacy generic record routes. Work and Inbox include current named approvals and task participants; business-role module selection does not gate personal requests or assigned work.

## Return/refund commands (simulation acceptance)

All POST commands use source permissions, `Idempotency-Key` and `expectedVersion` (export has a version for every instruction). See [S04 refund simulation](../../testing/return-refund-simulation.md).

| Route under `/api/v1/holdings/{holdingSlug}/finance` | Method | Owner behavior |
| --- | --- | --- |
| `/refunds`, `/refunds/{id}` | GET | Company/branch/department-filtered obligations and evidence |
| `/refunds/{id}/approve` | POST | Independent source/beneficiary approval |
| `/refunds/{id}/instructions` | POST | Reserve partial/final refund amount |
| `/refund-instructions/export` | POST | Immutable CSV receipt with SHA-256; no cash posting |
| `/refund-instructions/{id}/export` | GET | Recover an authorized committed export |
| `/refund-instructions/{id}/exception` | POST | Cancel before export, hold unknown result, independently verify failure |
| `/refund-instructions/{id}/confirm` | POST | Bank evidence + cash GL + payment + remaining obligation commit together |

## Supplier payment batches (simulation)

Company Finance/Treasury scope only; mutation commands require `Idempotency-Key` and expected versions (create supplies every invoice version). Details: [supplier payment simulation](../../testing/supplier-payment-simulation.md).

| Route under `/api/v1/holdings/{holdingSlug}/finance` | Method | Behavior |
| --- | --- | --- |
| `/supplier-batches`, `/supplier-batches/{id}` | GET | Source-owned batches / review workspace |
| `/supplier-batches` | POST | Reserve posted invoice amounts without cash posting |
| `/supplier-batches/{id}/approve`, `/cancel` | POST | Independent approval or cancel before export |
| `/supplier-batches/{id}/export` | POST / GET | Third-person export / recover immutable receipt |
| `/supplier-instructions/{id}/result` | POST | Unknown retains reservation; independently verified failure releases it |
| `/supplier-instructions/{id}/confirm` | POST | Bank evidence, payment, allocation, invoice and GL commit together |

## Company routing attention

`GET /api/v1/holdings/{holdingSlug}/routing-attention?kind={flowKind}&after={lastVisibleRecordId}&limit=25` requires current Company organization-management authority, then owner read permission for each source. Returns only authorized blockers and a next cursor based on visible records. Read-only scan; repairs use existing versioned `/flows/{kind}/{id}` commands. Named Flow coverage only; see [routing simulation](../../testing/routing-attention-simulation.md).

### Sales stock reservations

`POST /api/v1/holdings/{holdingSlug}/commerce/orders/{id}/reservations` — `inventory.prepare`, source Company/Branch/Department scope, approved sales order, `Idempotency-Key` and `expectedVersion`. Reserve/release quantity, reason and reserve expiry; atomic audit and source version. See [simulation contract](../../testing/shared-stock-reservation-simulation.md).

### Business readiness

`GET /api/v1/holdings/{holdingSlug}/business-readiness` — active Company organization manager only, private/no-store. Bundled workspace simulation manifest and scoped evidence; no Company production sign-off and no write/approval endpoint. UI: `/holdings/{holdingSlug}/settings/business-readiness`.
