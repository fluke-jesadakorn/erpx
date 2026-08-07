#![doc = "Finance bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AddAccount, ApplySubledgerSettlement, BeginPeriodClose, ChangeAccountStatus, ClosePeriod,
    CreateChart, CreateJournal, CreatePayableFromProcurement, CreatePeriod,
    CreateReceivableFromSales, FinanceApplicationError, FinanceCommand, FinanceCommandHandler,
    FinanceMutationResult, FinanceQuery, FinanceQueryHandler, FinanceQueryResponse, GetChart,
    GetJournal, GetPeriod, GetStatutoryTaxReport, GetSubledger, GetTrialBalance,
    ListBankTransactions, ListJournals, ListSubledger, PostJournal, ReopenPeriod,
};
pub use domain::{
    Account, AccountCategory, AccountId, AccountStatus, AccountingPeriod, BankTransactionFact,
    Chart, ChartId, FinanceAggregate, FinanceDomainError, Journal, JournalId, JournalLine,
    JournalLineId, JournalSource, JournalStatus, NormalBalance, PeriodId, PeriodStatus,
    StatutoryTaxFact, StatutoryTaxKind, SubledgerId, SubledgerItem, SubledgerKind, SubledgerStatus,
};
pub use ports::{
    AuthorizationPort, BankTransactionList, BankTransactionListFilter, BankTransactionView,
    CommandUnitOfWork, EntitlementPort, FinanceAction, FinanceCommit, JournalList,
    JournalListFilter, PortError, ReadRepository, StatutoryTaxFactView, StatutoryTaxReport,
    StoredCommit, SubledgerList, SubledgerListFilter, TrialBalanceLine, TrialBalanceView,
    WriteRepository,
};
