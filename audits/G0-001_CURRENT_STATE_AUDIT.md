# G0-001 Current-State Audit

**Audit date:** 2026-07-24 (Asia/Bangkok)  
**Repository:** `/Users/fluke/Desktop/Work/folio`  
**Scope:** Folio operational pilot, current ERPX workspace, database/migration/runtime state, and required baseline verification. The inventory snapshot was taken before G0-003/G0-004 changes; the delta is recorded near the end of this document.

This is an evidence record, not a claim that the migration is complete. Folio remains the operational write owner until the G13 cutover evidence and G14 audit are complete.

## Executive finding

Folio is a substantial Next.js 16 pilot with active accounting, expense, procurement, sales, inventory, HR, legal, AI, access-control, Operations, and Platform Customer Console surfaces. The worktree already contains an initial ERPX control-plane SQL migration, Platform Console, Operations dashboard, and a Rust/Dioxus prototype.

The ERPX prototype is not production-ready: `erpx/apps/server` constructs `InMemoryControlPlane` at startup, seeds one demo customer, and has no SQLx repository wiring, worker process, migration crate, tenant database provisioning, identity system, Stripe adapter, or complete tenant modules. `erpx/crates/infrastructure` has connection-pool and tenant-registry primitives, but they are not the server's runtime persistence boundary. The current Dioxus console is a single Customer 360 view with a hard-coded local endpoint and a metadata actor, not the shared product described by the target architecture.

## Worktree and runtime evidence

The initial `git status --short --branch` was:

```text
## development
 M .env.example
 M .gitignore
 M components/Sidebar.tsx
 M components/sidebar-config.ts
 M db/setup.sh
 M lib/db.ts
?? app/(app)/(protected)/operations/
?? app/(app)/(protected)/platform/
?? app/api/platform/
?? components/platform/
?? db/migrations/2026-07-23-erpx-control-plane.sql
?? db/migrations/2026-07-24-operations-command-center.sql
?? erpx/
?? lib/operations/
?? lib/platform/
```

These changes are pre-existing user work and are preserved.

| Component | Observed state | Evidence |
|---|---|---|
| Folio dev server | Listening on `*:3004`; `/` returned 200; protected routes redirect with 307 when unauthenticated | `ps`, `lsof`, `curl` |
| ERPX prototype server | Listening on `127.0.0.1:50051` | `ps`, `lsof`; `erpx/apps/server/src/main.rs` |
| PostgreSQL | Local PostgreSQL 18 accepting connections; `folio_db`/`contract` reachable through `/tmp` | `pg_isready`, `psql` |
| MinIO | Running on `127.0.0.1:9100`, console `9101` | `ps`/`lsof` |
| Redis | Running on `6379` | `ps`/`lsof` |
| Ollama | Running on `11434` | `ps`/`lsof` |
| n8n | No local process or listener was observed on the default `5678` probe; a Cloudflare tunnel configuration exists | `ps`, `nc` |
| Git hygiene | `.env`, `.env.local`, `infra/.env`, and `.runtime/` are ignored and not tracked; `.runtime/n8n` exists | `git ls-files`, `git check-ignore`, `find` |

No secret values were included in this audit.

## Verification baseline

Commands required by `AGENTS.md` and the execution contract were run from the specified directories:

| Command | Result | Notes |
|---|---|---|
| `bun run lint` | PASS | 0 errors, 4 warnings in `infra/scripts/start-n8n.js` (`require()` imports and unused `signal`) |
| `bunx tsc --noEmit` | PASS | No diagnostics |
| `cargo fmt --all --check` | PASS | `erpx/` workspace |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | `erpx/` workspace |
| `cargo test --workspace` | PASS | 10 domain tests, 1 infrastructure test; no integration/database tests |
| `cargo check -p erpx-console --no-default-features --features web` | PASS | Local web target check |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS | Local desktop target check |

Passing baseline checks prove only that the current prototype compiles and its existing unit tests pass. They do not prove SQLx persistence, isolation, auth, worker behavior, module completeness, migration reconciliation, mobile targets, or cutover safety.

## Folio route inventory

The route inventory below is derived from `find app -type f` for `page.tsx`, `route.ts`, `layout.tsx`, and `loading.tsx`. Route-group segments are omitted from the public URL. Dynamic segments are shown with `:name`.

### UI routes

| Area | Public routes currently represented |
|---|---|
| Root and shell | `/`, `/:slug`, `/login` |
| Accounting and finance | `/accounting`, `/accounting/periods`, `/audit`, `/budgets`, `/capital`, `/ledger`, `/ledger/:id`, `/reconciliation`, `/reports` |
| Work and AI | `/ai-settings`, `/chat`, `/cockpit`, `/executive`, `/expense`, `/inbox`, `/nudges`, `/tiles`, `/design-system` |
| Customer and sales | `/customers`, `/customers/:id`, `/sales`, `/sales/:id`, `/sales/by-so/:id` |
| Procurement | `/pr`, `/po` |
| HR | `/hr`, `/hr/employees`, `/hr/employees/:id`, `/hr/leave`, `/hr/leave/:id`, `/me`, `/me/leave` |
| Inventory and Operations | `/inventory`, `/operations`, plus the Operations command-center route modules currently under `app/(app)/(protected)/operations/` |
| Platform | `/platform`, plus Platform route modules currently under `app/(app)/(protected)/platform/` |
| Legal/documents | `/law`, `/law/admin`, `/law/upload`, `/law/:docId`, `/law/:docId/audit`, `/law/:docId/chat`, `/law/:docId/chunks`, `/law/:docId/metadata`, `/law/:docId/parties` |
| Waybills | `/waybill/:id`, `/waybill/:id/attachments`, `/waybill/:id/audit`, `/waybill/:id/chat`, `/waybill/:id/gl`, `/waybill/by-expense/:id`, `/waybill/by-po/:id`, `/waybill/by-pr/:id` |

`loading.tsx` files exist for several protected pages and are included in the source inventory even though they do not add URLs.

At the audit boundary the repository contained 56 UI `page.tsx` files, 126 API `route.ts` files, 12 server-action files, 188 `lib/` files, and 185 component files.

### API routes

| Area | Current route families |
|---|---|
| Access/auth | `/api/access-requests`, `/api/access-requests/:id/resolve`, `/api/auth/bootstrap`, `/api/auth/sign-in`, `/api/actor`, `/api/actor/users`, `/api/me/locale`, `/api/users`, `/api/users/:id` |
| Permissions and organization | `/api/perm/audit`, `/api/perm/departments/:id`, `/api/perm/levels`, `/api/perm/permissions`, `/api/perm/roles`, `/api/perm/roles/:id`, `/api/perm/tiles`, `/api/perm/tiles/:id/gate`, `/api/perm/users`, `/api/perm/users/:id/access`, `/api/perm/users/:id/grants`, `/api/perm/users/:id/roles`, `/api/departments`, `/api/org-tree` |
| Platform | `/api/platform/customers`, `/api/platform/customers/:id`, `/api/platform/roles`, `/api/platform/roles/:id/renew`, `/api/platform/sessions`, `/api/platform/sessions/:id`, `/api/platform/sessions/:id/approve`, `/api/platform/tickets` |
| Core work | `/api/command/intent`, `/api/cockpit/narrative`, `/api/cockpit/projection`, `/api/cockpit/sql`, `/api/cockpit/summarize`, `/api/customers/search`, `/api/notifications`, `/api/notifications/:id`, `/api/notifications/:id/open`, `/api/notifications/digest`, `/api/notifications/mark-read`, `/api/tiles`, `/api/tile/explainer` |
| Finance | `/api/finance/bank-import/preview`, `/api/finance/bank-import/commit`, `/api/finance/cashflow`, `/api/finance/rag`, `/api/finance/reports`, `/api/ledger/commentary` |
| Sales/procurement | `/api/sales`, `/api/sales/:id/attach-ar-receipt`, `/api/sales/apply-coa-suggestion`, `/api/sales/coa-suggestions`, `/api/sales/discard-draft`, `/api/sales/extract`, `/api/po/from-invoice`, `/api/waybill`, `/api/waybill/:id/attachments/file`, `/api/waybill/:id/attachments/presign`, `/api/waybill/:id/po`, `/api/waybill/:id/review-hint`, `/api/waybill/apply-coa-suggestion`, `/api/waybill/coa-suggestions`, `/api/waybill/explain-event`, `/api/waybill/nudges`, `/api/waybill/recommend`, `/api/waybill/risk/:id` |
| Expense/slips/uploads | `/api/expense/helper`, `/api/slips`, `/api/slips/file`, `/api/slips/payslip/:expenseId`, `/api/upload`, `/api/ocr`, `/api/vision-native` |
| HR/legal | `/api/hr`, `/api/hr/export`, `/api/hr/leave`, `/api/hr/employee/leave-quota`, `/api/hr/notify-manager`, `/api/law/chunks`, `/api/law/contracts`, `/api/law/index-worker`, `/api/law/preview`, `/api/law/rag`, `/api/law/upload` |
| AI | `/api/ai/assignments`, `/api/ai/chat`, `/api/ai/chat/full`, `/api/ai/chat/full/sessions`, `/api/ai/chat/full/sessions/:id`, `/api/ai/chat/stream`, `/api/ai/chat/tile`, `/api/ai/cockpit/summarize/stream`, `/api/ai/finance/rag/stream`, `/api/ai/invocations`, `/api/ai/invoke`, `/api/ai/ledger/commentary/stream`, `/api/ai/models`, `/api/ai/models/:id`, `/api/ai/preferences`, `/api/ai/providers`, `/api/ai/providers/:id`, `/api/ai/providers/:id/sync-models`, `/api/ai/providers/test`, `/api/ai/sections/health`, `/api/ai/sql`, `/api/ai/staff`, `/api/ai/staff/:id`, `/api/ai/staff/:id/invoke`, `/api/ai/waybill/explain-event/stream`, `/api/ai/waybill/recommend/stream` |
| Hooks/automation/cron | `/api/hook/events`, `/api/hook/events/:id/replay`, `/api/hook/generic`, `/api/hook/line`, `/api/hook/line-hr`, `/api/hook/line-law`, `/api/auto-wire/apply`, `/api/auto-wire/propose`, `/api/cron/approver-nudge`, `/api/exec/brief` |

This inventory is the legacy surface to be mapped context-by-context in G13. It is not a promise that every route is already an ERPX contract.

## Database schema inventory

### Source schema snapshot

`db/schema.sql` defines 9 schemas and 72 base tables:

| Schema | Tables in snapshot | Main responsibility |
|---|---:|---|
| `ai` | 1 | model ratings |
| `auth` | 1 | sessions |
| `chat` | 2 | chat sessions/messages |
| `finance` | 4 in snapshot | cashflow and AI finance support (live DB contains the expanded finance module) |
| `folio` | 45 in snapshot | customers, users, sales, procurement, expense, waybills, ledger, notifications, AI, and operational records |
| `hook` | 2 | providers and events |
| `law` | 4 | contracts, pages/chunks, indexing jobs |
| `perm` | 13 | permissions, roles, grants, policies, audit, tiles, departments |
| `n8n` | no table in the 72-table count | schema declaration only in the snapshot |

The source snapshot includes, among others, `folio.customers`, `customer_contacts`, `users`, `sales_orders`, `purchase_requisitions`, `purchase_orders`, `po_invoices`, `expenses`, `expense_items`, `expense_payments`, `waybills`, `waybill_events`, `waybill_attachments`, `journal_entries`, `ledger_lines`, `hr_leave`, law tables, AI tables, permission tables, and the existing `domain_events` table.

### Live local database

The local `folio_db` database was inspected read-only through `psql -h /tmp -U contract -d folio_db`. It contains the legacy schemas plus later migration objects:

| Schema | Live base tables/views observed |
|---|---:|
| `platform` | 12 control-plane tables |
| `folio` | 46 |
| `finance` | 31 (including accounting/AP/AR/banking/budget/report views) |
| `inventory` | 21 |
| `perm` | 13 |
| `law` | 4 |
| `hook` | 2 |
| `chat` | 2 |
| `ai` | 1 |
| `auth` | 1 |

The live `public` schema also contains 108 base tables, including n8n state tables such as `workflow_entity`, `execution_entity`, and `credentials_entity`. This state is operationally present but is not part of Folio's ERPX business schema and must be handled explicitly in backup/runtime separation.

Only two live tables currently have PostgreSQL row-level security enabled: `chat.messages` and `chat.sessions`. Platform and tenant isolation are therefore application/prototype concerns today, not database-enforced production boundaries.

Read-only live row counts at audit time were: `folio.users` 17, `folio.expenses` 7, `folio.waybills` 6, `finance.journals` 10, and all Platform control-plane customer/role/ticket/session/audit tables 0. These counts are baseline runtime facts, not migration acceptance targets.

The current Platform migration creates: `catalog_revisions`, `catalog_items`, `customer_accounts`, `holdings`, `companies`, `branches`, `customer_users`, `entitlements`, `support_tickets`, `support_sessions`, `support_access_audit`, and `role_assignments` in the same `folio_db` instance. It references legacy `folio` users/customers and is therefore a control-plane prototype, not physically separated control write/read databases.

`db/setup.sh` applies `schema.sql`, `seed.sql`, and every dated SQL file sequentially. It is an initialization/reset script, not a versioned SQLx migration runner. Reset is guarded and creates a dump under `.runtime/backups`; no destructive command was run during this audit.

## Permission and role inventory

The legacy authorization source is `perm` plus `lib/perm`:

- The source baseline has five departments: `it`, `hr`, `accounting`, `finance`, `executive`, and fourteen hierarchy roles: `it_manager`, `it_supervisor`, `it_officer`, `hr_manager`, `hr_supervisor`, `hr_officer`, `accounting_manager`, `accounting_supervisor`, `accounting_officer`, `cfo`, `finance_manager`, `finance_supervisor`, `finance_officer`, and `ceo`.
- The live database after later reference-user migrations has 6 departments, 17 roles, and 229 permission rows. The additional Sales roles/department come from `db/migrations/2026-07-19-J-local-reference-users.sql`; this source/live difference must be reconciled rather than silently collapsed.
- Legacy assignment tables: `user_departments`, `user_roles`, `user_permissions`, `role_permissions`, and `department_permissions`.
- Permission IDs use `<domain>:<subject>:<verb>[:<qualifier>]::<effect>`, with explicit deny precedence and `admin:system:bypass::allow` as the bypass marker.
- The seed/migrations contain 260 unique permission literals matching the `::allow` shape; the live `perm.permissions` table currently has 229 rows. These are different measures (source literals versus deduplicated live rows) and are both retained as evidence.
- Legacy direct grants may be permanent or time-bound; universal expiry, parent assignment cascade, authority levels, risk tiers, and immutable assignment versions are not uniformly enforced by Folio.
- Platform permissions currently added by the control-plane migration are `platform:customer:view::allow`, `platform:customer:manage::allow`, `platform:support:manage::allow`, and `platform:rbac:review::allow`.

Relevant source map: `db/seed.sql`, `docs/RBAC-STANDARD.md`, `lib/perm/grammar.ts`, `lib/perm/auth.ts`, `lib/perm/access.ts`, `lib/perm/chain.ts`, `lib/perm/stages.ts`, `lib/perm/deptGrant.ts`, and `lib/org/scope.ts`.

## Business-status and transition inventory

### Legacy approval/workflow states

`lib/perm/stages.ts` and waybill actions use the canonical/legacy-compatible stage set:

`submission → department_approval → accounting_review → accounting_approval → executive_approval → payment → settlement`, plus `dept_verification`, `dept_authorization`, `accounting_verification`, `accounting_supervision`, `accounting_authorization`, `disbursement_authorization`, `cfo_authorization`, `ceo_authorization`, `awaiting_disbursement`, `disbursed`, `rejected`, purchase-order states, and sales-order states (`so_draft`, `so_sales_review`, `so_dept_approval`, `so_credit_check`, `so_invoiced`, `so_paid`).

The existing action code performs row locking and rejects stale stage transitions for expenses, requisitions, purchase orders, sales orders, waybills, and payment/settlement actions. Operations currently reads these records and groups statuses; it does not own them.

### Existing Platform states

- Customer account: `prospect`, `onboarding`, `active`, `suspended`, `churned`.
- Billing: `trialing`, `active`, `past_due`, `grace`, `read_only`, `canceled`.
- Support ticket schema: `new`, `triaged`, `waiting_for_customer`, `access_requested`, `approved`, `in_progress`, `resolved`, `customer_confirmed`, `closed`.
- Support modes: `diagnostic_read` (maximum 8h), `tenant_mutation` (maximum 2h), `impersonation` (maximum 2h).
- Role assignment: bounded `valid_from`/`valid_until`, parent reference, risk tier, and assignment version are present in the prototype schema/domain, but no production write-side evaluator exists.

The current Rust `SupportTicket::transition` omits several required edges (`triaged → in_progress`, `waiting_for_customer → triaged/in_progress`, `in_progress → access_requested/waiting_for_customer`, and `resolved → in_progress`) and incorrectly permits `resolved → closed` directly. This is a G4 contract gap to fix before support acceptance.

### Required ERPX transitions not yet implemented

Stripe webhook inbox/reconciliation, billing grace/read-only recovery, catalog publication/grandfathering, provisioning states, incidents, marketing campaign/journey lifecycle, finance period controls, lead-to-cash, procure-to-pay, inventory movements/recosting, asset/maintenance, HR/payroll, legal retention/versioning, POS, projects, service, manufacturing, quality, AI tools, integrations, migration cutover, and Folio write retirement have no complete ERPX implementation in the current workspace.

## Rust crate and contract inventory

### Workspace

Current workspace members (`erpx/Cargo.toml`):

- `crates/domain`: `access`, `commercial`, `cqrs`, `customer`, `support`.
- `crates/application`: store ports and use cases for Customer 360, support, role lifecycle, and entitlement authorization.
- `crates/infrastructure`: `ControlPlaneDatabaseConfig`, `ControlPlanePools`, `TenantStoreRegistry`, `InMemoryControlPlane`, and an in-memory `ProjectionRunner`.
- `crates/proto`: one `erpx.platform.v1` protobuf package.
- `apps/server`: Tonic services for platform customer, support, role lifecycle, and entitlement checks.
- `apps/console`: one Dioxus Customer 360 console.

At the G0-001 boundary, missing target workspace members were `crates/migration`, `apps/worker`, `apps/marketing-web`, and `apps/migrator`. G0-004 has since added `crates/migration`; the remaining three are still absent.

### Current proto surface

`crates/proto/proto/erpx/platform/v1/platform.proto` exposes four services: `PlatformCustomerService`, `SupportService`, `RoleLifecycleService`, and `EntitlementService`. Messages cover customer summaries, tickets, support sessions, role assignments, and a simple entitlement check. The contract has no command/query context, consistency metadata, event envelope, typed error model, tenant scope, actor attribution, aggregate version, idempotency key, audit context, support capability context, or compatibility policy.

### Runtime boundary

`apps/server/src/main.rs` requires only `x-erpx-actor` metadata for most calls, uses an HMAC approval token for one support approval path, seeds `DEMO-TH-001`, and serves on `127.0.0.1:50051`. It does not construct `ControlPlanePools`, use SQLx repositories, enforce authoritative identity/session/RBAC/entitlement checks, expose health/readiness/metrics, provide raw-body Stripe webhooks, or redact/standardize all errors.

## Dioxus feature inventory

`apps/console` has one component tree with Cargo features `web`, `desktop`, and `mobile`; the local baseline verified web and desktop only. It calls `http://127.0.0.1:50051`, places a constant `x-erpx-actor: dioxus-platform-console` metadata value on requests, and renders Customer 360 list/detail, health, billing state, support counts, and an audit banner. It has no shared router, authenticated session storage, Thai/English localization, scope selector, support/impersonation/read-only banners, forms/validation, file access, offline drafts, notifications, deep links, conflict recovery, WCAG audit, or production Android/iOS CI target checks.

## Migration and cutover audit

At the G0-001 boundary, no deterministic migration fixture set, legacy-to-ERPX ID map, context-by-context backfill, shadow projection comparison, reconciliation report, rollback rehearsal, command-owner flag, cohort rollout, mutation freeze, or Folio read-only retirement mechanism existed in `erpx/`. G0-004 now supplies a deterministic fixture crate/dataset, but the remaining G13 controls are still absent. The existing Platform migration seeds customer accounts from `folio.customers` in-place and links to legacy IDs. This is useful bootstrap evidence but does not satisfy G13 and must not become a long-term dual-write design.

## G0-003/G0-004 audit delta

After the initial inventory snapshot, the main agent added and independently verified:

- `crates/domain/src/contracts.rs`: versioned command/query/event/error/tenant/actor/audit contracts with idempotency, purpose, correlation, and scope-containment tests.
- `crates/proto/proto/erpx/common/v1/context.proto`: transport envelopes; platform request messages now carry additive command/query context fields under the compatibility policy.
- `CONTRACT_POLICY.md`: additive protobuf/event compatibility, reserved-field, idempotency, consistency, stable-error, and support-context rules.
- `crates/migration` and `fixtures/migration/deterministic.json`: deterministic anonymized fixtures across the required G0 domains, with typed validation and balanced-journal/holding-reference checks.

The final G0 follow-up verification passed: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (17 tests), server check, and Dioxus web/desktop checks. These additions do not change the finding that production persistence, identity, authoritative authorization, workers, tenant databases, and ERPX business modules remain incomplete.

## G0 acceptance gaps and next actions

1. Freeze a shared Rust/protobuf contract for contexts, errors, tenant scope, actor/audit/support context, versions, idempotency, and consistency.
2. Add deterministic anonymized fixtures for organization, access, commercial, support, Operations, marketing consent, finance, SCM, assets, maintenance, HR, and legal.
3. Add SQLx migrations and repositories for physically separate control write/read databases; do not point production code at the existing in-memory store.
4. Correct the support transition contract and make approval/scope/permission/module/agent/session/billing/RBAC checks authoritative on the write side.
5. Create the worker/projector/replay/dead-letter/checkpoint foundations before business-context fan-out.
6. Keep `bun run dev` on port 3004 and the existing Folio database/runtime untouched during ERPX integration.

## Audit evidence index

- Folio package/runtime: `package.json`, `AGENTS.md`, `lib/db.ts`, `db/setup.sh`.
- Folio UI/API route source: `app/` (56 UI pages, 126 API route files, 12 server-action files at the audit boundary; raw manifests are in `audits/G0-001/evidence/routes.txt`).
- Folio schema and seed: `db/schema.sql`, `db/seed.sql`, `db/migrations/`.
- Folio authorization: `docs/RBAC-STANDARD.md`, `lib/perm/`, `lib/org/`.
- Operations: `lib/operations/`, `app/(app)/(protected)/operations/`.
- Platform: `lib/platform/`, `components/platform/`, `app/(app)/(protected)/platform/`, `db/migrations/2026-07-23-erpx-control-plane.sql`.
- ERPX architecture/prototype: `erpx/README.md`, `erpx/IMPLEMENTATION_PLAN.md`, `erpx/Cargo.toml`, `erpx/crates/`, `erpx/apps/`.
- Baseline commands: recorded above, in `audits/G0-001/evidence/baseline.txt`, and in `erpx/EXECUTION_LEDGER.md`.
