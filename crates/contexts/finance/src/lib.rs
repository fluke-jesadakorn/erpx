#![doc = "Finance bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;
pub mod sales_posting_application;

pub use application::{
    AddAccount, ApplySubledgerSettlement, BeginPeriodClose, CancelCustomerInvoice,
    ChangeAccountStatus, ClosePeriod, CreateChart, CreateCustomerInvoiceFromBilling, CreateJournal,
    CreatePayableFromProcurement, CreatePayment, CreatePeriod, CreateReceivableFromInvoice,
    CreateRevenueScheduleFromArrangement, CreateVendorInvoice, FinanceApplicationError,
    FinanceCommand, FinanceCommandHandler, FinanceMutationResult, FinanceQuery,
    FinanceQueryHandler, FinanceQueryResponse, FinanceRecordApplicationError,
    FinanceRecordCommandHandler, GetChart, GetCustomerInvoice, GetJournal, GetPayment, GetPeriod,
    GetRevenueSchedule, GetStatutoryTaxReport, GetSubledger, GetTrialBalance, GetVendorInvoice,
    ListBankTransactions, ListCharts, ListJournals, ListSubledger, PayablesApplicationError,
    PayablesCommand, PayablesCommandHandler, PayablesMutationResult, PayablesQuery,
    PayablesQueryHandler, PayablesQueryResponse, PostCustomerInvoice, PostJournal,
    PostVendorInvoice, RecognizeRevenueFromFulfillment, ReleasePayment, ReopenPeriod,
};
pub use domain::{
    Account, AccountCategory, AccountId, AccountStatus, AccountingObligation, AccountingPeriod,
    AccountingPlan, AccountingPlanId, AccountingPlanStatus, AccountingPlanSuggestionPayload,
    AccountingPlanSuggestionProvenance, BankTransactionFact, Chart, ChartId, CustomerInvoice,
    CustomerInvoiceId, CustomerInvoiceStatus, FinanceAggregate, FinanceDomainError, FinanceRecord,
    FinanceRecordError, FinanceRecordKind, FinanceRecordStatus, FinanceTaxClassification,
    FinanceTaxProfileSnapshot, Journal, JournalId, JournalLine, JournalLineId, JournalSource,
    JournalStatus, NormalBalance, Payable, PayableId, PayablesAggregate, PayablesDomainError,
    PayablesDomainEvent, Payment, PaymentAllocation, PaymentId, PaymentReleasedAllocationState,
    PaymentReleasedState, PaymentStatus, PeriodId, PeriodStatus, PostingActuals, PostingEntry,
    PostingInstructions, PostingPacket, PostingPacketId, PostingPacketStatus, PostingReceipt,
    PostingReviewRecommendation, PostingReviewSuggestionPayload, PostingSide,
    PostingValidationContext, RevenueObligation, RevenueRecognitionEntry, RevenueSchedule,
    RevenueScheduleId, RevenueScheduleStatus, SalesAccountRoles, SalesPostingError,
    StatutoryTaxFact, StatutoryTaxKind, SubledgerId, SubledgerItem, SubledgerKind, SubledgerStatus,
    ThreeWayMatchEvidence, ThreeWayMatchLineEvidence, ThreeWayMatchTolerance, VendorInvoice,
    VendorInvoiceId, VendorInvoiceLine, VendorInvoiceLineId, VendorInvoicePostedState,
    VendorInvoiceStatus, payment_release_decision_ref, vendor_invoice_post_decision_ref,
};
pub use ports::{
    AuthorizationPort, BankTransactionList, BankTransactionListFilter, BankTransactionView,
    CommandUnitOfWork, CustomerInvoiceView, EntitlementPort, FinanceAction, FinanceCommit,
    FinanceRecordAction, FinanceRecordAuthorizationPort, FinanceRecordAuthorizationRequest,
    FinanceRecordCommand, FinanceRecordCommandIdentity, FinanceRecordCommandRequest,
    FinanceRecordCommit, FinanceRecordReadRepository, FinanceRecordUnitOfWork,
    FinanceRecordWriteRepository, JournalList, JournalListFilter, PayableView, PayablesAction,
    PayablesAuthorizationPort, PayablesAuthorizationRequest, PayablesCommandFingerprint,
    PayablesCommandIdentity, PayablesCommandUnitOfWork, PayablesCommit, PayablesEntitlementPort,
    PayablesReadRepository, PayablesReferenceVersion, PayablesWriteRepository, PaymentView,
    PortError, ReadProjection, ReadRepository, RevenueScheduleView, StatutoryTaxFactView,
    StatutoryTaxReport, StoredCommit, StoredFinanceRecordCommit, StoredPayablesCommit,
    SubledgerList, SubledgerListFilter, ThreeWayMatchEvidencePort, TrialBalanceLine,
    TrialBalanceView, VendorInvoiceView, WriteRepository,
};
pub use sales_posting_application::{
    PostingValidationSnapshot, SalesPostingAggregate, SalesPostingApplicationError,
    SalesPostingAuthorizationPort, SalesPostingCommand, SalesPostingCommandReceipt,
    SalesPostingCommandRequest, SalesPostingCommit, SalesPostingRepository, SalesPostingService,
};
