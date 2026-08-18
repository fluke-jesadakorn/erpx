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
    CreatePayableFromProcurement, CreatePeriod, CreateReceivableFromInvoice,
    CreateRevenueScheduleFromArrangement, FinanceApplicationError, FinanceCommand,
    FinanceCommandHandler, FinanceMutationResult, FinanceQuery, FinanceQueryHandler,
    FinanceQueryResponse, FinanceRecordApplicationError, FinanceRecordCommandHandler, GetChart,
    GetCustomerInvoice, GetJournal, GetPeriod, GetRevenueSchedule, GetStatutoryTaxReport,
    GetSubledger, GetTrialBalance, ListBankTransactions, ListCharts, ListJournals, ListSubledger,
    PostCustomerInvoice, PostJournal, RecognizeRevenueFromFulfillment, ReopenPeriod,
};
pub use domain::{
    Account, AccountCategory, AccountId, AccountStatus, AccountingObligation, AccountingPeriod,
    AccountingPlan, AccountingPlanId, AccountingPlanStatus, AccountingPlanSuggestionPayload,
    AccountingPlanSuggestionProvenance, BankTransactionFact, Chart, ChartId, CustomerInvoice,
    CustomerInvoiceId, CustomerInvoiceStatus, FinanceAggregate, FinanceDomainError, FinanceRecord,
    FinanceRecordError, FinanceRecordKind, FinanceRecordStatus, FinanceTaxClassification,
    FinanceTaxProfileSnapshot, Journal, JournalId, JournalLine, JournalLineId, JournalSource,
    JournalStatus, NormalBalance, PeriodId, PeriodStatus, PostingActuals, PostingEntry,
    PostingInstructions, PostingPacket, PostingPacketId, PostingPacketStatus, PostingReceipt,
    PostingReviewRecommendation, PostingReviewSuggestionPayload, PostingSide,
    PostingValidationContext, RevenueObligation, RevenueRecognitionEntry, RevenueSchedule,
    RevenueScheduleId, RevenueScheduleStatus, SalesAccountRoles, SalesPostingError,
    StatutoryTaxFact, StatutoryTaxKind, SubledgerId, SubledgerItem, SubledgerKind, SubledgerStatus,
};
pub use ports::{
    AuthorizationPort, BankTransactionList, BankTransactionListFilter, BankTransactionView,
    CommandUnitOfWork, CustomerInvoiceView, EntitlementPort, FinanceAction, FinanceCommit,
    FinanceRecordAction, FinanceRecordAuthorizationPort, FinanceRecordAuthorizationRequest,
    FinanceRecordCommand, FinanceRecordCommandIdentity, FinanceRecordCommandRequest,
    FinanceRecordCommit, FinanceRecordReadRepository, FinanceRecordUnitOfWork,
    FinanceRecordWriteRepository, JournalList, JournalListFilter, PortError, ReadProjection,
    ReadRepository, RevenueScheduleView, StatutoryTaxFactView, StatutoryTaxReport, StoredCommit,
    StoredFinanceRecordCommit, SubledgerList, SubledgerListFilter, TrialBalanceLine,
    TrialBalanceView, WriteRepository,
};
pub use sales_posting_application::{
    PostingValidationSnapshot, SalesPostingAggregate, SalesPostingApplicationError,
    SalesPostingAuthorizationPort, SalesPostingCommand, SalesPostingCommandReceipt,
    SalesPostingCommandRequest, SalesPostingCommit, SalesPostingRepository, SalesPostingService,
};
