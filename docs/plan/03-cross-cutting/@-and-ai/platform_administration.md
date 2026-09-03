# Platform Administration — Deep `@` + AI (Production)

**Group:** `01-platform-governance` · **Code:** `crates/contexts/platform_administration` · **Proto:** `crates/proto/proto/erpx/platform_administration/service.proto` (`PlatformAdministrationCommandService, PlatformAdministrationQueryService, DataPlaneStreamService` · 65 RPCs) · **Adapters:** `crates/adapters/platform_administration-sqlx` · **UI:** `apps/console/src/features/platform_administration` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/01-platform-governance/platform_administration.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (65)

- **Services:** PlatformAdministrationCommandService, PlatformAdministrationQueryService, DataPlaneStreamService
- **RPCs:** ** RegisterHolding, TransitionHolding, RegisterConnector, TransitionConnector, IssueAdminContext, RevokeAdminContext, ProvisionHoldingBranch, RetryHoldingBranchProvisioning, SuspendHoldingBranchRoute, BeginHoldingAdminContext, EndHoldingAdminContext, BeginHoldingProvisioning, ProvisionHoldingResources, ActivateHolding, SuspendHolding, EnterHoldingRecovery, ScheduleHoldingBackup, ScheduleClusterBaseBackup, StartClusterRestoreRehearsal, StartProjectionRebuild, ActivateProjectionRebuild, ReplayDeadLetterOutbox, QueueMigrationWork, BeginMigrationRun, AdvanceMigrationPhase, SaveHoldingInfrastructureProfile, SaveBusinessUnitConfiguration, IssueDataPlaneEnrollment, ProposeDesiredConfiguration, DecideDesiredConfiguration, RequestRootAccess, DecideRootAccess, RevokeRootAccess, SaveCorporateIdentityDraft, PublishCorporateIdentityDraft, RollbackCorporateIdentity, GetHolding, ListHoldings, GetConnector, ListConnectors, GetAdminContext, GetHoldingBranchProvisioning, ListHoldingBranchProvisionings, ListOperationalHoldings, GetOperationalHolding, ResolveActiveHoldingRoute, ListHoldingControlHistory, GetHoldingAdminContext, DiscoverHoldingOrganizationUnits, ListHoldingBackupJobs, ListClusterRecoveryPoints, GetClusterRestoreRehearsal, ListProjectionStatuses, ListPlatformAudit, ListPlatformIntegrations, GetPlatformSettings, ListMigrationStatuses, GetHoldingInfrastructure, GetBusinessUnitConfiguration, ListRootAccessRequests, GetResolvedCorporateIdentity, GetCorporateIdentityDraft, ListCorporateIdentityHistory, GetNavigationManifest, Subscribe

| RPC | Service | Data touch |
|-----|---------|------------|
| `** RegisterHolding` | Command/Query | `platform_administration` aggregate + outbox |
| `TransitionHolding` | Command/Query | `platform_administration` aggregate + outbox |
| `RegisterConnector` | Command/Query | `platform_administration` aggregate + outbox |
| `TransitionConnector` | Command/Query | `platform_administration` aggregate + outbox |
| `IssueAdminContext` | Command/Query | `platform_administration` aggregate + outbox |
| `RevokeAdminContext` | Command/Query | `platform_administration` aggregate + outbox |
| `ProvisionHoldingBranch` | Command/Query | `platform_administration` aggregate + outbox |
| `RetryHoldingBranchProvisioning` | Command/Query | `platform_administration` aggregate + outbox |
| `SuspendHoldingBranchRoute` | Command/Query | `platform_administration` aggregate + outbox |
| `BeginHoldingAdminContext` | Command/Query | `platform_administration` aggregate + outbox |
| `EndHoldingAdminContext` | Command/Query | `platform_administration` aggregate + outbox |
| `BeginHoldingProvisioning` | Command/Query | `platform_administration` aggregate + outbox |

## Invariants (domain — keep)

- Holding provisioning and PITR/backup are platform-only; branch data never leaks across holdings.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `platform_administration` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `holding_provisioning + holding_backup_* + platform_pitr_* + worker_heartbeats` (light `platform_administration.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** RegisterHolding → ProvisionHoldingResources → ActivateHolding — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** provisioning milestones→platform posts; failures→lane (`platform`); PITR drills→System section.
- **Comments:** platform roles only; customer data never in text.

## Route & UI delta

- **Route:** `/platform/holdings + /platform/holding/:id + /work` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title PLATFORM ADMINISTRATION, list platform_administration-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/platform_administration/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/platform_administration/src/domain.rs`
- **Adapters:** `platform_administration-sqlx`
- **UI:** `apps/console/src/features/platform_administration`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `platform_administration`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
