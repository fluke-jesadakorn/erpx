//! Shared, route-agnostic Dioxus UI foundations for ERPX clients.
//!
//! This crate owns presentation primitives and client-local interaction state. It does not
//! know about authentication tokens, ERPX protobuf messages, tenant data, or server-side
//! authorization. Applications provide those concerns at their boundary.

mod auth;
mod components;
mod crm;
mod customer360;
mod finance;
mod hr;
mod i18n;
mod legal;
mod navigation;
mod operations;
mod platform;
mod pos;
mod pos_commands;
mod product_contexts;
mod routes;
mod session_target;
mod state;
mod support;
mod support_surfaces;
mod theme;
mod workflows;

pub use auth::{
    AccessToken, ActiveRoleMetadata, AuthTransportError, ExpiryTimestamp, IssuedSession,
    LoginRequest, RefreshToken, SecretInput, SecureSessionBundleStore, SecureSessionError,
    SecureSessionFuture, SecureSessionStore, SessionCommandError, SessionController, SessionFuture,
    SessionId, SessionMaterialError, SessionSnapshot, SessionState, SessionTransport,
    StoredAccountSummary, StoredSession, StoredSessionBundle,
};
pub use components::{
    AppShell, BannerKind, CollectionLayout, CorporateIdentityEditor, CorporateIdentityEditorAction,
    CorporateIdentityEditorActionKind, CorporateIdentityEditorSubmission, DashboardLayout,
    DataTable, Dialog, EnterpriseIcon, Field, FilterBar, LocaleToggle, OperationsLayout,
    PageHeader, RecordLayout, RouteStateBoundary, ScopeSelector, SessionBanners, SettingsLayout,
    StateBoundary, StatePanel, TableColumn, TextField, WorkflowLayout,
};
pub use crm::{
    CRM_SURFACES, CrmAttributionDisplayModel, CrmAttributionModel, CrmDisplayField, CrmFieldInput,
    CrmHandoffDisplayModel, CrmOpportunityDisplayModel, CrmOpportunityStatus, CrmPageError,
    CrmPageMetadata, CrmProjectionMetadata, CrmReadPage, CrmState, CrmSurface, CrmViewState,
    MAX_CRM_DISPLAY_FIELDS, MAX_CRM_DISPLAY_VALUE_BYTES, MAX_CRM_IDENTIFIER_BYTES,
    MAX_CRM_PAGE_SIZE, sort_attribution_rows, sort_handoff_rows, sort_opportunity_rows,
};
pub use customer360::{
    CUSTOMER_360_SECTIONS, Customer360DisplayField, Customer360DisplayModel, Customer360FieldInput,
    Customer360Freshness, Customer360ProjectionMetadata, Customer360Section, Customer360State,
    Customer360ViewState, CustomerNoteDisplayItem, CustomerNoteDtoInput, CustomerNotesDisplayModel,
    ProjectionFreshness,
};
pub use finance::{
    FINANCE_ROUTE_REGISTRY, FINANCE_ROUTES, FinanceBankingDisplayModel, FinanceBankingState,
    FinanceChartDisplayModel, FinanceChartState, FinanceCloseDisplayModel, FinanceCloseState,
    FinanceDisplayField, FinanceDisplayModel, FinanceDisplayValueError, FinanceIdentifierError,
    FinanceJournalDisplayModel, FinanceJournalState, FinanceLabel, FinanceModelError,
    FinanceOpaqueId, FinancePageState, FinancePageStateKind, FinanceProjectionMetadata,
    FinanceReadPage, FinanceRoute, FinanceRouteDefinition, FinanceRouteId, FinanceRouteParseError,
    FinanceRouteRegistry, FinanceSafeText, FinanceScalar, FinanceScope, FinanceState,
    FinanceSubledgerDisplayModel, FinanceSubledgerState, FinanceSurface, FinanceViewState,
    MAX_FINANCE_DISPLAY_FIELDS, MAX_FINANCE_DISPLAY_VALUE_BYTES, MAX_FINANCE_IDENTIFIER_BYTES,
    MAX_FINANCE_PAGE_ROWS,
};
pub use hr::{
    HR_ROUTE_REGISTRY, HR_ROUTES, HrCurrency, HrCurrencyError, HrDisplayValueError,
    HrEmployeeDisplayModel, HrEmployeeId, HrEmployeeState, HrEmployeeStatus, HrEmployeeViewModel,
    HrExpenseDisplayModel, HrExpenseId, HrExpenseState, HrExpenseStatus, HrExpenseViewModel,
    HrIdentifierError, HrLeaveDisplayModel, HrLeaveId, HrLeaveState, HrLeaveStatus,
    HrLeaveViewModel, HrOpaqueId, HrPageState, HrPageStateKind, HrPayrollDisplayModel, HrPayrollId,
    HrPayrollLineDisplayModel, HrPayrollRunId, HrPayrollState, HrPayrollStatus, HrPayrollViewModel,
    HrPrincipalId, HrProjectionMetadata, HrReassignmentDisplayModel, HrReassignmentId,
    HrReassignmentState, HrReassignmentStatus, HrReassignmentViewModel, HrRoute, HrRouteDefinition,
    HrRouteId, HrRouteParseError, HrRouteRegistry, HrScope, HrState, HrViewModelError, HrViewState,
    HrWorkflowRef, MAX_HR_DISPLAY_VALUE_BYTES, MAX_HR_IDENTIFIER_BYTES, MAX_HR_PAGE_ROWS,
    MAX_HR_PAYROLL_LINES,
};
pub use i18n::{
    Locale, LocaleContext, LocaleFormatter, LocalizedText, TranslationKey, provide_locale,
    use_locale,
};
pub use legal::{
    LEGAL_ROUTE_REGISTRY, LEGAL_ROUTES, LegalAccessAction, LegalAccessHistoryEntryDisplayModel,
    LegalAccessHistoryEntryId, LegalAccessHistoryEntryState, LegalApprovalDisplayModel,
    LegalApprovalId, LegalApprovalState, LegalApprovalStatus, LegalContractDisplayModel,
    LegalContractId, LegalContractState, LegalContractStatus, LegalDisplayTextError,
    LegalDocumentDisplayModel, LegalDocumentId, LegalDocumentState, LegalDocumentStatus,
    LegalDocumentVersionDisplayModel, LegalDocumentVersionId, LegalDocumentVersionState,
    LegalIdentifierError, LegalLabel, LegalPageState, LegalPageStateKind, LegalPartyDisplayModel,
    LegalPartyId, LegalPartyRole, LegalPartyState, LegalPrincipalId, LegalProjectionMetadata,
    LegalRetentionDisplayModel, LegalRetentionId, LegalRetentionState, LegalRetentionStatus,
    LegalRoute, LegalRouteDescriptor, LegalRouteId, LegalRouteParseError, LegalRouteRegistry,
    LegalScalar, MAX_LEGAL_DISPLAY_TEXT_BYTES, MAX_LEGAL_IDENTIFIER_BYTES,
};
pub use navigation::{NavGroup, NavItem, NavigationState, normalize_path};
pub use operations::{
    OPERATIONS_PAGES, OperationsDisplayField, OperationsDisplayModel, OperationsFieldInput,
    OperationsPage, OperationsProjectionMetadata, OperationsScope, OperationsScopeKind,
    OperationsSourceDomain, OperationsState, OperationsViewState,
};
pub use platform::{
    MAX_PLATFORM_TENANT_ID_BYTES, PLATFORM_OPERATIONS_ROUTES, PlatformOperationsAudience,
    PlatformOperationsPageState, PlatformOperationsProjectionMetadata, PlatformOperationsRoute,
    PlatformOperationsRouteDefinition, PlatformOperationsRouteError, PlatformOperationsRouteId,
    PlatformOperationsRouteRegistry, PlatformTenantAdministrationTab, PlatformTenantId,
    TenantWorkspaceBillingMode, TenantWorkspaceContext, TenantWorkspaceModule,
    TenantWorkspaceOrganizationScope, TenantWorkspaceViewerKind,
};
pub use pos::{
    MAX_POS_DISPLAY_LINES, MAX_POS_DISPLAY_VALUE_BYTES, MAX_POS_LINE_ID_BYTES,
    MAX_POS_PAYMENT_SUMMARIES, MAX_POS_SALE_ID_BYTES, POS_ROUTE_REGISTRY, POS_ROUTES, PosCurrency,
    PosCurrencyError, PosDisplayValueError, PosIdentifierError, PosLineId, PosPageState,
    PosPageStateKind, PosPaymentCategory, PosPaymentMethod, PosPaymentSummary,
    PosPaymentSummaryViewModel, PosProjectionMetadata, PosRoute, PosRouteDefinition, PosRouteId,
    PosRouteParseError, PosRouteRegistry, PosSaleId, PosSaleLine, PosSaleLineViewModel,
    PosSaleState, PosSaleStatus, PosSaleSummary, PosSaleSummaryViewModel, PosSaleViewModel,
    PosState, PosViewModelError, PosViewState,
};
pub use pos_commands::{
    CurrencyCode as PosCommandCurrencyCode, MAX_OPAQUE_ID_BYTES as MAX_POS_COMMAND_ID_BYTES,
    MAX_SALE_ENTRY_LINES, MAX_SCALAR_TEXT_BYTES as MAX_POS_COMMAND_SCALAR_BYTES,
    Money as PosCommandMoney, POS_COMMANDS, PosCommandInputError, PosCommandKind, PosCommandLocale,
    PosCommandState, PosCommandStateKind, PosOpaqueId, ReceiptDeliveryMode, ReceiptIssueInput,
    ReceiptIssueStatus, ReceiptIssueSummary, RefundReviewInput, RefundReviewStatus,
    RefundReviewSummary, SaleEntryInput, SaleEntryLine, SaleEntryStatus, SaleEntrySummary,
    ScalarText as PosCommandScalarText, ShiftCloseApprovalInput, ShiftCloseStatus,
    ShiftCloseSummary,
};
pub use product_contexts::{
    PRODUCT_CONTEXT_ROUTES, ProductContextId, ProductContextRoute, product_context_nav_items,
};
pub use routes::{
    CustomerId, CustomerView, ImpersonationMetadata, InvalidCustomerId, MAX_CUSTOMER_ID_BYTES,
    PLATFORM_CUSTOMER_360_ROUTES, PlatformRoute, ReadOnlyMetadata, RouteAuthRequirement,
    RouteDefinition, RouteId, RouteParseError, RouteRegistry, RouteState, SafeSessionMetadata,
    ScopeKind, ScopeOption, SupportAccessMetadata, normalize_deep_link, redact_deep_link,
};
pub use session_target::{
    ClientTarget, SecureSessionSupport, TargetSecureSessionAdapter, TargetSessionDescriptor,
};
pub use state::{
    FormState, SafeUiText, UiAccessibilityContract, UiState, UiStateKind, ValidationError,
    validate_email, validate_required,
};
pub use support::{
    DIAGNOSTIC_READ_MAX_SECONDS, IMPERSONATION_MAX_SECONDS, IncidentDetailState,
    IncidentDisplayModel, IncidentDtoInput, IncidentListDisplayModel, IncidentListItem,
    IncidentListState, IncidentProjectionMetadata, IncidentScope, IncidentStatus,
    IncidentViewState, SUPPORT_ROUTES, SupportApprovalState, SupportContextMetadata,
    SupportDisplayField, SupportDisplayModel, SupportDurationError, SupportFieldInput,
    SupportImpersonationState, SupportMode, SupportProjectionMetadata, SupportRoute,
    SupportRouteAudience, SupportRouteDefinition, SupportState, SupportTicketDtoInput,
    SupportTicketListDisplayModel, SupportTicketListItem, SupportTicketListState,
    SupportTicketStatus, SupportViewState, TENANT_MUTATION_MAX_SECONDS,
};
pub use support_surfaces::{
    IncidentDetailSurface, IncidentListSurface, SupportIncidentSurface,
    SupportIncidentSurfaceState, SupportRouteTabs, SupportSurface, SupportTicketListSurface,
};
pub use theme::{
    CorporateIdentityContext, ThemeContext, ThemeMode, corporate_identity_style,
    provide_corporate_identity, provide_theme, use_corporate_identity, use_theme,
};
pub use workflows::{
    HUMAN_UI_STATES, HumanAdapterKind, HumanUiState, RPC_WORKFLOW_SERVICES, RpcClassification,
    RpcServiceWorkflow, RpcWorkflow, rpc_workflow,
};

/// Shared CSS variables and component styles. Applications can include this once in their root
/// component and layer client-specific styles on top.
pub const SHARED_STYLES: &str = include_str!("../assets/ui.css");
