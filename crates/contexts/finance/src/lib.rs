#![doc = "Finance bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AddAccount, ApplySubledgerSettlement, BeginPeriodClose, CancelCustomerInvoice,
    ChangeAccountStatus, ClosePeriod, CreateChart, CreateCustomerInvoiceFromBilling, CreateJournal,
    CreatePayableFromProcurement, CreatePeriod, CreateReceivableFromInvoice,
    CreateRevenueScheduleFromArrangement, FinanceApplicationError, FinanceCommand,
    FinanceCommandHandler, FinanceMutationResult, FinanceQuery, FinanceQueryHandler,
    FinanceQueryResponse, GetChart, GetCustomerInvoice, GetJournal, GetPeriod, GetRevenueSchedule,
    GetStatutoryTaxReport, GetSubledger, GetTrialBalance, ListBankTransactions, ListJournals,
    ListSubledger, PostCustomerInvoice, PostJournal, RecognizeRevenueFromFulfillment, ReopenPeriod,
};
pub use domain::{
    Account, AccountCategory, AccountId, AccountStatus, AccountingPeriod, BankTransactionFact,
    Chart, ChartId, CustomerInvoice, CustomerInvoiceId, CustomerInvoiceStatus, FinanceAggregate,
    FinanceDomainError, Journal, JournalId, JournalLine, JournalLineId, JournalSource,
    JournalStatus, NormalBalance, PeriodId, PeriodStatus, RevenueObligation,
    RevenueRecognitionEntry, RevenueSchedule, RevenueScheduleId, RevenueScheduleStatus,
    StatutoryTaxFact, StatutoryTaxKind, SubledgerId, SubledgerItem, SubledgerKind, SubledgerStatus,
};
pub use ports::{
    AuthorizationPort, BankTransactionList, BankTransactionListFilter, BankTransactionView,
    CommandUnitOfWork, CustomerInvoiceView, EntitlementPort, FinanceAction, FinanceCommit,
    JournalList, JournalListFilter, PortError, ReadRepository, RevenueScheduleView,
    StatutoryTaxFactView, StatutoryTaxReport, StoredCommit, SubledgerList, SubledgerListFilter,
    TrialBalanceLine, TrialBalanceView, WriteRepository,
};
