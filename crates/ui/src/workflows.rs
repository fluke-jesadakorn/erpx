//! Human-workflow routing and transport classification.
//!
//! This is the client-side half of ERPX's route/RPC parity contract.  The
//! server still authorizes every request; these declarations only make human
//! capabilities discoverable and prevent a new RPC from silently becoming a
//! JSON-only or route-less console feature.

/// The audience and ingress boundary for an RPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcClassification {
    /// An authenticated person invokes the RPC from an ERPX application.
    HumanInteractive,
    /// An anonymous person invokes a deliberately public, abuse-controlled RPC.
    PublicHuman,
    /// An ERPX data-plane agent invokes the RPC without a human UI.
    MachineOnly,
    /// An external provider invokes the RPC or equivalent ingress boundary.
    ProviderIngress,
    /// An ERPX worker invokes the RPC or equivalent internal boundary.
    WorkerOnly,
}

impl RpcClassification {
    /// Whether route/action/state parity is required for this classification.
    pub const fn requires_human_surface(self) -> bool {
        matches!(self, Self::HumanInteractive | Self::PublicHuman)
    }
}

/// The reviewed client adapter used to construct or consume the wire message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanAdapterKind {
    /// A bounded typed read DTO is rendered through a domain view model.
    TypedRead,
    /// A bounded typed command form creates the protobuf command directly.
    TypedCommand,
    /// A typed form creates the canonical payload for a legacy generic aggregate RPC.
    CanonicalPayload,
}

/// Every human route must be able to represent these states without rendering
/// transport errors or raw payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanUiState {
    Loading,
    Empty,
    Denied,
    Offline,
    Stale,
    Conflict,
    Submitting,
    Success,
    RetryableError,
    Unavailable,
}

/// Canonical state contract shared by every human workflow.
pub const HUMAN_UI_STATES: &[HumanUiState] = &[
    HumanUiState::Loading,
    HumanUiState::Empty,
    HumanUiState::Denied,
    HumanUiState::Offline,
    HumanUiState::Stale,
    HumanUiState::Conflict,
    HumanUiState::Submitting,
    HumanUiState::Success,
    HumanUiState::RetryableError,
    HumanUiState::Unavailable,
];

/// One explicit service entry in the RPC classification registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RpcServiceWorkflow {
    pub service: &'static str,
    pub methods: &'static [&'static str],
    pub classification: RpcClassification,
    pub routes: &'static [&'static str],
}

/// The resolved parity contract for one RPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RpcWorkflow<'a> {
    pub service: &'a str,
    pub method: &'a str,
    pub classification: RpcClassification,
    pub routes: &'static [&'static str],
    /// Stable server-owned capability key. The RPC method is deliberately the
    /// source of truth so capability drift is visible in descriptor review.
    pub action_capability: Option<&'a str>,
    pub adapter: Option<HumanAdapterKind>,
    pub states: &'static [HumanUiState],
}

const PLATFORM_CUSTOMER_METHODS: &[&str] = &[
    "ListCustomers",
    "GetCustomer",
    "GetCustomerProjection",
    "GetCustomer360Detail",
    "GetCustomerAccessAudit",
    "AddCustomerNote",
];
const PLATFORM_OPERATIONS_METHODS: &[&str] = &[
    "ListTenants",
    "GetTenant",
    "BeginTenantAdminContext",
    "GetTenantAdminContext",
    "EndTenantAdminContext",
    "DiscoverTenantOrganizationUnits",
    "BeginTenantProvisioning",
    "ProvisionTenantResources",
    "ActivateTenant",
    "SuspendTenant",
    "EnterTenantRecovery",
    "ScheduleTenantBackup",
    "ListTenantBackupJobs",
    "ListTenantControlHistory",
    "ScheduleClusterBaseBackup",
    "ListClusterRecoveryPoints",
    "StartClusterRestoreRehearsal",
    "GetClusterRestoreRehearsal",
    "StartProjectionRebuild",
    "ActivateProjectionRebuild",
    "ListProjectionStatuses",
    "ListPlatformAudit",
    "ReplayDeadLetterOutbox",
    "ListPlatformIntegrations",
    "GetPlatformSettings",
];
const BILLING_METHODS: &[&str] = &[
    "ListCatalog",
    "GetSubscription",
    "CreateCheckoutSession",
    "CreateCustomerPortalSession",
    "ListInvoices",
    "GrantGraceExtension",
];
const MIGRATION_METHODS: &[&str] = &[
    "ListMigrationStatuses",
    "QueueMigrationWork",
    "BeginMigrationRun",
    "AdvanceMigrationPhase",
];
const DIRECTORY_METHODS: &[&str] = &[
    "ListDirectoryConnectors",
    "SaveDirectoryConnector",
    "StartDirectoryImport",
    "GetDirectoryImportStatus",
    "ListDirectoryRoleMappings",
    "ListDirectoryIdentityLinks",
    "SaveDirectoryRoleMapping",
    "PreflightDirectoryConnector",
    "TransitionDirectoryConnectorLifecycle",
    "LinkDirectoryPrincipal",
    "UnlinkDirectoryPrincipal",
    "GetDirectoryConnectorReadiness",
    "ReconcileDirectoryConnector",
    "ListDirectoryConflicts",
    "ProposeDirectoryIdentityReplacement",
    "ApproveDirectoryIdentityReplacement",
    "RejectDirectoryIdentityReplacement",
];
const ORGANIZATION_METHODS: &[&str] = &[
    "SaveOrganizationNode",
    "ListOrganizationNodes",
    "ProvisionBranch",
    "GetBranchProvisioning",
    "ListBranchProvisionings",
    "RetryBranchProvisioning",
    "SuspendBranchRoute",
];
const OPERATIONS_METHODS: &[&str] = &[
    "GetOperationsSummary",
    "ListOperationsExceptions",
    "CreateOperationsExceptionFromSource",
    "TransitionOperationsException",
    "PublishOperationsWorkQueuePolicy",
    "ListOperationsWorkQueuePolicies",
];
const OPERATIONS_COMMAND_METHODS: &[&str] = &[
    "ReassignException",
    "AcknowledgeException",
    "EscalateException",
    "ResolveException",
    "CloseException",
    "PublishWorkQueuePolicy",
];
const OPERATIONS_QUERY_METHODS: &[&str] = &[
    "GetException",
    "ListExceptionQueue",
    "ListWorkQueuePolicies",
    "GetSummary",
];
const CUSTOMER_360_QUERY_METHODS: &[&str] = &["GetCustomer360"];
const CUSTOMER_ACCESS_AUDIT_QUERY_METHODS: &[&str] = &["GetCustomerAccessAudit"];
const SUPPORT_METHODS: &[&str] = &[
    "CreateTicket",
    "UpdateTicket",
    "GetTicket",
    "ListTickets",
    "CreateTicketRecurrence",
    "AddTicketReply",
    "AddTicketInternalNote",
    "CreateIncident",
    "UpdateIncident",
    "GetIncident",
    "ListIncidents",
    "RequestSupportSession",
    "ApproveSupportSession",
    "RevokeSupportSession",
    "GetSupportDiagnostics",
];
const ROLE_METHODS: &[&str] = &[
    "ListExpiringRoles",
    "RenewRole",
    "ListRoleReview",
    "RecoverFinalRole",
];
const ENTITLEMENT_METHODS: &[&str] = &[
    "CheckEntitlement",
    "AllocateCompanyEntitlement",
    "ActivateCompanyEntitlement",
    "ActivateBranchEntitlement",
];
const FINANCE_METHODS: &[&str] = &[
    "ListPostedJournals",
    "GetFinanceTrialBalance",
    "GetFinanceConsolidatedTrialBalance",
    "SaveFinancePeriod",
    "PostFinanceJournal",
    "RecordThaiTaxFact",
    "GetThaiTaxReport",
    "SaveSubledger",
    "SaveAllocationBatch",
    "SaveFinanceChart",
    "ListFinanceCharts",
    "ListFinanceSubledgerReadRows",
    "RegisterBankAccount",
    "ImportBankStatement",
    "EnqueueFinanceBankProviderImport",
    "GetFinanceBankProviderImportJobStatus",
    "SaveBankReconciliation",
    "SaveBankMatchProposal",
    "SaveBankMatchDecision",
    "SaveFinanceCloseReview",
    "GetFinanceCloseReview",
    "ListFinanceCloseReviews",
    "GetFinanceBankingReadRow",
    "ListFinanceBankingReadRows",
];
const HR_METHODS: &[&str] = &[
    "SaveHrEmployee",
    "TransitionHrEmployee",
    "SaveHrLeave",
    "SaveHrExpense",
    "SaveHrPayroll",
    "ListHrEmployees",
    "ListHrLeaveRequests",
    "ListHrExpenseClaims",
    "ListHrPayrollRuns",
    "ListHrReassignments",
];
const POS_METHODS: &[&str] = &["SavePosSale", "ListPosSales"];
const LEGAL_METHODS: &[&str] = &[
    "SaveLegalContract",
    "SaveLegalDocument",
    "RecordLegalAccess",
    "ListLegalContracts",
    "ListLegalDocuments",
    "ListLegalAccessHistory",
];
const CRM_METHODS: &[&str] = &[
    "ListCrmReadRows",
    "SaveCrmHandoff",
    "SaveCrmOpportunity",
    "RecordCrmAttribution",
];
const MARKETING_METHODS: &[&str] = &[
    "SubmitMarketingForm",
    "ListMarketingContentReadRows",
    "GetMarketingDeliveryRead",
    "EraseMarketingProfile",
    "IssueMarketingPreview",
    "RevokeMarketingPreview",
    "IssueMarketingDomainVerification",
    "VerifyMarketingDomain",
    "BindMarketingDomain",
    "ConfigureMarketingProviderAccount",
    "AcknowledgeMarketingProviderVerification",
    "SetMarketingProviderAccountState",
];
const SALES_METHODS: &[&str] = &[
    "SaveSalesAggregate",
    "TransitionSalesAggregate",
    "GetSalesAggregate",
    "ListSalesAggregates",
];
const SALES_CQRS_COMMAND_METHODS: &[&str] = &[
    "CreateQuote",
    "SendQuote",
    "AcceptQuote",
    "RejectQuote",
    "ExpireQuote",
    "CreateSalesOrder",
    "ConfirmSalesOrder",
    "MarkSalesOrderPartiallyFulfilled",
    "MarkSalesOrderFulfilled",
    "CancelSalesOrder",
];
const SALES_CQRS_QUERY_METHODS: &[&str] = &["GetQuote", "GetSalesOrder", "ListSalesOrders"];
const PEOPLE_PAYROLL_COMMAND_METHODS: &[&str] = &[
    "HireEmployee",
    "TransitionEmployee",
    "CreateLeaveRequest",
    "TransitionLeaveRequest",
    "CreateExpenseClaim",
    "TransitionExpenseClaim",
    "CreatePayrollRun",
    "TransitionPayrollRun",
];
const PEOPLE_PAYROLL_QUERY_METHODS: &[&str] = &[
    "GetEmployee",
    "ListEmployees",
    "GetLeaveRequest",
    "ListLeaveRequests",
    "GetExpenseClaim",
    "ListExpenseClaims",
    "GetPayrollRun",
    "ListPayrollRuns",
];
const POS_CQRS_COMMAND_METHODS: &[&str] = &[
    "CreateSale",
    "OpenSale",
    "HoldSale",
    "ResumeSale",
    "RecordPayment",
    "CompleteSale",
    "VoidSale",
    "RequestRefund",
    "ApproveRefund",
    "RejectRefund",
    "PostRefund",
    "CancelRefund",
];
const POS_CQRS_QUERY_METHODS: &[&str] = &["GetSale", "ListSales", "GetRefund", "ListRefunds"];
const FINANCE_CQRS_COMMAND_METHODS: &[&str] = &[
    "CreateChart",
    "AddAccount",
    "ChangeAccountStatus",
    "CreatePeriod",
    "BeginPeriodClose",
    "ClosePeriod",
    "ReopenPeriod",
    "CreateJournal",
    "PostJournal",
    "ApplySubledgerSettlement",
];
const FINANCE_CQRS_QUERY_METHODS: &[&str] = &[
    "GetChart",
    "GetPeriod",
    "GetJournal",
    "ListJournals",
    "GetSubledger",
    "ListSubledger",
    "GetTrialBalance",
    "ListBankTransactions",
    "GetStatutoryTaxReport",
];
const LEGAL_CQRS_COMMAND_METHODS: &[&str] = &[
    "CreateContract",
    "AddContractParty",
    "TransitionContract",
    "CreateDocument",
    "AddDocumentVersion",
    "ArchiveDocument",
    "RecordDocumentAccess",
];
const LEGAL_CQRS_QUERY_METHODS: &[&str] = &[
    "GetContract",
    "ListContracts",
    "GetDocument",
    "ListDocuments",
    "ListAccessHistory",
];
const SUPPORT_CQRS_COMMAND_METHODS: &[&str] = &[
    "OpenTicket",
    "TransitionTicket",
    "AddCustomerReply",
    "AddInternalNote",
    "RequestSession",
    "ApproveSession",
    "RevokeSession",
    "IssueCapability",
    "RevokeCapability",
    "OpenIncident",
    "TransitionIncident",
    "AddIncidentTimeline",
    "BroadcastIncident",
];
const SUPPORT_CQRS_QUERY_METHODS: &[&str] = &[
    "GetCustomerTicket",
    "ListCustomerTickets",
    "GetInternalTicket",
    "ListInternalTickets",
    "GetIncident",
    "ListIncidents",
    "GetSession",
    "GetCapability",
];
const SCM_METHODS: &[&str] = &[
    "SaveScmAggregate",
    "TransitionScmAggregate",
    "GetScmAggregate",
    "ListScmAggregates",
];
const ASSET_MAINTENANCE_METHODS: &[&str] = &[
    "SaveAssetMaintenanceAggregate",
    "GetAssetMaintenanceAggregate",
    "ListAssetMaintenanceAggregates",
];
const MARKETING_COMMAND_METHODS: &[&str] = &[
    "CreateLeadCandidate",
    "CaptureEngagement",
    "QualifyLeadCandidate",
    "RejectLeadCandidate",
];
const MARKETING_QUERY_METHODS: &[&str] = &["GetLeadCandidate", "ListLeadCandidates"];
const CRM_COMMAND_METHODS: &[&str] = &[
    "AcceptMarketingLead",
    "CreateOpportunity",
    "TransitionOpportunity",
];
const CRM_QUERY_METHODS: &[&str] = &["GetCrmLead", "GetOpportunity", "ListOpportunities"];
const PROCUREMENT_COMMAND_METHODS: &[&str] = &["Create", "Release", "Cancel"];
const PROCUREMENT_QUERY_METHODS: &[&str] = &["Get", "List"];
const INVENTORY_COMMAND_METHODS: &[&str] = &[
    "Register",
    "Receive",
    "Issue",
    "Reserve",
    "ReleaseReservation",
];
const INVENTORY_QUERY_METHODS: &[&str] = &["Get", "List"];
const ASSETS_COMMAND_METHODS: &[&str] = &[
    "Create",
    "PlaceInService",
    "MarkDegraded",
    "BeginMaintenance",
    "ReturnToService",
    "Retire",
];
const ASSETS_QUERY_METHODS: &[&str] = &["Get", "List"];
const MAINTENANCE_COMMAND_METHODS: &[&str] = &[
    "Open",
    "Schedule",
    "Start",
    "Block",
    "Resume",
    "Complete",
    "Cancel",
    "RecordDowntime",
];
const MAINTENANCE_QUERY_METHODS: &[&str] = &["Get", "List"];
const QUALITY_COMMAND_METHODS: &[&str] = &["Create", "Start", "RecordResult", "Release", "Cancel"];
const QUALITY_QUERY_METHODS: &[&str] = &["Get", "List"];
const IDENTITY_ACCESS_COMMAND_METHODS: &[&str] = &[
    "CreatePrincipal",
    "ActivatePrincipal",
    "SuspendPrincipal",
    "ReactivatePrincipal",
    "DisablePrincipal",
    "AssignRole",
    "RevokeRole",
    "DefineRole",
    "SetRolePermissions",
];
const IDENTITY_ACCESS_QUERY_METHODS: &[&str] =
    &["GetPrincipal", "ListPrincipals", "GetRole", "ListRoles"];
const ORGANIZATION_COMMAND_METHODS: &[&str] = &[
    "CreateNode",
    "RenameNode",
    "ChangeNodeCode",
    "ReparentNode",
    "SuspendNode",
    "ActivateNode",
    "ArchiveNode",
];
const ORGANIZATION_QUERY_METHODS: &[&str] = &["GetNode", "ListNodes"];
const COMMERCIAL_HUMAN_COMMAND_METHODS: &[&str] = &[
    "CreateCatalogPlan",
    "PublishCatalogPlan",
    "CreateSubscription",
    "CreateEntitlement",
];
const COMMERCIAL_PROVIDER_COMMAND_METHODS: &[&str] =
    &["ApplyBillingEvent", "ApplyEntitlementChange"];
const COMMERCIAL_QUERY_METHODS: &[&str] = &[
    "GetCatalogPlan",
    "ListCatalogPlans",
    "GetSubscription",
    "ListSubscriptions",
    "GetEntitlement",
    "ListEntitlements",
];
const PLATFORM_ADMINISTRATION_COMMAND_METHODS: &[&str] = &[
    "RegisterTenant",
    "TransitionTenant",
    "RegisterConnector",
    "TransitionConnector",
    "IssueAdminContext",
    "RevokeAdminContext",
];
const PLATFORM_ADMINISTRATION_QUERY_METHODS: &[&str] = &[
    "GetTenant",
    "ListTenants",
    "GetConnector",
    "ListConnectors",
    "GetAdminContext",
];
const BRAND_COMMAND_METHODS: &[&str] = &[
    "CreateDraft",
    "SaveDraft",
    "PublishDraft",
    "Rollback",
    "SetCategoryLocks",
];
const BRAND_QUERY_METHODS: &[&str] = &["GetDraft", "GetResolved", "ListHistory"];
const CUSTOMER_AUTHORITY_COMMAND_METHODS: &[&str] = &[
    "Create",
    "UpdateIdentity",
    "ChangeStatus",
    "LinkAuthority",
    "UnlinkAuthority",
    "Merge",
];
const CUSTOMER_AUTHORITY_QUERY_METHODS: &[&str] = &["Get", "List"];
const DATA_PLANE_CONTROL_METHODS: &[&str] = &[
    "GetTenantInfrastructure",
    "SaveTenantInfrastructureProfile",
    "GetBusinessUnitConfiguration",
    "SaveBusinessUnitConfiguration",
    "IssueDataPlaneEnrollment",
    "ProposeDesiredConfiguration",
    "DecideDesiredConfiguration",
    "RequestRootAccess",
    "DecideRootAccess",
    "RevokeRootAccess",
    "ListRootAccessRequests",
    "GetResolvedCorporateIdentity",
    "GetCorporateIdentityDraft",
    "SaveCorporateIdentityDraft",
    "PublishCorporateIdentityDraft",
    "RollbackCorporateIdentity",
    "ListCorporateIdentityHistory",
    "GetNavigationManifest",
];
const DATA_PLANE_AGENT_METHODS: &[&str] = &["Enroll", "ReportObservation", "Connect"];

const CUSTOMER_ROUTES: &[&str] = &["/platform/customers"];
const PLATFORM_ROUTES: &[&str] = &[
    "/platform/tenants",
    "/platform/projections",
    "/platform/backups",
    "/platform/integrations",
    "/platform/audit",
    "/platform/settings",
];
const BILLING_ROUTES: &[&str] = &["/billing"];
const MIGRATION_ROUTES: &[&str] = &["/platform/migrations"];
const DIRECTORY_ROUTES: &[&str] = &["/platform/directories"];
const ORGANIZATION_ROUTES: &[&str] = &["/tenant/organization", "/tenant/branches"];
const OPERATIONS_ROUTES: &[&str] = &["/workspaces/operations"];
const SUPPORT_ROUTES: &[&str] = &["/support"];
const ROLE_ROUTES: &[&str] = &["/platform/role-review"];
const ENTITLEMENT_ROUTES: &[&str] = &["/billing/entitlements"];
const FINANCE_ROUTES: &[&str] = &["/workspaces/finance"];
const HR_ROUTES: &[&str] = &["/workspaces/hr-payroll"];
const POS_ROUTES: &[&str] = &["/workspaces/point-of-sale"];
const LEGAL_ROUTES: &[&str] = &["/workspaces/legal"];
const CRM_ROUTES: &[&str] = &["/crm/opportunities"];
const MARKETING_ROUTES: &[&str] = &["/marketing"];
const PUBLIC_MARKETING_ROUTES: &[&str] = &["/public/marketing/forms"];
const SALES_ROUTES: &[&str] = &["/workspaces/sales"];
const SCM_ROUTES: &[&str] = &["/workspaces/procurement", "/workspaces/inventory"];
const ASSET_ROUTES: &[&str] = &["/workspaces/assets", "/workspaces/maintenance"];
const QUALITY_ROUTES: &[&str] = &["/workspaces/quality"];
const BRAND_ROUTES: &[&str] = &["/tenant/corporate-identity"];
const DATA_PLANE_ROUTES: &[&str] = &[
    "/tenant/infrastructure",
    "/tenant/configuration",
    "/tenant/root-access",
    "/tenant/corporate-identity",
];
const NO_ROUTES: &[&str] = &[];

/// Explicit registry covering every RPC in the ERPX platform descriptor.
pub const RPC_WORKFLOW_SERVICES: &[RpcServiceWorkflow] = &[
    RpcServiceWorkflow {
        service: "PlatformCustomerService",
        methods: PLATFORM_CUSTOMER_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: CUSTOMER_ROUTES,
    },
    RpcServiceWorkflow {
        service: "PlatformOperationsService",
        methods: PLATFORM_OPERATIONS_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: PLATFORM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "SaasBillingService",
        methods: BILLING_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: BILLING_ROUTES,
    },
    RpcServiceWorkflow {
        service: "MigrationService",
        methods: MIGRATION_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: MIGRATION_ROUTES,
    },
    RpcServiceWorkflow {
        service: "DirectoryService",
        methods: DIRECTORY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: DIRECTORY_ROUTES,
    },
    RpcServiceWorkflow {
        service: "OrganizationService",
        methods: ORGANIZATION_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ORGANIZATION_ROUTES,
    },
    RpcServiceWorkflow {
        service: "OperationsService",
        methods: OPERATIONS_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: OPERATIONS_ROUTES,
    },
    RpcServiceWorkflow {
        service: "OperationsCommandService",
        methods: OPERATIONS_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: OPERATIONS_ROUTES,
    },
    RpcServiceWorkflow {
        service: "OperationsQueryService",
        methods: OPERATIONS_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: OPERATIONS_ROUTES,
    },
    RpcServiceWorkflow {
        service: "Customer360QueryService",
        methods: CUSTOMER_360_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: CUSTOMER_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CustomerAccessAuditQueryService",
        methods: CUSTOMER_ACCESS_AUDIT_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: CUSTOMER_ROUTES,
    },
    RpcServiceWorkflow {
        service: "SupportService",
        methods: SUPPORT_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SUPPORT_ROUTES,
    },
    RpcServiceWorkflow {
        service: "SupportCommandService",
        methods: SUPPORT_CQRS_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SUPPORT_ROUTES,
    },
    RpcServiceWorkflow {
        service: "SupportQueryService",
        methods: SUPPORT_CQRS_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SUPPORT_ROUTES,
    },
    RpcServiceWorkflow {
        service: "RoleLifecycleService",
        methods: ROLE_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ROLE_ROUTES,
    },
    RpcServiceWorkflow {
        service: "EntitlementService",
        methods: ENTITLEMENT_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ENTITLEMENT_ROUTES,
    },
    RpcServiceWorkflow {
        service: "FinanceService",
        methods: FINANCE_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: FINANCE_ROUTES,
    },
    RpcServiceWorkflow {
        service: "FinanceCommandService",
        methods: FINANCE_CQRS_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: FINANCE_ROUTES,
    },
    RpcServiceWorkflow {
        service: "FinanceQueryService",
        methods: FINANCE_CQRS_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: FINANCE_ROUTES,
    },
    RpcServiceWorkflow {
        service: "HrService",
        methods: HR_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: HR_ROUTES,
    },
    RpcServiceWorkflow {
        service: "PosService",
        methods: POS_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: POS_ROUTES,
    },
    RpcServiceWorkflow {
        service: "LegalService",
        methods: LEGAL_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: LEGAL_ROUTES,
    },
    RpcServiceWorkflow {
        service: "LegalCommandService",
        methods: LEGAL_CQRS_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: LEGAL_ROUTES,
    },
    RpcServiceWorkflow {
        service: "LegalQueryService",
        methods: LEGAL_CQRS_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: LEGAL_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CrmService",
        methods: CRM_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: CRM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "MarketingContentService",
        methods: MARKETING_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: MARKETING_ROUTES,
    },
    RpcServiceWorkflow {
        service: "SalesService",
        methods: SALES_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SALES_ROUTES,
    },
    RpcServiceWorkflow {
        service: "SalesCommandService",
        methods: SALES_CQRS_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SALES_ROUTES,
    },
    RpcServiceWorkflow {
        service: "SalesQueryService",
        methods: SALES_CQRS_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SALES_ROUTES,
    },
    RpcServiceWorkflow {
        service: "PeoplePayrollCommandService",
        methods: PEOPLE_PAYROLL_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: HR_ROUTES,
    },
    RpcServiceWorkflow {
        service: "PeoplePayrollQueryService",
        methods: PEOPLE_PAYROLL_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: HR_ROUTES,
    },
    RpcServiceWorkflow {
        service: "PosCommandService",
        methods: POS_CQRS_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: POS_ROUTES,
    },
    RpcServiceWorkflow {
        service: "PosQueryService",
        methods: POS_CQRS_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: POS_ROUTES,
    },
    RpcServiceWorkflow {
        service: "ScmService",
        methods: SCM_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SCM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "AssetMaintenanceService",
        methods: ASSET_MAINTENANCE_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ASSET_ROUTES,
    },
    RpcServiceWorkflow {
        service: "MarketingCommandService",
        methods: MARKETING_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: MARKETING_ROUTES,
    },
    RpcServiceWorkflow {
        service: "MarketingQueryService",
        methods: MARKETING_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: MARKETING_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CrmCommandService",
        methods: CRM_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: CRM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CrmQueryService",
        methods: CRM_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: CRM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "ProcurementCommandService",
        methods: PROCUREMENT_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SCM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "ProcurementQueryService",
        methods: PROCUREMENT_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SCM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "InventoryCommandService",
        methods: INVENTORY_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SCM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "InventoryQueryService",
        methods: INVENTORY_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: SCM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "AssetsCommandService",
        methods: ASSETS_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ASSET_ROUTES,
    },
    RpcServiceWorkflow {
        service: "AssetsQueryService",
        methods: ASSETS_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ASSET_ROUTES,
    },
    RpcServiceWorkflow {
        service: "MaintenanceCommandService",
        methods: MAINTENANCE_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ASSET_ROUTES,
    },
    RpcServiceWorkflow {
        service: "MaintenanceQueryService",
        methods: MAINTENANCE_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ASSET_ROUTES,
    },
    RpcServiceWorkflow {
        service: "QualityCommandService",
        methods: QUALITY_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: QUALITY_ROUTES,
    },
    RpcServiceWorkflow {
        service: "QualityQueryService",
        methods: QUALITY_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: QUALITY_ROUTES,
    },
    RpcServiceWorkflow {
        service: "IdentityAccessCommandService",
        methods: IDENTITY_ACCESS_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ROLE_ROUTES,
    },
    RpcServiceWorkflow {
        service: "IdentityAccessQueryService",
        methods: IDENTITY_ACCESS_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ROLE_ROUTES,
    },
    RpcServiceWorkflow {
        service: "OrganizationCommandService",
        methods: ORGANIZATION_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ORGANIZATION_ROUTES,
    },
    RpcServiceWorkflow {
        service: "OrganizationQueryService",
        methods: ORGANIZATION_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: ORGANIZATION_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CommercialCommandService",
        methods: COMMERCIAL_HUMAN_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: BILLING_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CommercialCommandService",
        methods: COMMERCIAL_PROVIDER_COMMAND_METHODS,
        classification: RpcClassification::ProviderIngress,
        routes: NO_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CommercialQueryService",
        methods: COMMERCIAL_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: BILLING_ROUTES,
    },
    RpcServiceWorkflow {
        service: "PlatformAdministrationCommandService",
        methods: PLATFORM_ADMINISTRATION_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: PLATFORM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "PlatformAdministrationQueryService",
        methods: PLATFORM_ADMINISTRATION_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: PLATFORM_ROUTES,
    },
    RpcServiceWorkflow {
        service: "BrandCommandService",
        methods: BRAND_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: BRAND_ROUTES,
    },
    RpcServiceWorkflow {
        service: "BrandQueryService",
        methods: BRAND_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: BRAND_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CustomerAuthorityCommandService",
        methods: CUSTOMER_AUTHORITY_COMMAND_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: CUSTOMER_ROUTES,
    },
    RpcServiceWorkflow {
        service: "CustomerAuthorityQueryService",
        methods: CUSTOMER_AUTHORITY_QUERY_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: CUSTOMER_ROUTES,
    },
    RpcServiceWorkflow {
        service: "DataPlaneControlService",
        methods: DATA_PLANE_CONTROL_METHODS,
        classification: RpcClassification::HumanInteractive,
        routes: DATA_PLANE_ROUTES,
    },
    RpcServiceWorkflow {
        service: "DataPlaneAgentService",
        methods: DATA_PLANE_AGENT_METHODS,
        classification: RpcClassification::MachineOnly,
        routes: NO_ROUTES,
    },
];

/// Resolve one descriptor RPC through the explicit registry.
pub fn rpc_workflow<'a>(service: &'a str, method: &'a str) -> Option<RpcWorkflow<'a>> {
    let registered = RPC_WORKFLOW_SERVICES
        .iter()
        .find(|candidate| candidate.service == service && candidate.methods.contains(&method))?;
    let classification = if service == "MarketingContentService" && method == "SubmitMarketingForm"
    {
        RpcClassification::PublicHuman
    } else {
        registered.classification
    };
    let routes = if classification == RpcClassification::PublicHuman {
        PUBLIC_MARKETING_ROUTES
    } else {
        registered.routes
    };
    let adapter = classification
        .requires_human_surface()
        .then(|| adapter_for(method));
    Some(RpcWorkflow {
        service,
        method,
        classification,
        routes,
        action_capability: classification.requires_human_surface().then_some(method),
        adapter,
        states: if classification.requires_human_surface() {
            HUMAN_UI_STATES
        } else {
            &[]
        },
    })
}

fn adapter_for(method: &str) -> HumanAdapterKind {
    if matches!(
        method,
        "SaveSalesAggregate"
            | "TransitionSalesAggregate"
            | "SaveScmAggregate"
            | "TransitionScmAggregate"
            | "SaveAssetMaintenanceAggregate"
    ) {
        HumanAdapterKind::CanonicalPayload
    } else if method.starts_with("Get")
        || method.starts_with("List")
        || method.starts_with("Check")
        || method.starts_with("Discover")
    {
        HumanAdapterKind::TypedRead
    } else {
        HumanAdapterKind::TypedCommand
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const PLATFORM_PROTO: &str = concat!(
        include_str!("../../proto/proto/erpx/platform/v1/asset_maintenance.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/assets.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/brand.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/commercial.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/crm.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/customer.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/customer_360.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/data_plane.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/finance.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/identity_access.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/inventory.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/legal.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/marketing.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/maintenance.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/operations.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/organization.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/people_payroll.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/platform_administration.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/pos.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/procurement.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/quality.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/sales.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/supply_chain.proto"),
        include_str!("../../proto/proto/erpx/platform/v1/support.proto"),
    );

    fn descriptor_rpcs() -> BTreeSet<(String, String)> {
        let mut service = None::<String>;
        let mut result = BTreeSet::new();
        for line in PLATFORM_PROTO.lines().map(str::trim) {
            if let Some(rest) = line.strip_prefix("service ") {
                service = rest.split_whitespace().next().map(str::to_owned);
            } else if line == "}" {
                service = None;
            } else if let Some(rest) = line.strip_prefix("rpc ") {
                let method = rest.split('(').next().unwrap_or_default().trim();
                if let Some(service) = service.as_ref() {
                    result.insert((service.clone(), method.to_owned()));
                }
            }
        }
        result
    }

    #[test]
    fn every_descriptor_rpc_has_an_explicit_classification() {
        let descriptor = descriptor_rpcs();
        assert!(!descriptor.is_empty());
        for (service, method) in &descriptor {
            assert!(
                rpc_workflow(service, method).is_some(),
                "{service}.{method} has no RPC classification"
            );
        }
        for service in RPC_WORKFLOW_SERVICES {
            for method in service.methods {
                assert!(
                    descriptor.contains(&(service.service.to_owned(), (*method).to_owned())),
                    "stale RPC classification: {}.{method}",
                    service.service
                );
            }
        }
    }

    #[test]
    fn every_human_rpc_has_routes_adapter_action_and_visible_states() {
        for service in RPC_WORKFLOW_SERVICES {
            for method in service.methods {
                let workflow = rpc_workflow(service.service, method).expect("registered RPC");
                if workflow.classification.requires_human_surface() {
                    assert!(
                        !workflow.routes.is_empty(),
                        "{}.{} has no route",
                        service.service,
                        method
                    );
                    assert!(
                        workflow.adapter.is_some(),
                        "{}.{} has no adapter",
                        service.service,
                        method
                    );
                    assert!(
                        workflow.action_capability.is_some(),
                        "{}.{} has no action",
                        service.service,
                        method
                    );
                    assert_eq!(
                        workflow.states, HUMAN_UI_STATES,
                        "{}.{} has an incomplete state model",
                        service.service, method
                    );
                    assert!(
                        workflow
                            .routes
                            .iter()
                            .all(|route| route.starts_with('/') && !route.contains(['?', '#']))
                    );
                } else {
                    assert!(workflow.routes.is_empty());
                    assert!(workflow.adapter.is_none());
                    assert!(workflow.action_capability.is_none());
                    assert!(workflow.states.is_empty());
                }
            }
        }
    }

    #[test]
    fn anonymous_marketing_and_agent_boundaries_cannot_be_confused() {
        let public =
            rpc_workflow("MarketingContentService", "SubmitMarketingForm").expect("public form");
        assert_eq!(public.classification, RpcClassification::PublicHuman);
        assert_eq!(public.routes, PUBLIC_MARKETING_ROUTES);

        for method in DATA_PLANE_AGENT_METHODS {
            let machine = rpc_workflow("DataPlaneAgentService", method).expect("agent RPC");
            assert_eq!(machine.classification, RpcClassification::MachineOnly);
            assert!(!machine.classification.requires_human_surface());
        }

        // Keep the complete vocabulary present even when the current descriptor
        // exposes provider and worker ingress over HTTP/outbox rather than RPC.
        assert!(!RpcClassification::ProviderIngress.requires_human_surface());
        assert!(!RpcClassification::WorkerOnly.requires_human_surface());
    }
}
