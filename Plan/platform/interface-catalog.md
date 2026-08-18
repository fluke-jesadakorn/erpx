# Interface Catalog Requirements and Baseline

This catalog separates the observed legacy interfaces in the source snapshot
from the accepted clean-rebuild target. It does not create public APIs or
customer API keys. [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md)
is authoritative where the source snapshot differs.

## Catalog requirements

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-IFC-001 | Catalog identity | ERPX shall assign a stable catalog ID to every UI/BFF route, typed service, integration event, provider adapter, and data-plane operation. | Interfaces need durable governance. | Success: each entry has one ID; Denial: uncataloged target interface cannot be approved; Failure: duplicate ID fails validation. | [FSD-001](../decisions/FSD-001-baseline-and-governance.md) | E2E-30 |
| REQ-IFC-002 | Required metadata | ERPX shall record owner, caller/consumer, scope, authn/authz, idempotency, version, classification, failure semantics, lifecycle, and source. | Reviewers need one complete interface record. | Success: all fields are populated; Denial: unknown ownership blocks acceptance; Failure: incompatible metadata fails validation. | [FSD-001](../decisions/FSD-001-baseline-and-governance.md) | E2E-30 |
| REQ-IFC-003 | Internal-only boundary | ERPX shall expose developer operations only through internal typed services, BFFs, adapters, imports, and exports. | A public platform was explicitly excluded. | Success: approved internal caller is authenticated; Denial: customer API-key issuance is unavailable; Failure: adapter outage cannot bypass the boundary. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-IFC-004 | Version and cutover | ERPX shall version every interface and perform the clean-rebuild transition as an atomic hard cutover without legacy route, service, event, or schema aliases. | One coherent contract generation prevents ambiguous authority. | Success: the signed artifact exposes only the approved v2 catalog; Denial: legacy identifiers cannot enter the new runtime; Failure: failed cutover returns to the preserved environment or governed forward recovery. | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | E2E-15, E2E-30 |
| REQ-IFC-005 | Command idempotency | ERPX shall declare the idempotency identity and conflict semantics for each command interface. | Retry safety must be explicit. | Success: retry returns prior result; Denial: changed payload under same key conflicts; Failure: uncertain commit is queryable. | [ADR-002](../../adr/ADR-002-cqrs.md) | E2E-30 |
| REQ-IFC-006 | Query consistency | ERPX shall declare authoritative, projection, freshness, paging, and audit semantics for each query interface. | Consumers need truthful reads. | Success: response declares semantics; Denial: protected query reveals no hidden count; Failure: stale/unavailable state is explicit. | [ADR-002](../../adr/ADR-002-cqrs.md) | E2E-16, E2E-30 |
| REQ-IFC-007 | Event contract | ERPX shall declare producer, consumers, key, ordering, version, classification, replay, and deletion semantics for each integration event. | Events cross ownership boundaries. | Success: consumer replays idempotently; Denial: consumer cannot write producer state; Failure: gap enters reconciliation. | [Integration contracts](../../crates/integration-contracts/src/lib.rs) | E2E-16, E2E-30 |
| REQ-IFC-008 | Catalog reconciliation | ERPX shall reconcile implementation catalogs against exactly 31 context owners, 21 module workspaces, 65 replacement service IDs, singular routes, typed events, and schema generation v2. | Documentation and runtime must not drift silently. | Success: the implemented typed catalog matches this baseline; Denial: a duplicate, omitted, retired, or uncataloged identifier fails; Failure: parser uncertainty reports a blocking error. | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | E2E-16, E2E-30 |

## Replacement protobuf service catalog

The active clean-rebuild contract set contains exactly 65 internal services.
Each bounded context owns one Command and one Query service; the three
cross-cutting entries are read/stream adapters without business write
authority. Names, IDs, and ownership are normative. Implementation packages
use context-specific `erpx.<context>.v1` namespaces rather than the legacy
monolithic package.

| Required metadata | Resolved value for every replacement `IFC-SVC-*` entry |
|---|---|
| Caller/consumer | Authenticated ERPX UI delivery adapters, workers, owning services, data-plane agents, or named provider adapters; never an anonymous/public API caller |
| Scope | Mandatory tenant plus applicable holding/company/branch, realm, record, field, customer-support, provider, and purpose scope; route parameters are not trusted authority |
| Authentication/authorization | ERPX human session or allow-listed service principal plus owner-context operation and current policy decision on every request or stream resume |
| Idempotency | Commands require caller key, payload digest, conflict response, and prior-result retrieval; queries are non-mutating; stream cursors resume idempotently |
| Version | Context-specific internal `erpx.<context>.v1`; incompatible changes require a new version, while the clean cutover provides no legacy alias |
| Classification | Public/Internal/Confidential/Restricted field classification with strictest-field propagation into messages, events, traces, provider payloads, and evidence |
| Failure semantics | Fail closed on identity, scope, policy, routing, classification, or ownership uncertainty; expose typed retryability, correlation, freshness, and reconciliation state |
| Lifecycle and source | Target for baseline 2.0.0 under FSD-006; an entry becomes Active only when implementation and acceptance evidence pass |

| Catalog ID | Service | Owner | Kind | Lifecycle/source |
|---|---|---|---|---|
| IFC-SVC-101 | IdentityAccessCommandService | Identity & Access | Command | Target via FSD-006 |
| IFC-SVC-102 | IdentityAccessQueryService | Identity & Access | Query | Target via FSD-006 |
| IFC-SVC-103 | PlatformAdministrationCommandService | Platform Administration | Command | Target via FSD-006 |
| IFC-SVC-104 | PlatformAdministrationQueryService | Platform Administration | Query | Target via FSD-006 |
| IFC-SVC-105 | OrganizationCommandService | Organization | Command | Target via FSD-006 |
| IFC-SVC-106 | OrganizationQueryService | Organization | Query | Target via FSD-006 |
| IFC-SVC-107 | CommercialCommandService | Commercial | Command | Target via FSD-006 |
| IFC-SVC-108 | CommercialQueryService | Commercial | Query | Target via FSD-006 |
| IFC-SVC-109 | BrandCommandService | Brand | Command | Target via FSD-006 |
| IFC-SVC-110 | BrandQueryService | Brand | Query | Target via FSD-006 |
| IFC-SVC-111 | CustomerAuthorityCommandService | Customer Authority | Command | Target via FSD-006 |
| IFC-SVC-112 | CustomerAuthorityQueryService | Customer Authority | Query | Target via FSD-006 |
| IFC-SVC-113 | Customer360CommandService | Customer 360 | Command | Target via FSD-006 |
| IFC-SVC-114 | Customer360QueryService | Customer 360 | Query | Target via FSD-006 |
| IFC-SVC-115 | SupportCommandService | Support | Command | Target via FSD-006 |
| IFC-SVC-116 | SupportQueryService | Support | Query | Target via FSD-006 |
| IFC-SVC-117 | FinanceCommandService | Finance | Command | Target via FSD-006 |
| IFC-SVC-118 | FinanceQueryService | Finance | Query | Target via FSD-006 |
| IFC-SVC-119 | SalesCommandService | Sales | Command | Target via FSD-006 |
| IFC-SVC-120 | SalesQueryService | Sales | Query | Target via FSD-006 |
| IFC-SVC-121 | PointOfSaleCommandService | Point of Sale | Command | Target via FSD-006 |
| IFC-SVC-122 | PointOfSaleQueryService | Point of Sale | Query | Target via FSD-006 |
| IFC-SVC-123 | ProcurementCommandService | Procurement | Command | Target via FSD-006 |
| IFC-SVC-124 | ProcurementQueryService | Procurement | Query | Target via FSD-006 |
| IFC-SVC-125 | InventoryCommandService | Inventory | Command | Target via FSD-006 |
| IFC-SVC-126 | InventoryQueryService | Inventory | Query | Target via FSD-006 |
| IFC-SVC-127 | AssetsCommandService | Assets | Command | Target via FSD-006 |
| IFC-SVC-128 | AssetsQueryService | Assets | Query | Target via FSD-006 |
| IFC-SVC-129 | MaintenanceCommandService | Maintenance | Command | Target via FSD-006 |
| IFC-SVC-130 | MaintenanceQueryService | Maintenance | Query | Target via FSD-006 |
| IFC-SVC-131 | QualityCommandService | Quality | Command | Target via FSD-006 |
| IFC-SVC-132 | QualityQueryService | Quality | Query | Target via FSD-006 |
| IFC-SVC-133 | OperationsCommandService | Operations | Command | Target via FSD-006 |
| IFC-SVC-134 | OperationsQueryService | Operations | Query | Target via FSD-006 |
| IFC-SVC-135 | PeoplePayrollCommandService | People & Payroll | Command | Target via FSD-006 |
| IFC-SVC-136 | PeoplePayrollQueryService | People & Payroll | Query | Target via FSD-006 |
| IFC-SVC-137 | LegalCommandService | Legal | Command | Target via FSD-006 |
| IFC-SVC-138 | LegalQueryService | Legal | Query | Target via FSD-006 |
| IFC-SVC-139 | MarketingCommandService | Marketing | Command | Target via FSD-006 |
| IFC-SVC-140 | MarketingQueryService | Marketing | Query | Target via FSD-006 |
| IFC-SVC-141 | CrmCommandService | CRM | Command | Target via FSD-006 |
| IFC-SVC-142 | CrmQueryService | CRM | Query | Target via FSD-006 |
| IFC-SVC-143 | ManufacturingCommandService | Manufacturing | Command | Target via FSD-006 |
| IFC-SVC-144 | ManufacturingQueryService | Manufacturing | Query | Target via FSD-006 |
| IFC-SVC-145 | PlanningCommandService | Supply & Demand Planning | Command | Target via FSD-006 |
| IFC-SVC-146 | PlanningQueryService | Supply & Demand Planning | Query | Target via FSD-006 |
| IFC-SVC-147 | ProjectsCommandService | Projects | Command | Target via FSD-006 |
| IFC-SVC-148 | ProjectsQueryService | Projects | Query | Target via FSD-006 |
| IFC-SVC-149 | EcommerceCommandService | E-commerce | Command | Target via FSD-006 |
| IFC-SVC-150 | EcommerceQueryService | E-commerce | Query | Target via FSD-006 |
| IFC-SVC-151 | TalentCommandService | Recruiting & Talent | Command | Target via FSD-006 |
| IFC-SVC-152 | TalentQueryService | Recruiting & Talent | Query | Target via FSD-006 |
| IFC-SVC-153 | LearningCommandService | Learning | Command | Target via FSD-006 |
| IFC-SVC-154 | LearningQueryService | Learning | Query | Target via FSD-006 |
| IFC-SVC-155 | BenefitsCommandService | Benefits | Command | Target via FSD-006 |
| IFC-SVC-156 | BenefitsQueryService | Benefits | Query | Target via FSD-006 |
| IFC-SVC-157 | FieldServiceCommandService | Field Service | Command | Target via FSD-006 |
| IFC-SVC-158 | FieldServiceQueryService | Field Service | Query | Target via FSD-006 |
| IFC-SVC-159 | CollaborationCommandService | Collaboration | Command | Target via FSD-006 |
| IFC-SVC-160 | CollaborationQueryService | Collaboration | Query | Target via FSD-006 |
| IFC-SVC-161 | AgentOrchestrationCommandService | Agent Orchestration | Command | Target via FSD-006 |
| IFC-SVC-162 | AgentOrchestrationQueryService | Agent Orchestration | Query | Target via FSD-006 |
| IFC-SVC-163 | CollaborationStreamService | Collaboration | Stream | Target via FSD-006 |
| IFC-SVC-164 | ReferenceQueryService | Item Directory projection | Query | Target via FSD-006 |
| IFC-SVC-165 | DataPlaneStreamService | Platform Administration | Stream | Target via FSD-006 |

## Observed legacy protobuf service catalog

All 65 entries below are observed internal `erpx.platform.v1` services from the
source snapshot. FSD-006 retires `IFC-SVC-001..065` as a set; the per-row
observed-state value records history only and is not a baseline 2.0 lifecycle.
No retired ID, service, package, or compatibility service is available through
the clean-rebuild runtime.
Each `IFC-SVC-*` record is the composition of its service row and the following
legacy-interface metadata.

| Required metadata | Resolved value for every retired `IFC-SVC-001..065` entry |
|---|---|
| Owner | The bounded context or platform authority in the service row |
| Caller/consumer | Authenticated ERPX Console BFFs, workers, owning services, data-plane agents, or named provider adapters; never an anonymous/public API caller |
| Scope | The request and identity claims carry tenant and, where applicable, holding/company/branch, customer-support, or platform scope; the owner rejects unresolved or inconsistent scope |
| Authentication/authorization | ERPX human session or allow-listed service principal plus owner-context operation, record, field, realm, and purpose authorization |
| Idempotency | Every state-changing RPC requires a caller key and payload-conflict detection; a query is non-mutating and uses stable paging/cursors where applicable |
| Version | Protobuf package `erpx.platform.v1`; incompatible changes require a new version or compatibility service |
| Classification | Field-classified from Public through Restricted; the effective request/response classification is the strictest included field and controls logging/provider use |
| Failure semantics | Canonical typed status, no authority fallback, absent/denied equivalence where enumeration is sensitive, retry only when declared, and correlation for uncertain outcomes |
| Lifecycle and source | Retired by FSD-006; deleted source paths are not treated as active evidence, and the row retains only its historical name, owner, and pre-baseline observed state |

| Catalog ID | Service | Owner | Retirement authority | Pre-baseline observed state |
|---|---|---|---|---|
| IFC-SVC-001 | OrganizationService | Organization | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-002 | OrganizationCommandService | Organization | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-003 | OrganizationQueryService | Organization | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-004 | AssetMaintenanceService | Assets/Maintenance | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-005 | PlatformOperationsService | Platform Administration | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-006 | MigrationService | Platform Administration | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-007 | PlatformAdministrationCommandService | Platform Administration | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-008 | PlatformAdministrationQueryService | Platform Administration | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-009 | LegalService | Legal | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-010 | LegalCommandService | Legal | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-011 | LegalQueryService | Legal | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-012 | DataPlaneControlService | Platform Administration | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-013 | DataPlaneAgentService | Customer data plane | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-014 | FinanceService | Finance | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-015 | FinanceCommandService | Finance | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-016 | FinanceQueryService | Finance | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-017 | PlatformCustomerService | Customer Authority | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-018 | CustomerAccessAuditQueryService | Customer Authority | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-019 | Customer360DetailQueryService | Customer 360 | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-020 | CustomerNoteCommandService | Customer 360 | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-021 | CustomerAuthorityCommandService | Customer Authority | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-022 | CustomerAuthorityQueryService | Customer Authority | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-023 | ProcurementCommandService | Procurement | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-024 | ProcurementQueryService | Procurement | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-025 | HrService | People & Payroll | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-026 | PeoplePayrollCommandService | People & Payroll | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-027 | PeoplePayrollQueryService | People & Payroll | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-028 | MaintenanceCommandService | Maintenance | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-029 | MaintenanceQueryService | Maintenance | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-030 | AssetsCommandService | Assets | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-031 | AssetsQueryService | Assets | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-032 | QualityCommandService | Quality | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-033 | QualityQueryService | Quality | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-034 | Customer360QueryService | Customer 360 | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-035 | MarketingContentService | Marketing | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-036 | MarketingCommandService | Marketing | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-037 | MarketingQueryService | Marketing | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-038 | BrandCommandService | Brand | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-039 | BrandQueryService | Brand | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-040 | SalesService | Sales | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-041 | SalesCommandService | Sales | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-042 | SalesQueryService | Sales | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-043 | DirectoryService | Identity & Access | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-044 | RoleLifecycleService | Identity & Access | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-045 | EntitlementService | Identity & Access | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-046 | IdentityAccessCommandService | Identity & Access | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-047 | IdentityAccessQueryService | Identity & Access | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-048 | CrmService | CRM | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-049 | CrmCommandService | CRM | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-050 | CrmQueryService | CRM | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-051 | SaasBillingService | Commercial | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-052 | CommercialCommandService | Commercial | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-053 | CommercialQueryService | Commercial | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-054 | SupportService | Support | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-055 | SupportCommandService | Support | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-056 | SupportQueryService | Support | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-057 | InventoryCommandService | Inventory | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-058 | InventoryQueryService | Inventory | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-059 | OperationsService | Operations | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-060 | OperationsCommandService | Operations | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-061 | OperationsQueryService | Operations | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-062 | ScmService | Supply chain compatibility | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-063 | PosService | Point of Sale | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current compatibility |
| IFC-SVC-064 | PosCommandService | Point of Sale | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |
| IFC-SVC-065 | PosQueryService | Point of Sale | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | Current |

## Target contract concepts

These types define semantic compatibility targets rather than software changes
in this baseline. Every serialized form is versioned, field-classified, rejects
unknown authority-bearing values, and preserves correlation/provenance.

| Catalog ID | Concept | Semantic owner | Required target meaning |
|---|---|---|---|
| IFC-TYP-001 | `PrincipalRef` | Identity & Access | Stable human, agent, service, controlled-human-group, or agent-pool identity plus realm and organizational scope; never a display label as authority |
| IFC-TYP-002 | `RecordRef` | Owning business context | Opaque globally unique reference ID, owner context, aggregate type, and optional anchored sub-reference; never a URL or business number as identity |
| IFC-TYP-003 | `ItemDescriptor` | Owning context; Item Directory projects | Record reference, scope, canonical route, lifecycle, policy reference, descriptor version, and owner event position without requiring protected title/body |
| IFC-TYP-004 | `ThreadAnchor` | Collaboration | Anchor record, optional record version/field/block/page/region locator, source revision, digest, data-plane route, and maximum audience |
| IFC-TYP-005 | `CommentDocument` | Collaboration | Versioned full-block content, safe attachment/embed references, typed mention/reference tokens, and accessible plain-text representation |
| IFC-TYP-006 | `CommentRevision` | Collaboration | Immutable author-attributed revision, parent version, content digest, moderation state, timestamps, and source-event position |
| IFC-TYP-007 | `MentionEvent` | Collaboration | Append-only mention target, revision, scope, recipient/pool snapshot, group version, dedupe key, delivery/assignment reference, and withdraw/cancel state |
| IFC-TYP-008 | `ReferenceRelation` | Collaboration for fact; endpoint owners for effects | Versioned typed relation and inverse, endpoint references/versions, actor, effective lifecycle, receipts, and effect-boundary state |
| IFC-TYP-009 | `ActionCard` | Command-owning business context | Typed proposed command, schema/version, target, payload digest, required authority/approvals, idempotency identity, expiry, and correlated result |
| IFC-TYP-010 | `AgentVersionRef` | Agent Orchestration | Stable agent principal plus approved immutable definition version, model/provider/tool manifest, evaluation, policy digest, and lifecycle |
| IFC-TYP-011 | `AgentAssignment` | Agent Orchestration | Root invocation, initiating principal, effective agent/version, context/scope ceiling, durable state, budget reservation, retry/cancel policy, and output provenance |
| IFC-TYP-012 | `DelegationEdge` | Agent Orchestration | Parent/child assignments, delegator/delegate, reason, scope intersection, limits, lineage, cost reservation, and cycle-check result |
| IFC-TYP-013 | `AgentSession` | Agent Orchestration | Private owner, device/channel, context manifest, assignment links, configurable TTL, provider provenance, and explicit publication receipts |
| IFC-TYP-014 | `BudgetDecision` | Agent Orchestration | Tenant/workspace/agent/principal/root layers, estimate, reservation, actual usage, concurrency, priority, stop condition, approver, reason, and expiry |

## Route families

| Catalog ID | Route family | Owner/caller | Authorization and lifecycle |
|---|---|---|---|
| IFC-UI-001 | `/work`, `/processes`, `/insights`, `/modules` | Enterprise Home | Shared entry points; active realm, role, and organization scope |
| IFC-UI-002 | `/<module>` | Typed workspace catalog | Exactly 21 authorized module roots; Commerce and Talent Web-only |
| IFC-UI-003 | `/<module>/<record>` | Owning context | Singular collection route assembled from approved catalog slugs |
| IFC-UI-004 | `/<module>/<record>/:id` | Owning context | Singular detail route; lowercase-hyphenated UUID v4 identity |
| IFC-UI-005 | `/reference/item/:id` | Reference Query adapter | Stable copied-link resolver; authorize before `303` redirect and never leak denied/absent distinction |
| IFC-HTTP-001 | `GET /api/v1/<module>/<record>` | Query adapter | Authorized paged projection query with freshness and no hidden counts |
| IFC-HTTP-002 | `POST /api/v1/<module>/<record>` | Command adapter | Typed create command with idempotency and owner authorization |
| IFC-HTTP-003 | `GET /api/v1/<module>/<record>/:id` | Query adapter | Authorized projected detail with source position and degraded state |
| IFC-HTTP-004 | `POST /api/v1/<module>/<record>/:id/commands/<command>` | Command adapter | Explicit named command; expected version, idempotency, audit, and typed receipt |
| IFC-HTTP-005 | `GET /api/v1/collaboration/thread/:id/stream` | Collaboration stream adapter | Authorized resumable SSE with cursor reauthorization |

## Event, provider, and data-plane families

| Catalog ID | Family | Owner | Required semantics |
|---|---|---|---|
| IFC-EVT-001 | `<context>.<record>.<event>.v1` | Every aggregate owner | Typed-registry name; minimized versioned payload, owner key, event position, classification, replay, and deletion semantics |
| IFC-EVT-002 | `collaboration.thread.changed.v1` / `collaboration.comment.changed.v1` | Collaboration | Anchor-plane content reference, audience digest, revision, no replicated body |
| IFC-EVT-003 | `collaboration.mention.created.v1` / `collaboration.mention.withdrawn.v1` | Collaboration | Recipient/pool snapshot, dedupe key, source revision, opaque preview |
| IFC-EVT-004 | `collaboration.reference-relation.changed.v1` | Collaboration | Typed relation, inverse, source versions, authorization-safe projection |
| IFC-EVT-005 | `agent.assignment.state-changed.v1` | Agent Orchestration | Root/delegation, pinned version, state, usage, redacted outcome |
| IFC-PRV-001 | Stripe/payment adapters | Finance/Commercial/POS | Signed callbacks, idempotency, reconciliation, no instrument storage |
| IFC-PRV-002 | LDAP, AD DS, UCS directory adapters | IAM | Immutable provider IDs, cursor continuity, TLS trust, fail-closed revocation |
| IFC-PRV-003 | Ollama/local and approved external AI providers | Platform Administration/Agent Orchestration | Model registry, classification, residency, DPA, payload provenance |
| IFC-PRV-004 | Email, SMS, Push, Webhook adapters | Notifications | Optional delivery, retry, dedupe, callback reconciliation |
| IFC-PRV-005 | E-signature providers | E-signature/source owner | Envelope digest, signer order, callback verification, evidence export |
| IFC-PRV-006 | Reserved (retired workflow adapter) | Platform Administration | Wire/catalog identifier is not reusable |
| IFC-PRV-007 | Object-storage providers | Source contexts | Tenant-scoped keys, digest, encryption, quarantine, short-lived authorization |
| IFC-PRV-008 | Thailand tax/regulatory adapters | Finance/People & Payroll | Versioned pack, official source, external certification, reconciliation |
| IFC-DP-001 | Provision, migrate, suspend, resume, rotate, backup, restore, PITR, exit | Platform/Data Plane agents | Receipt-bound, resumable, scoped, audited, rollback-safe operations |
