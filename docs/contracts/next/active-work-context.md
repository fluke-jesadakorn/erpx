# Active business work context — 2026-09-14

Business authorization uses one selected assignment. This supersedes the combined-responsibility and preferred-role behavior described in earlier workplace plans. Platform authorization remains separate.

## Identity and authorization

`AccountAccessContext` resolves a signed-in account's own valid assignments and workspace destinations. It is used by landing navigation, the company selector and role recovery. It cannot authorize a business command.

`WorkContext` contains one active assignment, role, scope and permitted modules, plus `authority.sessionId`, `assignmentVersion` and `revision`. `assignments` has one entry; `availableAssignments` is only picker data. Policy helpers never search that picker list for a more powerful role. An account with multiple grants chooses when it has no valid saved selection. A new session restores the account’s saved assignment for that company after checking ownership, validity, version and scope; exactly one eligible company assignment is selected automatically on first entry. Business role preference cookies and the Automatic/null option no longer authorize anything.

The holding database stores `active_work_contexts`, keyed by session ID and company ID, with the owning user ID and expiry. Session tokens are never stored there. Each request validates the authenticated session, current grant, validity dates, version, company and organization scope. Other devices have different session contexts. `work_role_preferences` stores the last explicit choice by user and company, independently of session expiry. Restoration creates a new audited session context and does not change a context already open on another device. A tab sends its rendered company explicitly, so changing the company navigation cookie in another tab does not change its API authority.

Reports-to decisions require the active `people_manager` role and the saved eligible reporting route. Accounting preparation, Accounting approval/posting and Treasury actions retain their separate role and independence checks. Levels and reporting relationships grant no permissions. Candidate routing searches all valid grants belonging to each candidate; it does not depend on which role that candidate is currently using. Existing route snapshots remain intact. HR organization issues identify managers missing a covering People Manager grant; no grants are created automatically.

Identity-based access remains explicit: a person's own requests, assigned work, direct conversations and eligible channel membership keep their existing participation rules. A directly named task reviewer can review that assigned task; this does not grant Reports-to or spending approval. No identity rule grants HR, Accounting, Treasury or ownership administration. Invisible records continue to return 404 without revealing their existence; an unauthorized action on accessible work returns 403.

## HTTP contract

For any existing holding/branch/department API base:

- `GET assignments/active` returns `{valid, options, assignmentId, revision, companyId}`. Options contain stable assignment IDs, role, company/scope IDs, readable scope labels, assignment version, expiry and an authorized Home URL. The response is private and uncached.
- `POST assignments/active` accepts `{assignmentId, revision}`. Both are required; `null` is invalid. The server checks ownership and current eligibility, compares the revision, changes the selection atomically, increments its revision and records an audit event. It returns the assignment ID, revision and Home URL.
- Business mutations send `x-erpx-context-revision`, `x-erpx-context-assignment` and `x-erpx-context-company` from the rendered page, together with their existing record version and idempotency key. Supplied context headers are also validated on reads. The proxy derives the actual HTTP method and route scope; caller-provided method/scope headers cannot bypass checks.
- `409 WORK_ROLE_REQUIRED` means selection is needed. `409 WORK_ROLE_UNAVAILABLE` means the selected grant is no longer valid or does not cover the workspace. `409 WORK_CONTEXT_CHANGED` means the displayed context changed. Recovery uses `/work-role?from=<workspace path>` and the account-only endpoints, even after revocation. Invalid action roles return 403. Assignment changes never fall back to Owner.

`workFetch` attaches the rendered context to client requests. Role selection and company recovery intentionally use the account-only endpoints. API clients must upgrade before enforcement; an old mutation without context headers is rejected.

## Transactions, evidence and jobs

Role switching and business writes lock the same context row. A business transaction rechecks the context, assignment/version/validity/scope, record version and domain independence guards before committing writes, receipt, audit and outbox. A racing command either commits under the still-selected role before the switch, or fails without business writes. Legacy Finance, People/payroll and generic record writes now use this shared command transaction.

Idempotency fingerprints include assignment, role, original authority and company/workspace scope. Replays revalidate the original authority; a key cannot be reused across roles. Legacy receipts lacking that fingerprint are preserved and rejected for ambiguous replay instead of receiving invented authority.

New command/audit evidence includes the actual role, assignment ID/version, grant scope, rendered workspace and context revision. Approval evidence records the originating active authority alongside existing route and independence evidence. A trigger copies the command authority into newly inserted ledger decision evidence. Historical evidence is not backfilled.

Expense, request and social AI jobs retain the originating actor/assignment/version/context. Workers recheck that original assignment and the job's source permissions; they never choose the user's latest role or an Owner grant. Expired or revoked origin authority blocks the job. Previously queued expense proposals without an origin snapshot become stale and must be requested again. System maintenance/scheduled routines continue using their explicit system identity.

## Browser behavior

The searchable selector shows role and company/branch/department together. The header retains the active role on mobile and desktop; action panels show the server-resolved “Approve as …” or “Act as …”. Changing role warns about unsaved input before navigating to an authorized Home and discarding old page caches.

BroadcastChannel and focus revalidation detect another tab's changes. Stale content becomes hidden and inert while mounted form state is retained. Submission is blocked, and the server independently rejects stale requests. Reload requires confirmation before discarding unsaved input. Neither the browser nor the server silently changes to a more powerful role.

## Rollout and verification

1. Apply additive holding-write migrations `0015_active_work_context.sql`, `0016_queued_work_authority.sql` and `0017_decision_work_authority.sql` before running the new UI/server.
2. Release UI and server together. Existing multi-role sessions must make an explicit choice; do not copy old cookie preferences into the context table.
3. Review People Manager readiness issues in Organization and repair through authorized HR access management. Do not rewrite submitted routes.
4. Retain existing evidence. Expired context rows confer no access because every request checks the real session and grant.

Automated coverage is in `tests/active-work-context.test.ts`, `tests/preferred-role.test.ts`, spending/expense, business command, employee service, Work and AI tests, and `tests/e2e/preferred-role.spec.ts` / `auth-smoke.spec.ts`. Coverage includes stale/racing writes, revoked/expired/future/foreign grants, scope inheritance, identical role names in different branches, company and session isolation, audit and cross-role replay, People Manager versus Employee, Treasury versus Accounting, a 17-role account's direct API denials, two tabs and a single-assignment employee.

The development database has the migrations applied. The public development origin remains `https://erpx.jesadakorn.com` through port 3100; the installed port 3101 production service is unchanged. Builds use the isolated release builder.

Verification on 2026-09-14: typecheck, lint and the isolated Node 24.20.0 release build passed; 331 unit/integration tests and 24 desktop/mobile Playwright cases passed. The authenticated public UI switched Employee → Finance Approver → Employee, opened Finance under the selected reviewer role, returned Home when selecting Employee, and displayed the role at 390px without document overflow. Home and Finance returned HTTP 200; both local/public health endpoints returned HTTP 200. Dev remained running on port 3100.

## Remembered workspace — 2026-09-15

The control database stores the last area in `account_workspace_preferences` and the last company for each holding in `account_company_preferences`, keyed by account. These preferences survive sign-out and browser-cookie removal. An explicit login return URL wins; otherwise `/` resolves the saved area against current assignments. Invalid company preferences recover to an accessible company. Invalid saved roles require selection and never grant new permissions.

The mounted workspace saves its area through `POST /api/v1/workspace-preference`; prefetching a server page does not save it. Business requests include the rendered authority headers. Role and company selection endpoints also save the destination before returning success. Platform role preferences are stored separately from business-role authority. Existing cookies are a fallback for accounts without database preferences.
