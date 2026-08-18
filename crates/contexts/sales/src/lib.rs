#![doc = "Sales bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;
pub mod quote_workflow_application;

pub use application::{
    AcceptQuote, AcceptRevenueFulfillment, ActivateRevenueArrangement, CancelRevenueArrangement,
    CancelSalesOrder, ConfirmSalesOrder, CreateQuote, CreateSalesOrder, ExpireQuote, GetQuote,
    GetRevenueArrangement, GetSalesOrder, ListSalesOrders, MarkSalesOrderFulfilled,
    MarkSalesOrderPartiallyFulfilled, RejectQuote, SalesApplicationError, SalesCommand,
    SalesCommandHandler, SalesLifecycleRecordApplicationError, SalesLifecycleRecordCommandHandler,
    SalesMutationResult, SalesQuery, SalesQueryHandler, SalesQueryResponse, SendQuote,
};
pub use domain::{
    AcceptedFulfillment, AppliedQuoteSuggestion, AuthoritativeTaxProfile, BillingEventId,
    ExternalQuoteDecision, ExternalQuoteDecisionKind, FulfillmentId, MAX_QUOTE_WORKFLOW_LINES,
    PerformanceObligation, PerformanceObligationDefinition, PerformanceObligationId, Quote,
    QuoteAccountingPlanEvidence, QuoteApprovalEvidence, QuoteApprovalRejectionEvidence,
    QuoteDocumentLanguage, QuoteDraftLine, QuoteDraftLineSource, QuoteDraftPayload, QuoteId,
    QuotePdfArtifact, QuoteStatus, QuoteWorkflow, QuoteWorkflowError, QuoteWorkflowStatus,
    RevenueArrangement, RevenueArrangementId, RevenueArrangementStatus,
    RevenueArrangementSuggestionPayload, RevenueObligationSuggestion, SalesAggregate,
    SalesDomainError, SalesLifecycleRecord, SalesLifecycleRecordError, SalesLifecycleRecordKind,
    SalesLifecycleRecordStatus, SalesOrder, SalesOrderId, SalesOrderLine, SalesOrderLineId,
    SalesOrderStatus, SupersededQuoteVersionEvidence, THAI_VAT_RATE_BPS, TaxClassification,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, PortError, QuoteView, ReadProjection,
    ReadRepository, RevenueArrangementView, SalesAction, SalesCommit, SalesLifecycleRecordAction,
    SalesLifecycleRecordAuthorizationPort, SalesLifecycleRecordAuthorizationRequest,
    SalesLifecycleRecordCommand, SalesLifecycleRecordCommandIdentity,
    SalesLifecycleRecordCommandRequest, SalesLifecycleRecordCommit,
    SalesLifecycleRecordReadRepository, SalesLifecycleRecordUnitOfWork,
    SalesLifecycleRecordWriteRepository, SalesOrderList, SalesOrderListFilter, SalesOrderView,
    StoredCommit, StoredSalesLifecycleRecordCommit, WriteRepository,
};
pub use quote_workflow_application::{
    QuoteWorkflowApplicationError, QuoteWorkflowAuthorizationPort, QuoteWorkflowCommand,
    QuoteWorkflowCommandRequest, QuoteWorkflowCommit, QuoteWorkflowPage, QuoteWorkflowPermission,
    QuoteWorkflowReceipt, QuoteWorkflowRepository, QuoteWorkflowService,
};
