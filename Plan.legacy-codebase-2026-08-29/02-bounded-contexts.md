# 02 — Bounded Contexts (Codebase-Derived)

**Source:** `crates/contexts/*/src/{domain,application,ports}.rs`, `crates/domain/src/*.rs`, `crates/proto/proto/erpx/*/service.proto`.

Each context owns its aggregates, invariants, and ports; adapters implement persistence. Domain has no transport deps.

| Context | Proto services | Core aggregates (sample) |
|---|---|---|
| agent-orchestration | — | AgentDomainError, PrincipalKind, AgentDefinitionStatus, AgentVersionStatus, AgentManifest, AgentVersion |
| assets | AssetsCommandService, AssetsQueryService | — |
| benefits | BenefitsCommandService, BenefitsQueryService | PlanStatus, ContributionRule, BenefitPlan, BenefitPlanRef, EnrollmentWindow, EligibilityDecision |
| brand | BrandCommandService, BrandQueryService | — |
| collaboration | CollaborationCommandService, CollaborationQueryService, CollaborationStreamService | CollaborationError, PrincipalRef, ExternalPortal, Audience, AnchorPolicy, AnchorLocationKind |
| commercial | CommercialCommandService, CommercialQueryService | — |
| crm | CrmCommandService, CrmQueryService | — |
| customer-360 | — | — |
| customer-authority | — | — |
| ecommerce | EcommerceCommandService, EcommerceQueryService | CommerceRealm, CommerceHumanDecisionEvidence, CommerceIdentity, ChannelStatus, CommerceChannel, ListingStatus |
| field-service | — | FieldServiceHumanDecisionEvidence, Connectivity, ServiceAgreement, ServiceRequest, TechnicianQualification, PartReservation |
| finance | FinancePayablesCommandService, FinancePayablesQueryService, FinanceCommandService, FinanceQueryService | — |
| identity-access | — | — |
| inventory | InventoryCommandService, InventoryQueryService | — |
| learning | LearningCommandService, LearningQueryService | CourseStatus, LearningHumanDecisionEvidence, Course, CourseRef, EnrollmentStatus, AttendanceFact |
| legal | LegalCommandService, LegalQueryService | — |
| maintenance | MaintenanceCommandService, MaintenanceQueryService | — |
| manufacturing | ManufacturingCommandService, ManufacturingQueryService | ManufacturingHumanDecisionEvidence, ManufacturingMode, MaterialRole, MaterialLine, RoutingStep, SpecificationStatus |
| marketing | MarketingCommandService, MarketingQueryService | — |
| operations | OperationsCommandService, OperationsQueryService | — |
| organization | OrganizationCommandService, OrganizationQueryService | — |
| people-payroll | — | — |
| planning | PlanningCommandService, PlanningQueryService | PlanningMode, DemandFact, SupplyFact, CapacityBucket, CapacityRequirement, RecommendationKind |
| platform-administration | — | — |
| pos | PointOfSaleCommandService, PointOfSaleQueryService | — |
| procurement | ProcurementCommandService, ProcurementQueryService | — |
| projects | ProjectsCommandService, ProjectsQueryService | — |
| quality | QualityCommandService, QualityQueryService | — |
| sales | SalesCommandService, SalesQueryService | — |
| support | SupportCommandService, SupportQueryService | — |
| talent | TalentCommandService, TalentQueryService | TalentRealm, TalentHumanDecisionEvidence, RealmPrincipal, RequisitionStatus, Requisition, CandidateStatus |

Per-context detail: `Plan/contexts/<context>.md`. Traceability: `Plan/traceability.md`.
