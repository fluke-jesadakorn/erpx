# docs/contracts/next — Route Table (Living, Next.js)

Apps `/` is the Feed entry; Apps `/applications` is the Company → Workspace → App selector. Feed continues through the existing authorized `/launch` flow to `/holdings/…/home`. Work has no registered host or redirect.

> Current versioned REST route reference. Handlers in `src/app/api/` and the owner contracts are authoritative; archived Rust RPCs are recoverable through [CUTOVER](../../../README.md#recovery).

## Implemented foundation routes

Directory & access at scoped /people/access owns the People, Groups, Access policies and Custom attributes views. Its authorized /people/access/management read and /iam/user-management/{preview|save} commands are specified in [Local user management](user-management.md). No additional sidebar destinations or module catalog routes are introduced.

| Route | Method | Context | Contract | Status |
|---|---|---|---|---|
| `/` on Apps | GET | Authenticated Apps identity | Company → Workspace → App in one page; exact authorized scope links, six initial apps, account-separated optional browser memory; see [Apps selection](identity-access.md#apps-company-first-selection) | Development |
| `/api/mcp/executive` | POST | Executive | Token-authenticated Streamable HTTP for five read-only dashboard/KPI tools | Development; production rollout separate |
| `/api/v1/holdings/{holdingSlug}/executive/connections` | GET/POST | Executive | Own connection metadata, one-time token issuance and revocation via browser work context | Implemented |
| `/api/auth/[...all]` | GET/POST | identity | Better Auth handler with device multi-session account switching | Implemented |
| `/api/v1/account/device-accounts` | GET/DELETE/OPTIONS | Account device owner | Token-free saved account summaries and removal; exact staff-origin credentialed CORS | Development |
| `/api/v1/account/device-accounts/{initialize,sign-in,active}` | POST/OPTIONS | Account device owner | Five-account browser selection, normal authentication, revision checks and old Work-context revocation | Development |
| `/api/v1/account/switcher-context` | GET | Staff product identity | Current local identity and minimal authorized Company/Platform role choices | Development |
| `/api/v1/account/role-selection` | POST | Staff product identity | Delegate the current versioned role choice to its original owner | Development |
| `/account/continue` | GET | Staff product identity | Check the source workspace after SSO, or return to Apps; verified original-account draft continuation | Development |
| `/account/device-return` | GET | Account identity | Closed return from normal Account/Directory login to the selected staff app | Development |
| `/api/v1/platform/assignments/active` | POST | platform identity/access | Select an active Platform-only assignment | Implemented |
| `/api/v1/platform/operations` | GET | platform operations | Read-only live control-plane and per-Holding outbox health; requires Platform context and returns private no-store data | Implemented |
| `/api/v1/platform/operations/recover` | POST | platform operations | Platform-admin reviewed dead-letter recovery with reason, expected attempt/error snapshot, idempotency and Holding audit | Implemented |
| `/api/v1/holdings/{holdingSlug}/assignments/active` | POST | identity/access | Select an active assignment for the authenticated account; PostgreSQL assignment eligibility is rechecked | Implemented |
| `/api/v1/holdings/{holdingSlug}/companies/active` | POST | organization | Select a validated Company in a Holding workspace; Local workspaces reject overrides | Implemented |
| `/api/v1/holdings/{holdingSlug}/leave-requests/{id}` | GET | people/payroll | Authorized owner read | Implemented |
| `/api/v1/holdings/{holdingSlug}/leave-requests/{id}` | PATCH | people/payroll | Submit draft; expected version and `Idempotency-Key` | Implemented |
| `/api/v1/holdings/{holdingSlug}/leave-requests/{id}/approve` | POST | people/payroll | Current named department or independent HR decision | Implemented |
| `/api/v1/holdings/{holdingSlug}/uploads` | POST | files | Authorized private leave/expense upload intent | Implemented |
| `/api/v1/holdings/{holdingSlug}/uploads/{id}/content` | PUT | files | Bounded upload, atomic finalization and replay | Implemented |
| `/api/v1/holdings/{holdingSlug}/attachments/{id}/content` | GET | files | Owner-scoped private download/range | Implemented |
| `/api/v1/holdings/{holdingSlug}/attachment-sets` | GET/POST | files | List authorized bound/current-draft manifests or create a server-policy-derived File Set | Implemented behind the global attachment gate |
| `/api/v1/holdings/{holdingSlug}/attachment-sets/{setId}/files` | POST | files | Reserve Company quota and a 15-minute private quarantine upload intent | Implemented behind the global attachment gate |
| `/api/v1/holdings/{holdingSlug}/attachment-sets/{setId}/links` | POST | files | Reuse one clean canonical source in the same Company after source-read and destination-write authorization | Implemented behind the global attachment gate |
| `/api/v1/holdings/{holdingSlug}/attachment-sets/{setId}/bind` | POST | files | Bind clean record documents with optimistic target/policy checks; action workflows bind in their own domain transaction | Implemented behind the global attachment gate |
| `/api/v1/holdings/{holdingSlug}/attachment-sets/{setId}/sanitization-attestation` | POST | files/security | Record the reviewed safe-manifest attestation required for IAM, release, migration, and provisioning evidence | Implemented behind the global attachment gate |
| `/api/v1/holdings/{holdingSlug}/attachment-links/{linkId}/status` | GET | files | Reauthorize a link and return its safe upload/scan/binding state | Implemented behind the global attachment read gate |
| `/api/v1/holdings/{holdingSlug}/attachment-links/{linkId}/content` | GET | files | Audited private original download or inline server-sanitized preview; never renders original bytes inline | Implemented behind the global attachment read gate |
| `/api/v1/holdings/{holdingSlug}/attachment-links/{linkId}/withdraw` | POST | files | Reasoned withdrawal before binding; bound evidence is immutable | Implemented behind the global attachment gate |
| `/api/v1/holdings/{holdingSlug}/media-publications` | GET/POST | public media | List or propose a revisioned sanitized derivative with alt text and order | Implemented behind the global attachment gate |
| `/api/v1/holdings/{holdingSlug}/media-publications/{publicationId}/{approve,revoke}` | POST | public media | Independent approval or reasoned revocation of a sanitized derivative | Implemented behind the global attachment gate |
| `/api/v1/holdings/{holdingSlug}/media-publications/order` | POST | public media | Optimistically reorder approved publication revisions | Implemented behind the global attachment gate |
| `/api/public/holdings/{holdingSlug}/media-publications/{publicationId}/content` | GET | public media | Serve only an approved sanitized derivative; original source IDs and object keys are never public | Implemented |
| `/api/v1/holdings/{holdingSlug}/customer-access` | GET/POST | customer authority | Read approved binding/history; issue or resend an explicit invitation only for a current approved binding | Implemented; browser acceptance pending |
| `/customer-portal/enroll` | GET | Account / customer portal | Read the invitation token from the URL fragment and scrub it from the address bar; no identity binding occurs on page load | Implemented; browser acceptance pending |
| `/api/customer-portal/enroll` | POST | Account / customer portal | Restrict signup to the invited address or explicitly accept the signed token for the exact verified Account identity | Implemented; browser acceptance pending |
| `/api/customer-portal/{holdingSlug}/attachments/{...path}` | GET/POST/PUT | customer portal/files | Authenticated explicit-owner upload, scan status, and customer-shareable preview-only access; no anonymous generic upload | Implemented behind the global attachment gate |
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
| `/api/v1/holdings/{holdingSlug}/people/*` | GET/POST | people/payroll | People directory, engagements, compensation, pay groups, time entries, country-pack lifecycle, payroll previews, immutable review exports, controlled correction requests, payroll advances and legacy payment export/confirmation; production payroll gated | Partial domain implementation |
| `/api/v1/holdings/{holdingSlug}/people/lifecycle/tasks/{taskId}/final-pay` | POST | people/payroll and retirement clearance | Assigned Payroll owner creates one person-scoped `final_pay` preview through the last working day; idempotent and task-version checked | Implemented |
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

Department authority routes:

| Route under `/api/v1/holdings/{holdingSlug}` | Method | Contract |
| --- | --- | --- |
| `/authority` | GET | Current CEO, HR and head seats, appointment history, published flows, and the caller's available actions |
| `/authority/appointments` | POST | Owner, CEO, or HR appointment within their authority; reason, `expectedVersion`, and `Idempotency-Key` |
| `/authority/flows` | POST | Appointed head publishes one to twenty ordered person or Position selectors; CEO can publish while the head seat is vacant |
| `/authority/decisions` | POST | Higher reporting manager with higher Level decides the pending department step with a reason and both reviewer identities recorded |
| `/technology/position-gpo` | GET | Position GPO revisions, Directory application state, and pending membership jobs |
| `/technology/position-gpo/{preview,publish,retry,retry-config}` | POST | IT preview or versioned publish and retry; Directory readback is separate from endpoint receipt |
| `/technology/organization` | GET | Scope-filtered organization explorer and separate HR action capabilities |
| `/technology/gpo-rules` | GET | Direct and inherited Level/Department/Position rules, effective policy sources and delivery status |
| `/technology/gpo-rules/{preview,publish,remove-preview,remove,retry-config}` | POST | Versioned, previewed IT rule changes with current Company authority and Directory evidence |
| `/people/organization/position-moves[/:id]` | GET | Reviewed Position move and cohort status |
| `/people/organization/position-moves/{preview,submit}` | POST | HR preview or submission of one atomic, effective-dated Position/cohort change |
| `/people/organization/position-moves/:id/cancel` | POST | Cancel a versioned move before it takes effect |

New employee requests save the department policy and selected reviewers at submission. Module decision endpoints enforce the department stage before their specialist stages. Existing submitted approvals retain their saved routes.

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

Slugs are immutable and unique within their route parent. Authenticated People move workflows use opaque source and destination IDs in query parameters so selection survives Back and refresh; reasons, compensation, and private profile fields never go in the URL. The removed short-form namespace has no redirect or compatibility alias.

| Route | Source and behavior |
|---|---|
| `/login` | Live sign-in with existing device accounts |
| `/holdings/{holdingSlug}/home` | Authorized leave, expense, physical-count and transfer priorities/totals and team updates |
| `/holdings/{holdingSlug}/work?queue=todo\|waiting\|completed\|all&q=...` | Role-specific leave, expense, count and transfer queues sharing status/search filters |
| `/holdings/{holdingSlug}/stock-counts` and `/stock-counts/{id}` | Physical-count register, typed preparation, correction and independent posting |
| `/holdings/{holdingSlug}/stock-transfers` and `/stock-transfers/{id}` | Warehouse transfer register, source preparation and independent receipt confirmation |
| `/holdings/{holdingSlug}/finance` | Role-aware finance workbench with live receivable, payable, posting, payment, bank and close queues |
| `/holdings/{holdingSlug}/finance/accounting` | Manual journals and journal review; legacy `/finance/ledger` redirects here |
| `/holdings/{holdingSlug}/finance/close` | Period blockers, close readiness and independently approved close/reopen controls |
| `/holdings/{holdingSlug}/finance/reports` | Company/chart-scoped statements and trial balance |
| `/holdings/{holdingSlug}/finance/engines` | Versioned calculation runs for allocation, cash flow, Thai tax, consolidation, costing, and reconciliation |
| `/holdings/{holdingSlug}/finance/group` | Holding intercompany status and consolidation-run snapshots |
| `/holdings/{holdingSlug}/finance/setup` | Finance readiness and opening-balance import |
| `/holdings/{holdingSlug}/tasks/people_payroll/{id}` | Live leave details, evidence, submission review and independent approval |
| `/holdings/{holdingSlug}/people/move?kind=...&id=...` | HR/Owner chooses a same-Company destination by browsing, search, keyboard, or drag and drop; selected target kind and ID stay in the URL |
| `/holdings/{holdingSlug}/people/move/review?kind=...&id=...&targetKind=...&targetId=...` | Full-page impact preview and submission using existing placement, position-move, or independently reviewed department-relocation commands; server rechecks authority and destination |
| `/holdings/{holdingSlug}/people/payroll/runs/{id}` | Payroll calculation evidence, immutable review exports and independently approved correction/recalculation history |
| `/holdings/{holdingSlug}/feed` | Live updates, comments, reactions, follows and Stories |
| `/holdings/{holdingSlug}/learn` | Captioned sample learning videos |
| `/holdings/{holdingSlug}/inbox` | Explicit notification preview |

The former holding and platform module catalogs, sample-record previews, and
generic record workbench were development UI and are intentionally not public
routes. They return the standard unavailable/not-found response without a
redirect. The universal record APIs above remain for backend compatibility and
historical data access.

Authenticated business-workspace discovery uses one role- and scope-aware
destination catalog. Today, Calendar, and Messages are primary destinations, with Start work and Apps actions.
The sidebar exposes app-level links under presentation-only People &
organization, CRM, Sales, Procurement, Inventory, POS, Hotel PMS, Projects & Routines,
Finance, Management, and System categories. CRM contains customer relationship
pages, Customer directory, and Customer support; Inventory contains Quality;
Projects & Routines contains projects, staff tasks, routine templates and approval routing. Other deep
pages remain in app-local tabs and become launcher or command-search results
only when searched. Redirect aliases, creation states, and individual records
inherit their app parent for active navigation and `App › Page` breadcrumbs but
are not separate default launcher cards. Platform keeps its existing flat
navigation contract.

## Conventions

- **Idempotency:** every command carries `Idempotency-Key` + authoritative tenant/holding scope (from `CONTRACT_POLICY.md`).
- **Versioned namespace:** `/api/v1/platform/...` and the canonical Holding/Local workspace prefixes below `/api/v1/holdings/{holdingSlug}/...` — never reuse a route or field with different meaning; removed fields stay reserved.
- **Verification:** `bun run lint`, `bun run typecheck`, `bun run deploy:build`, and `bun run test:integration` pass. Archived Rust gates stay on `backup/rust-2026-09-02`.

## Links

- Current workflow ownership: [catalogue](catalogue.md); Rust recovery: [CUTOVER](../../../README.md#recovery)
- Spec hub: `docs/TOC.md` → `docs/production-plan.md` (current scope) → affected owner contract.
- Policy: `CONTRACT_POLICY.md` + `docs/adr/ADR-020-nextjs-stack.md` + `docs/adr/ADR-024-local-postgresql-runtime.md` + `docs/adr/ADR-023-three-level-workspace-routing.md`


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

## Recurring accrual schedules

| Route under `/api/v1/holdings/{holdingSlug}/finance` | Method | Behavior |
| --- | --- | --- |
| `/accrual-schedules` | GET | Finance-scoped schedules, due runs and active account options |
| `/accrual-schedules` | POST | Save a draft recurring schedule with amount, frequency and source accounts |
| `/accrual-schedules/{id}/approve`, `/cancel` | POST | Independent Finance approval or controlled cancellation |
| `/accrual-schedules/{id}/prepare` | POST | Prepare the next due period as a draft journal in an open accounting period |
| `/accrual-runs/{id}/post` | POST | Independent approval posts the balanced journal and advances the schedule cursor |

## Company routing attention

`GET /api/v1/holdings/{holdingSlug}/routing-attention?kind={flowKind}&after={lastVisibleRecordId}&limit=25` requires current Company organization-management authority, then owner read permission for each source. Returns only authorized blockers and a next cursor based on visible records. Read-only scan; repairs use existing versioned `/flows/{kind}/{id}` commands. Named Flow coverage only; see [routing simulation](../../testing/routing-attention-simulation.md).

### Sales stock reservations

`POST /api/v1/holdings/{holdingSlug}/commerce/orders/{id}/reservations` — `inventory.prepare`, source Company/Branch/Department scope, approved sales order, `Idempotency-Key` and `expectedVersion`. Reserve/release quantity, reason and reserve expiry; atomic audit and source version. See [simulation contract](../../testing/shared-stock-reservation-simulation.md).

### Business readiness

`GET /api/v1/holdings/{holdingSlug}/business-readiness` — active Company organization manager only, private/no-store. Bundled scoped inventory observations plus separate Company setup/release state; release submission, review and revoke commands are listed below. This read does not grant Company production sign-off. UI: `/holdings/{holdingSlug}/settings/business-readiness`.

## Account time and member profiles

| Route | Methods | Access and meaning |
| --- | --- | --- |
| `/api/v1/account/time-preference` | GET, PATCH | Authenticated account owner; System or Manual IANA timezone and 12/24-hour clock with optimistic version |
| `/api/v1/holdings/{holdingSlug}/profiles/{userId}` | GET | Active Holding member and active subject; public profile plus posts already visible in the current Company scope |
| `/api/v1/holdings/{holdingSlug}/profiles/me` | PATCH | Owner bio/HTTPS links; idempotency key and profile version required |
| `/api/v1/holdings/{holdingSlug}/profiles/me/media/{avatar\|cover}` | POST, DELETE | Owner image upload/removal; size, type, scan, processing, version and idempotency checks |
| `/api/v1/holdings/{holdingSlug}/profiles/{userId}/media/{avatar\|cover}` | GET | Private processed image after active Holding access check |

## Company release acceptance

The existing Company setup API remains compatible. [Release acceptance contract](business-readiness.md) defines Company-scoped evidence, independent review, version/idempotency and configuration invalidation.

| Route | Methods | Owner | Meaning |
| --- | --- | --- | --- |
| `/api/v1/holdings/{holdingSlug}/business-readiness` | GET | Readiness | Setup, reference observations and separate release status |
| `/api/v1/holdings/{holdingSlug}/business-readiness/releases` | GET, POST | Release acceptance | Latest status / submit evidence for the current scope and build |
| `/api/v1/holdings/{holdingSlug}/business-readiness/releases/{id}` | GET | Release acceptance | Company-scoped evidence and effective gates |
| `/api/v1/holdings/{holdingSlug}/business-readiness/releases/{id}/review` | POST | Release acceptance | Independent criterion review |
| `/api/v1/holdings/{holdingSlug}/business-readiness/releases/{id}/revoke` | POST | Release acceptance | Revoke while preserving history |

## People evaluation

| Route | Owner | Access |
| --- | --- | --- |
| `/holdings/{holdingSlug}/people/evaluation` | People evaluation | Scoped `people.read`; compensation separately gated |
| `/api/v1/holdings/{holdingSlug}/people/evaluation/[[...path]]` | People evaluation | Fresh IAM; typed versioned commands; no-store reads |
