# 04 — Interfaces (Codebase-Derived)

**Source:** `crates/proto/proto/erpx/*/service.proto`, `apps/console/src/routes.rs`, `apps/server/src/*`, `crates/console-*`, `crates/ui-domains/*`.

## 4.1 gRPC services

All services are Tonic. Each bounded context exposes `*CommandService` + `*QueryService` (+ `*StreamService` where noted). Descriptor pinned (see `CUTOVER.md` SHA `01fd64d...`).

| Context | Services | Notable RPCs |
|---|---|---|
| agent | AgentOrchestrationCommand/Query | RegisterAgent, AddAgentVersion, ApproveAgentVersion, Suspend/RetireAgent, Start/Delegate/Retry/CancelAssignment, Register/Replace/RoutePool, Start/Append/Publish/SetLegalHold/ExpirePrivateSession, Request/Start/Complete/Apply/RejectFieldSuggestion |
| assets | AssetsCommand/Query | Create, PlaceInService, MarkDegraded, BeginMaintenance, ReturnToService, Retire |
| benefits | BenefitsCommand/Query | PublishPlan, OpenEnrollment, AddDependent, ElectCoverage, Submit/AcknowledgeProvider, ReleasePayroll |
| brand | BrandCommand/Query | CreateDraft, SaveDraft, PublishDraft, Rollback, SetCategoryLocks |
| collaboration | CollaborationCommand/Query/Stream | CreateThread, Add/Edit/Tombstone/ModerateComment, TransitionThread, SetReaction/Following, WithdrawMention, ProposeActionCard… + attachment/typed-mention/phase2 tables (`0003..0006`) |
| commercial | CommercialCommand/Query | Stripe-authoritative catalog/billing/entitlement (ADR-007) |
| crm | CrmCommand/Query | — |
| customer_360 | Customer360Command/Query | — |
| customer_authority | CustomerAuthorityCommand/Query | — |
| ecommerce | EcommerceCommand/Query | — |
| field_service | FieldServiceCommand/Query | — |
| finance | FinanceCommand/Query + PayablesCommand/Query | Chart, subledgers, banking, budget, tax, close; banking provider import jobs |
| identity_access | IdentityAccessCommand/Query | Principal/Role/Grant, AccessReview, BreakGlassSession, expiring assignments |
| inventory | InventoryCommand/Query | — |
| learning | LearningCommand/Query | — |
| legal | LegalCommand/Query | — |
| maintenance | MaintenanceCommand/Query | — |
| manufacturing | ManufacturingCommand/Query | — |
| marketing | MarketingCommand/Query | provider accounts, abuse windows, delivery scheduler |
| operations | OperationsCommand/Query | — (see ADR-008) |
| organization | OrganizationCommand/Query | holdings/companies/branches, sharing |
| people_payroll | PeoplePayrollCommand/Query | — |
| planning | PlanningCommand/Query | supply & demand planning |
| platform_administration | PlatformAdministrationCommand/Query + DataPlaneStreamService | holding provisioning, backup/PITR, data-plane |
| pos | PosCommand/Query | including refunds |
| procurement | ProcurementCommand/Query | — |
| projects | ProjectsCommand/Query | — |
| quality | QualityCommand/Query | — |
| sales | SalesCommand/Query | quote workflow, posting to finance |
| support | SupportCommand/Query | tickets/sessions, capabilities, SLA, attachments |
| talent | TalentCommand/Query | recruiting & talent |

Common/interface/catalog/reference are shared protos (`common/context.proto`, `interface/reference.proto`, `catalog/active.proto`).

Total: ~571 RPCs. Run `grep -r "rpc " crates/proto --include="*.proto" | wc -l` to recount.

## 4.2 Console routes (`AppRoute`)

Public: `/public/marketing/form/:id`, `/public/marketing/preview/:id`, `/`, `/work?:view&:domain&:severity&:cursor`, `/processes`, `/insights`, `/modules`.
Auth: `/auth/sign-in`, `/auth/accounts`, `/auth/security`, `/auth/devices`, `/auth/recovery`.
Settings: `/settings/ai-provider`, `/settings/billing`.
Platform: `/platform/holdings`, `/platform/holdings/:holding_id`, `/platform/holding/:holding_id`, `/platform/support`, `/platform/support/:id`, `/platform/incidents`, `/platform/incidents/:id`.
Identity: `/identity/principal`, `/identity/principal/:id`, `/identity/role`, `/identity/role/:id`, `/identity/grant`, `/identity/grant/:id`, `/identity/access-review`, `/identity/access-review/:id`, `/identity/break-glass-session`, `/identity/break-glass-session/:id`.
Reference: `/reference/item/:id`.
Workspaces (21): `/operations`, `/finance`, `/sales`, `/procurement`, `/inventory`, `/asset`, `/maintenance`, `/quality`, `/marketing`, `/people`, `/pos`, `/legal`, `/manufacturing`, `/planning`, `/project`, `/commerce`, `/talent`, `/learning`, `/benefits`, `/field-service`, `/ai` plus `WorkspaceResource`/`WorkspaceRecord` nested routes.

## 4.3 Console → server binding

`apps/console/src/bff.rs` + `crates/console-bff`, `crates/console-transport*`, `crates/console-shell`. Auth via `web-session`; Tonic-Web bridging via `third_party/tonic-web-wasm-client`. RBAC enforced server-side; proxy identity headers never trusted.
