use async_trait::async_trait;
use chrono::NaiveDate;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::{RecordRef, finance, procurement, sales};
use erpx_kernel::{
    AuthorityScope, Clock, CommandId, CorrelationId, ErrorCategory, ErrorContext,
    HumanDecisionProof, KernelError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        Account, AccountId, AccountStatus, AccountingPeriod, Chart, ChartId, CustomerInvoice,
        CustomerInvoiceId, FinanceAggregate, FinanceDomainError, FxRate, FxSnapshot, Journal,
        JournalId, JournalLine, JournalSource, PeriodId, RevenueSchedule, RevenueScheduleId,
        SubledgerId, SubledgerItem, SubledgerKind, journal_post_decision_ref,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, BankTransactionList, BankTransactionListFilter,
        ChartList, ChartListFilter, ChartView, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, CustomerInvoiceView, EntitlementPort, FinanceAction, FinanceCommit,
        JournalList, JournalListFilter, JournalView, PeriodView, PortError, ReadRepository,
        ReferenceVersion, RevenueScheduleView, StatutoryTaxReport, StoredCommit, SubledgerList,
        SubledgerListFilter, SubledgerView, TrialBalanceView, WriteRepository,
    },
};

pub const MAX_FINANCE_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChart {
    pub chart_id: ChartId,
    pub scope: AuthorityScope,
    pub code: String,
    pub name: String,
    pub currency: String,
    pub accounts: Vec<Account>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddAccount {
    pub chart_id: ChartId,
    pub account: Account,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeAccountStatus {
    pub chart_id: ChartId,
    pub account_id: AccountId,
    pub status: AccountStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePeriod {
    pub period_id: PeriodId,
    pub scope: AuthorityScope,
    pub code: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeginPeriodClose {
    pub period_id: PeriodId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosePeriod {
    pub period_id: PeriodId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReopenPeriod {
    pub period_id: PeriodId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateJournal {
    pub journal_id: JournalId,
    pub scope: AuthorityScope,
    pub chart_id: ChartId,
    pub period_id: PeriodId,
    pub journal_code: String,
    pub entry_date: NaiveDate,
    pub currency: String,
    pub fx_rate: Option<FxRate>,
    pub source: JournalSource,
    pub description: String,
    pub lines: Vec<JournalLine>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostJournal {
    pub journal_id: JournalId,
    pub expected_chart_version: u64,
    pub expected_period_version: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateCustomerInvoiceFromBilling {
    pub invoice_id: CustomerInvoiceId,
    pub scope: AuthorityScope,
    pub invoice_code: String,
    pub source: RecordRef,
    pub fact: sales::BillingEligibilityFact,
    pub due_on: NaiveDate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostCustomerInvoice {
    pub invoice_id: CustomerInvoiceId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelCustomerInvoice {
    pub invoice_id: CustomerInvoiceId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRevenueScheduleFromArrangement {
    pub schedule_id: RevenueScheduleId,
    pub scope: AuthorityScope,
    pub source: RecordRef,
    pub fact: sales::RevenueArrangementFact,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognizeRevenueFromFulfillment {
    pub schedule_id: RevenueScheduleId,
    pub source: RecordRef,
    pub fact: sales::FulfillmentAcceptedFact,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateReceivableFromInvoice {
    pub subledger_id: SubledgerId,
    pub scope: AuthorityScope,
    pub fact: finance::CustomerInvoicePostedFact,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePayableFromProcurement {
    pub subledger_id: SubledgerId,
    pub scope: AuthorityScope,
    pub fact: procurement::ReleasedPurchaseOrderFact,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplySubledgerSettlement {
    pub subledger_id: SubledgerId,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevalueSubledgers {
    pub period_id: PeriodId,
    pub chart_id: ChartId,
    pub fx_snapshot: FxSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportChartCsv {
    pub chart_id: ChartId,
    pub scope: AuthorityScope,
    pub code: String,
    pub name: String,
    pub currency: String,
    pub csv_rows: Vec<crate::domain::ChartCsvRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportOpeningTrialBalance {
    pub chart_id: ChartId,
    pub period_id: PeriodId,
    pub rows: Vec<crate::domain::OpeningTrialBalanceRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetFinancialStatements {
    pub chart_id: ChartId,
    pub period_id: PeriodId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListAging {
    pub kind: SubledgerKind,
    pub as_of: NaiveDate,
    pub page_size: u16,
    pub after_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum FinanceCommand {
    CreateChart(CreateChart),
    AddAccount(AddAccount),
    ChangeAccountStatus(ChangeAccountStatus),
    CreatePeriod(CreatePeriod),
    BeginPeriodClose(BeginPeriodClose),
    ClosePeriod(ClosePeriod),
    ReopenPeriod(ReopenPeriod),
    CreateJournal(CreateJournal),
    PostJournal(PostJournal),
    CreateCustomerInvoiceFromBilling(CreateCustomerInvoiceFromBilling),
    PostCustomerInvoice(PostCustomerInvoice),
    CancelCustomerInvoice(CancelCustomerInvoice),
    CreateRevenueScheduleFromArrangement(CreateRevenueScheduleFromArrangement),
    RecognizeRevenueFromFulfillment(RecognizeRevenueFromFulfillment),
    CreateReceivableFromInvoice(CreateReceivableFromInvoice),
    CreatePayableFromProcurement(CreatePayableFromProcurement),
    ApplySubledgerSettlement(ApplySubledgerSettlement),
    RevalueSubledgers(RevalueSubledgers),
    ImportChartCsv(ImportChartCsv),
    ImportOpeningTrialBalance(ImportOpeningTrialBalance),
}

impl FinanceCommand {
    const fn action(&self) -> FinanceAction {
        match self {
            Self::CreateChart(_) => FinanceAction::CreateChart,
            Self::AddAccount(_) => FinanceAction::AddAccount,
            Self::ChangeAccountStatus(_) => FinanceAction::ChangeAccountStatus,
            Self::CreatePeriod(_) => FinanceAction::CreatePeriod,
            Self::BeginPeriodClose(_) => FinanceAction::BeginPeriodClose,
            Self::ClosePeriod(_) => FinanceAction::ClosePeriod,
            Self::ReopenPeriod(_) => FinanceAction::ReopenPeriod,
            Self::CreateJournal(_) => FinanceAction::CreateJournal,
            Self::PostJournal(_) => FinanceAction::PostJournal,
            Self::CreateCustomerInvoiceFromBilling(_) => FinanceAction::CreateCustomerInvoice,
            Self::PostCustomerInvoice(_) => FinanceAction::PostCustomerInvoice,
            Self::CancelCustomerInvoice(_) => FinanceAction::CancelCustomerInvoice,
            Self::CreateRevenueScheduleFromArrangement(_) => FinanceAction::CreateRevenueSchedule,
            Self::RecognizeRevenueFromFulfillment(_) => FinanceAction::RecognizeRevenue,
            Self::CreateReceivableFromInvoice(_) | Self::CreatePayableFromProcurement(_) => {
                FinanceAction::CreateSubledger
            }
            Self::ApplySubledgerSettlement(_) => FinanceAction::SettleSubledger,
            Self::RevalueSubledgers(_) => FinanceAction::CreateJournal,
            Self::ImportChartCsv(_) => FinanceAction::CreateChart,
            Self::ImportOpeningTrialBalance(_) => FinanceAction::CreateJournal,
        }
    }

    const fn is_create(&self) -> bool {
        matches!(
            self,
            Self::CreateChart(_)
                | Self::CreatePeriod(_)
                | Self::CreateJournal(_)
                | Self::CreateCustomerInvoiceFromBilling(_)
                | Self::CreateRevenueScheduleFromArrangement(_)
                | Self::CreateReceivableFromInvoice(_)
                | Self::CreatePayableFromProcurement(_)
                | Self::RevalueSubledgers(_)
                | Self::ImportChartCsv(_)
                | Self::ImportOpeningTrialBalance(_)
        )
    }

    const fn requires_preload_human_decision(&self) -> bool {
        matches!(
            self,
            Self::ClosePeriod(_)
                | Self::ReopenPeriod(_)
                | Self::PostJournal(_)
                | Self::PostCustomerInvoice(_)
        )
    }

    fn human_decision_ref(&self, command_id: CommandId) -> Option<String> {
        if !self.requires_preload_human_decision() {
            return None;
        }
        Some(match self {
            Self::PostJournal(value) => journal_post_decision_ref(command_id, value.journal_id),
            _ => command_id.to_string(),
        })
    }

    fn fingerprint(&self) -> CommandFingerprint {
        match self {
            Self::CreateChart(v) => CommandFingerprint::CreateChart {
                chart_id: v.chart_id,
                scope: v.scope.clone(),
                code: v.code.clone(),
                name: v.name.clone(),
                currency: v.currency.clone(),
                accounts: v.accounts.clone(),
            },
            Self::AddAccount(v) => CommandFingerprint::AddAccount {
                chart_id: v.chart_id,
                account: v.account.clone(),
            },
            Self::ChangeAccountStatus(v) => CommandFingerprint::ChangeAccountStatus {
                chart_id: v.chart_id,
                account_id: v.account_id,
                status: v.status,
            },
            Self::CreatePeriod(v) => CommandFingerprint::CreatePeriod {
                period_id: v.period_id,
                scope: v.scope.clone(),
                code: v.code.clone(),
                starts_on: v.starts_on,
                ends_on: v.ends_on,
            },
            Self::BeginPeriodClose(v) => CommandFingerprint::BeginPeriodClose {
                period_id: v.period_id,
            },
            Self::ClosePeriod(v) => CommandFingerprint::ClosePeriod {
                period_id: v.period_id,
            },
            Self::ReopenPeriod(v) => CommandFingerprint::ReopenPeriod {
                period_id: v.period_id,
            },
            Self::CreateJournal(v) => CommandFingerprint::CreateJournal {
                journal_id: v.journal_id,
                scope: v.scope.clone(),
                chart_id: v.chart_id,
                period_id: v.period_id,
                journal_code: v.journal_code.clone(),
                entry_date: v.entry_date,
                currency: v.currency.clone(),
                source: v.source.clone(),
                description: v.description.clone(),
                lines: v.lines.clone(),
            },
            Self::PostJournal(v) => CommandFingerprint::PostJournal {
                journal_id: v.journal_id,
                expected_chart_version: v.expected_chart_version,
                expected_period_version: v.expected_period_version,
            },
            Self::CreateCustomerInvoiceFromBilling(v) => {
                CommandFingerprint::CreateCustomerInvoice {
                    invoice_id: v.invoice_id,
                    scope: v.scope.clone(),
                    invoice_code: v.invoice_code.clone(),
                    source: v.source.clone(),
                    fact: v.fact.clone(),
                    due_on: v.due_on,
                }
            }
            Self::PostCustomerInvoice(v) => CommandFingerprint::PostCustomerInvoice {
                invoice_id: v.invoice_id,
            },
            Self::CancelCustomerInvoice(v) => CommandFingerprint::CancelCustomerInvoice {
                invoice_id: v.invoice_id,
            },
            Self::CreateRevenueScheduleFromArrangement(v) => {
                CommandFingerprint::CreateRevenueSchedule {
                    schedule_id: v.schedule_id,
                    scope: v.scope.clone(),
                    source: v.source.clone(),
                    fact: v.fact.clone(),
                }
            }
            Self::RecognizeRevenueFromFulfillment(v) => {
                CommandFingerprint::RecognizeRevenueFromFulfillment {
                    schedule_id: v.schedule_id,
                    source: v.source.clone(),
                    fact: v.fact.clone(),
                }
            }
            Self::CreateReceivableFromInvoice(v) => {
                CommandFingerprint::CreateReceivableFromInvoice {
                    subledger_id: v.subledger_id,
                    scope: v.scope.clone(),
                    fact: v.fact.clone(),
                }
            }
            Self::CreatePayableFromProcurement(v) => {
                CommandFingerprint::CreatePayableFromProcurement {
                    subledger_id: v.subledger_id,
                    scope: v.scope.clone(),
                    fact: v.fact.clone(),
                }
            }
            Self::ApplySubledgerSettlement(v) => CommandFingerprint::ApplySubledgerSettlement {
                subledger_id: v.subledger_id,
                amount_minor: v.amount_minor,
            },
            Self::RevalueSubledgers(v) => CommandFingerprint::RevalueSubledgers {
                period_id: v.period_id,
                chart_id: v.chart_id,
                fx_snapshot_micros: v.fx_snapshot.rate.micros,
                base_currency: v.fx_snapshot.base_currency.clone(),
                quote_currency: v.fx_snapshot.quote_currency.clone(),
                as_of: v.fx_snapshot.as_of,
            },
            Self::ImportChartCsv(v) => CommandFingerprint::ImportChartCsv {
                chart_id: v.chart_id,
                scope: v.scope.clone(),
                code: v.code.clone(),
                currency: v.currency.clone(),
                row_count: v.csv_rows.len(),
            },
            Self::ImportOpeningTrialBalance(v) => CommandFingerprint::ImportOpeningTrialBalance {
                chart_id: v.chart_id,
                period_id: v.period_id,
                row_count: v.rows.len(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetChart {
    pub chart_id: ChartId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCharts {
    pub filter: ChartListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPeriod {
    pub period_id: PeriodId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetJournal {
    pub journal_id: JournalId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListJournals {
    pub filter: JournalListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSubledger {
    pub subledger_id: SubledgerId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCustomerInvoice {
    pub invoice_id: CustomerInvoiceId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRevenueSchedule {
    pub schedule_id: RevenueScheduleId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSubledger {
    pub filter: SubledgerListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetTrialBalance {
    pub chart_id: ChartId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListBankTransactions {
    pub filter: BankTransactionListFilter,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetStatutoryTaxReport {
    pub tax_period: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum FinanceQuery {
    GetChart(GetChart),
    ListCharts(ListCharts),
    GetPeriod(GetPeriod),
    GetJournal(GetJournal),
    ListJournals(ListJournals),
    GetSubledger(GetSubledger),
    GetCustomerInvoice(GetCustomerInvoice),
    GetRevenueSchedule(GetRevenueSchedule),
    ListSubledger(ListSubledger),
    GetTrialBalance(GetTrialBalance),
    GetFinancialStatements(GetFinancialStatements),
    ListAging(ListAging),
    ListBankTransactions(ListBankTransactions),
    GetStatutoryTaxReport(GetStatutoryTaxReport),
}

impl FinanceQuery {
    const fn action(&self) -> FinanceAction {
        match self {
            Self::GetChart(_) => FinanceAction::ViewChart,
            Self::ListCharts(_) => FinanceAction::ListCharts,
            Self::GetPeriod(_) => FinanceAction::ViewPeriod,
            Self::GetJournal(_) => FinanceAction::ViewJournal,
            Self::ListJournals(_) => FinanceAction::ListJournals,
            Self::GetSubledger(_) => FinanceAction::ViewSubledger,
            Self::GetCustomerInvoice(_) => FinanceAction::ViewCustomerInvoice,
            Self::GetRevenueSchedule(_) => FinanceAction::ViewRevenueSchedule,
            Self::ListSubledger(_) => FinanceAction::ListSubledger,
            Self::GetTrialBalance(_) => FinanceAction::ViewTrialBalance,
            Self::GetFinancialStatements(_) => FinanceAction::ViewFinancialStatements,
            Self::ListAging(_) => FinanceAction::ViewAging,
            Self::ListBankTransactions(_) => FinanceAction::ViewBanking,
            Self::GetStatutoryTaxReport(_) => FinanceAction::ViewStatutoryTax,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum FinanceMutationResult {
    Chart(ChartView),
    Period(PeriodView),
    Journal(JournalView),
    Subledger(SubledgerView),
    CustomerInvoice(CustomerInvoiceView),
    RevenueSchedule(RevenueScheduleView),
}

impl From<&FinanceAggregate> for FinanceMutationResult {
    fn from(value: &FinanceAggregate) -> Self {
        match value {
            FinanceAggregate::Chart(v) => Self::Chart(v.into()),
            FinanceAggregate::Period(v) => Self::Period(v.into()),
            FinanceAggregate::Journal(v) => Self::Journal(v.into()),
            FinanceAggregate::Subledger(v) => Self::Subledger(v.into()),
            FinanceAggregate::CustomerInvoice(v) => Self::CustomerInvoice(v.into()),
            FinanceAggregate::RevenueSchedule(v) => Self::RevenueSchedule(v.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum FinanceQueryResponse {
    Chart(Option<ChartView>),
    Charts(ChartList),
    Period(Option<PeriodView>),
    Journal(Option<JournalView>),
    Journals(JournalList),
    Subledger(Option<SubledgerView>),
    CustomerInvoice(Option<CustomerInvoiceView>),
    RevenueSchedule(Option<RevenueScheduleView>),
    SubledgerList(SubledgerList),
    TrialBalance(TrialBalanceView),
    FinancialStatements(crate::domain::FinancialStatements),
    Aging(crate::domain::AgingReport),
    BankTransactions(BankTransactionList),
    StatutoryTaxReport(StatutoryTaxReport),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FinanceApplicationError {
    #[error(transparent)]
    Domain(#[from] FinanceDomainError),
    #[error("command scope does not contain the Finance record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("Finance aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Finance reference aggregate version conflict")]
    ReferenceVersionConflict,
    #[error("Finance list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("Finance integration event mapping failed")]
    IntegrationMapping,
    #[error("Finance decision requires a directly authenticated human")]
    HumanDecisionRequired,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct FinanceCommandHandler<W, U, A, E, C> {
    write: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}
impl<W, U, A, E, C> FinanceCommandHandler<W, U, A, E, C> {
    pub const fn new(
        write: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        clock: C,
    ) -> Self {
        Self {
            write,
            unit_of_work,
            authorization,
            entitlement,
            clock,
        }
    }
}

impl<W, U, A, E, C> FinanceCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: FinanceAction,
    ) -> Result<(), FinanceApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(FinanceApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_finance_enabled(target_scope, action.requires_write_entitlement())
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target_scope,
                action,
            })
            .await?;
        Ok(())
    }

    async fn replay(
        &self,
        identity: &CommandIdentity,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &AuthorityScope,
        action: FinanceAction,
    ) -> Result<Option<StoredCommit>, FinanceApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(actor, authority_scope, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    async fn validate_existing(
        &self,
        aggregate: &FinanceAggregate,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &AuthorityScope,
        action: FinanceAction,
        expected: Option<u64>,
    ) -> Result<u64, FinanceApplicationError> {
        self.preflight(actor, authority_scope, aggregate.scope(), action)
            .await?;
        let expected = expected.ok_or(FinanceApplicationError::MissingExpectedVersion)?;
        if expected != aggregate.version() {
            return Err(FinanceApplicationError::VersionConflict {
                expected,
                actual: aggregate.version(),
            });
        }
        Ok(expected)
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<FinanceCommand> for FinanceCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = FinanceMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<FinanceCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.command.action();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: envelope.command.fingerprint(),
        };
        // Every human-only Finance decision is bound before idempotency replay
        // or aggregate access, including journal posting.
        let command_decision_ref = envelope.command.human_decision_ref(envelope.command_id);
        let command_decision_at = command_decision_ref.as_ref().map(|_| self.clock.now());
        let mut command_decision = if let Some(decided_at) = command_decision_at {
            Some(
                HumanDecisionProof::try_new(
                    envelope.actor.clone(),
                    envelope.active_scope.clone(),
                    command_decision_ref
                        .expect("preloaded Finance decision has an exact reference"),
                    decided_at,
                )
                .map_err(|_| {
                    to_cqrs(FinanceApplicationError::HumanDecisionRequired, correlation)
                })?,
            )
        } else {
            None
        };
        if let Some(stored) = self
            .replay(&identity, &envelope.actor, &envelope.active_scope, action)
            .await
            .map_err(|e| to_cqrs(e, correlation))?
        {
            let response = FinanceMutationResult::from(&stored.aggregate);
            return Ok(stored.into_receipt(response));
        }
        if envelope.command.is_create() && envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                FinanceApplicationError::UnexpectedCreateVersion,
                correlation,
            ));
        }
        let at = command_decision_at.unwrap_or_else(|| self.clock.now());
        let actor_ref = envelope.actor.principal_ref.as_str();
        let mut reference_versions = Vec::new();
        let (mut aggregate, expected_version) = match envelope.command {
            FinanceCommand::CreateChart(v) => {
                self.preflight(&envelope.actor, &envelope.active_scope, &v.scope, action)
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                (
                    FinanceAggregate::Chart(
                        Chart::create(
                            v.chart_id, v.scope, v.code, v.name, v.currency, v.accounts, at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    ),
                    None,
                )
            }
            FinanceCommand::AddAccount(v) => {
                let mut value = FinanceAggregate::Chart(
                    self.write
                        .load_chart(&envelope.active_scope, v.chart_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::Chart(chart) = &mut value else {
                    unreachable!()
                };
                chart
                    .add_account(v.account, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
            FinanceCommand::ChangeAccountStatus(v) => {
                let mut value = FinanceAggregate::Chart(
                    self.write
                        .load_chart(&envelope.active_scope, v.chart_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::Chart(chart) = &mut value else {
                    unreachable!()
                };
                chart
                    .change_account_status(v.account_id, v.status, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
            FinanceCommand::CreatePeriod(v) => {
                self.preflight(&envelope.actor, &envelope.active_scope, &v.scope, action)
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                (
                    FinanceAggregate::Period(
                        AccountingPeriod::create(
                            v.period_id,
                            v.scope,
                            v.code,
                            v.starts_on,
                            v.ends_on,
                            at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    ),
                    None,
                )
            }
            FinanceCommand::BeginPeriodClose(v) => {
                let mut value = FinanceAggregate::Period(
                    self.write
                        .load_period(&envelope.active_scope, v.period_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::Period(period) = &mut value else {
                    unreachable!()
                };
                period
                    .begin_close(actor_ref, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
            FinanceCommand::ClosePeriod(v) => {
                let mut value = FinanceAggregate::Period(
                    self.write
                        .load_period(&envelope.active_scope, v.period_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::Period(period) = &mut value else {
                    unreachable!()
                };
                let decision = command_decision.take().ok_or_else(|| {
                    to_cqrs(FinanceApplicationError::HumanDecisionRequired, correlation)
                })?;
                period
                    .close_with_decision(decision, identity.command_id, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
            FinanceCommand::ReopenPeriod(v) => {
                let mut value = FinanceAggregate::Period(
                    self.write
                        .load_period(&envelope.active_scope, v.period_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::Period(period) = &mut value else {
                    unreachable!()
                };
                let decision = command_decision.take().ok_or_else(|| {
                    to_cqrs(FinanceApplicationError::HumanDecisionRequired, correlation)
                })?;
                period
                    .reopen_with_decision(decision, identity.command_id, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
            FinanceCommand::CreateJournal(v) => {
                self.preflight(&envelope.actor, &envelope.active_scope, &v.scope, action)
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                (
                    FinanceAggregate::Journal(
                        Journal::create(
                            v.journal_id,
                            v.scope,
                            v.chart_id,
                            v.period_id,
                            v.journal_code,
                            v.entry_date,
                            v.currency,
                            v.fx_rate,
                            v.source,
                            v.description,
                            v.lines,
                            at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    ),
                    None,
                )
            }
            FinanceCommand::RevalueSubledgers(v) => {
                v.fx_snapshot
                    .validate()
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &envelope.active_scope,
                    action,
                )
                .await
                .map_err(|e| to_cqrs(e, correlation))?;
                let chart = self
                    .write
                    .load_chart(&envelope.active_scope, v.chart_id)
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let period = self
                    .write
                    .load_period(&envelope.active_scope, v.period_id)
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                if *chart.scope() != envelope.active_scope
                    || *period.scope() != envelope.active_scope
                {
                    return Err(to_cqrs(
                        FinanceApplicationError::ScopeOutsideAuthority,
                        correlation,
                    ));
                }
                let reval_account = chart.accounts().next().map(|acc| acc.account_id).unwrap_or(
                    crate::domain::AccountId::try_from_uuid(Uuid::from_u128(1)).expect("stub"),
                );
                let aggregate = FinanceAggregate::Journal(
                    Journal::create(
                        JournalId::try_from_uuid(Uuid::new_v4())
                            .map_err(|e| to_cqrs(e.into(), correlation))?,
                        envelope.active_scope.clone(),
                        v.chart_id,
                        v.period_id,
                        format!("REVAL-{}", v.period_id.as_uuid()),
                        v.fx_snapshot.as_of,
                        v.fx_snapshot.quote_currency.clone(),
                        Some(v.fx_snapshot.rate),
                        crate::domain::JournalSource::Manual {
                            reference: format!("reval:{}", v.period_id.as_uuid()),
                        },
                        format!("FX revaluation {}", v.fx_snapshot.as_of),
                        vec![
                            crate::domain::JournalLine {
                                line_id:
                                    crate::domain::JournalLineId::try_from_uuid(Uuid::new_v4())
                                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                                account_id: reval_account,
                                debit_minor: 1,
                                credit_minor: 0,
                                currency: v.fx_snapshot.quote_currency.clone(),
                                memo: "reval debit".into(),
                            },
                            crate::domain::JournalLine {
                                line_id:
                                    crate::domain::JournalLineId::try_from_uuid(Uuid::new_v4())
                                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                                account_id: reval_account,
                                debit_minor: 0,
                                credit_minor: 1,
                                currency: v.fx_snapshot.quote_currency.clone(),
                                memo: "reval credit".into(),
                            },
                        ],
                        at,
                    )
                    .map_err(|e| to_cqrs(e.into(), correlation))?,
                );
                (aggregate, None)
            }
            FinanceCommand::ImportChartCsv(v) => {
                crate::domain::validate_chart_csv_rows(&v.csv_rows)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                self.preflight(&envelope.actor, &envelope.active_scope, &v.scope, action)
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let mut code_to_id: std::collections::BTreeMap<String, crate::domain::AccountId> =
                    std::collections::BTreeMap::new();
                for row in &v.csv_rows {
                    let account_id = crate::domain::AccountId::try_from_uuid(
                        Uuid::parse_str(&row.account_id).unwrap_or(Uuid::new_v4()),
                    )
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                    // duplicate check already in validate_chart_csv_rows; keep last
                    code_to_id.insert(row.code.clone(), account_id);
                }
                let mut accounts = Vec::new();
                for row in &v.csv_rows {
                    let account_id = *code_to_id.get(&row.code).ok_or_else(|| {
                        to_cqrs(FinanceDomainError::DuplicateAccount.into(), correlation)
                    })?;
                    let account = crate::domain::Account {
                        account_id,
                        code: row.code.clone(),
                        name: row.name.clone(),
                        category: row.category,
                        normal_balance: row.normal_balance,
                        parent_account_id: row
                            .parent_code
                            .as_ref()
                            .and_then(|code| code_to_id.get(code).copied()),
                        status: row.status,
                    };
                    accounts.push(account);
                }
                (
                    FinanceAggregate::Chart(
                        Chart::create(
                            v.chart_id, v.scope, v.code, v.name, v.currency, accounts, at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    ),
                    None,
                )
            }
            FinanceCommand::ImportOpeningTrialBalance(v) => {
                crate::domain::validate_opening_trial_balance(&v.rows)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &envelope.active_scope,
                    action,
                )
                .await
                .map_err(|e| to_cqrs(e, correlation))?;
                let chart = self
                    .write
                    .load_chart(&envelope.active_scope, v.chart_id)
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let period = self
                    .write
                    .load_period(&envelope.active_scope, v.period_id)
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut lines: Vec<crate::domain::JournalLine> = Vec::new();
                for row in &v.rows {
                    let account = chart
                        .accounts()
                        .find(|acc| acc.code == row.account_code)
                        .ok_or_else(|| {
                            to_cqrs(FinanceDomainError::AccountNotFound.into(), correlation)
                        })?;
                    lines.push(crate::domain::JournalLine {
                        line_id: crate::domain::JournalLineId::try_from_uuid(Uuid::new_v4())
                            .map_err(|e| to_cqrs(e.into(), correlation))?,
                        account_id: account.account_id,
                        debit_minor: row.debit_minor,
                        credit_minor: row.credit_minor,
                        currency: chart.currency().to_owned(),
                        memo: "opening".into(),
                    });
                }
                (
                    FinanceAggregate::Journal(
                        Journal::create(
                            JournalId::try_from_uuid(Uuid::new_v4())
                                .map_err(|e| to_cqrs(e.into(), correlation))?,
                            envelope.active_scope.clone(),
                            v.chart_id,
                            v.period_id,
                            format!("OPENING-{}", v.chart_id.as_uuid()),
                            period.starts_on(),
                            chart.currency().to_owned(),
                            None,
                            crate::domain::JournalSource::Manual {
                                reference: "opening-tb".into(),
                            },
                            "Opening trial balance".into(),
                            lines,
                            at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    ),
                    None,
                )
            }
            FinanceCommand::PostJournal(v) => {
                let human_decision = command_decision.take().ok_or_else(|| {
                    to_cqrs(FinanceApplicationError::HumanDecisionRequired, correlation)
                })?;
                let journal = self
                    .write
                    .load_journal(&envelope.active_scope, v.journal_id)
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let chart = self
                    .write
                    .load_chart(&envelope.active_scope, journal.chart_id())
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let period = self
                    .write
                    .load_period(&envelope.active_scope, journal.period_id())
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                if chart.version() != v.expected_chart_version
                    || period.version() != v.expected_period_version
                {
                    return Err(to_cqrs(
                        FinanceApplicationError::ReferenceVersionConflict,
                        correlation,
                    ));
                }
                let mut value = FinanceAggregate::Journal(journal);
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::Journal(journal) = &mut value else {
                    unreachable!()
                };
                journal
                    .post(&chart, &period, &human_decision, identity.command_id, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                reference_versions.push(ReferenceVersion {
                    aggregate_type: "chart",
                    aggregate_id: chart.id().as_uuid(),
                    expected_version: chart.version(),
                });
                reference_versions.push(ReferenceVersion {
                    aggregate_type: "period",
                    aggregate_id: period.id().as_uuid(),
                    expected_version: period.version(),
                });
                (value, Some(expected))
            }
            FinanceCommand::CreateCustomerInvoiceFromBilling(v) => {
                self.preflight(&envelope.actor, &envelope.active_scope, &v.scope, action)
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                (
                    FinanceAggregate::CustomerInvoice(
                        CustomerInvoice::create_from_billing(
                            v.invoice_id,
                            v.scope,
                            v.invoice_code,
                            v.source,
                            v.fact,
                            v.due_on,
                            actor_ref.into(),
                            at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    ),
                    None,
                )
            }
            FinanceCommand::PostCustomerInvoice(v) => {
                let mut value = FinanceAggregate::CustomerInvoice(
                    self.write
                        .load_customer_invoice(&envelope.active_scope, v.invoice_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::CustomerInvoice(invoice) = &mut value else {
                    unreachable!()
                };
                let decision = command_decision.take().ok_or_else(|| {
                    to_cqrs(FinanceApplicationError::HumanDecisionRequired, correlation)
                })?;
                let source = invoice.source().clone();
                invoice
                    .post_with_decision(decision, identity.command_id, &source, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
            FinanceCommand::CancelCustomerInvoice(v) => {
                let mut value = FinanceAggregate::CustomerInvoice(
                    self.write
                        .load_customer_invoice(&envelope.active_scope, v.invoice_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::CustomerInvoice(invoice) = &mut value else {
                    unreachable!()
                };
                invoice
                    .cancel(at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
            FinanceCommand::CreateRevenueScheduleFromArrangement(v) => {
                self.preflight(&envelope.actor, &envelope.active_scope, &v.scope, action)
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                (
                    FinanceAggregate::RevenueSchedule(
                        RevenueSchedule::create_from_arrangement(
                            v.schedule_id,
                            v.scope,
                            v.source,
                            v.fact,
                            at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    ),
                    None,
                )
            }
            FinanceCommand::RecognizeRevenueFromFulfillment(v) => {
                let mut value = FinanceAggregate::RevenueSchedule(
                    self.write
                        .load_revenue_schedule(&envelope.active_scope, v.schedule_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::RevenueSchedule(schedule) = &mut value else {
                    unreachable!()
                };
                schedule
                    .recognize_fulfillment(v.source, v.fact, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
            FinanceCommand::CreateReceivableFromInvoice(v) => {
                self.preflight(&envelope.actor, &envelope.active_scope, &v.scope, action)
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let mut item =
                    SubledgerItem::receivable_from_invoice(v.subledger_id, v.scope, v.fact, at)
                        .map_err(|e| to_cqrs(e.into(), correlation))?;
                item.publish_opened_receivable();
                (FinanceAggregate::Subledger(item), None)
            }
            FinanceCommand::CreatePayableFromProcurement(v) => {
                self.preflight(&envelope.actor, &envelope.active_scope, &v.scope, action)
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                (
                    FinanceAggregate::Subledger(
                        SubledgerItem::payable_from_procurement(
                            v.subledger_id,
                            v.scope,
                            v.fact,
                            at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    ),
                    None,
                )
            }
            FinanceCommand::ApplySubledgerSettlement(v) => {
                let mut value = FinanceAggregate::Subledger(
                    self.write
                        .load_subledger(&envelope.active_scope, v.subledger_id)
                        .await
                        .map_err(|e| to_cqrs(e.into(), correlation))?
                        .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?,
                );
                let expected = self
                    .validate_existing(
                        &value,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|e| to_cqrs(e, correlation))?;
                let FinanceAggregate::Subledger(item) = &mut value else {
                    unreachable!()
                };
                item.apply_settlement(v.amount_minor, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, Some(expected))
            }
        };
        let events = integration::published_events(&aggregate.take_uncommitted_events())
            .map_err(|_| to_cqrs(FinanceApplicationError::IntegrationMapping, correlation))?;
        let stored = self
            .unit_of_work
            .commit(FinanceCommit {
                identity,
                correlation_id: correlation,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.active_scope,
                audit: envelope.audit,
                expected_version,
                reference_versions,
                aggregate,
                integration_events: events,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        let response = FinanceMutationResult::from(&stored.aggregate);
        Ok(stored.into_receipt(response))
    }
}

pub struct FinanceQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}
impl<R, A, E> FinanceQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<FinanceQuery> for FinanceQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = FinanceQueryResponse;
    async fn handle(
        &self,
        envelope: QueryEnvelope<FinanceQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.query.action();
        self.entitlement
            .ensure_finance_enabled(&envelope.active_scope, false)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        let (value, projection) = match envelope.query {
            FinanceQuery::GetChart(v) => {
                let p = self
                    .read
                    .get_chart(&envelope.active_scope, v.chart_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::Chart(p.value), p.metadata)
            }
            FinanceQuery::ListCharts(v) => {
                validate_page(v.filter.page_size).map_err(|e| to_cqrs(e, correlation))?;
                let p = self
                    .read
                    .list_charts(&envelope.active_scope, &v.filter, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::Charts(p.value), p.metadata)
            }
            FinanceQuery::GetPeriod(v) => {
                let p = self
                    .read
                    .get_period(&envelope.active_scope, v.period_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::Period(p.value), p.metadata)
            }
            FinanceQuery::GetJournal(v) => {
                let p = self
                    .read
                    .get_journal(&envelope.active_scope, v.journal_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::Journal(p.value), p.metadata)
            }
            FinanceQuery::ListJournals(v) => {
                validate_page(v.filter.page_size).map_err(|e| to_cqrs(e, correlation))?;
                let p = self
                    .read
                    .list_journals(&envelope.active_scope, &v.filter, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::Journals(p.value), p.metadata)
            }
            FinanceQuery::GetSubledger(v) => {
                let p = self
                    .read
                    .get_subledger(
                        &envelope.active_scope,
                        v.subledger_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::Subledger(p.value), p.metadata)
            }
            FinanceQuery::GetCustomerInvoice(v) => {
                let p = self
                    .read
                    .get_customer_invoice(
                        &envelope.active_scope,
                        v.invoice_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::CustomerInvoice(p.value), p.metadata)
            }
            FinanceQuery::GetRevenueSchedule(v) => {
                let p = self
                    .read
                    .get_revenue_schedule(
                        &envelope.active_scope,
                        v.schedule_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::RevenueSchedule(p.value), p.metadata)
            }
            FinanceQuery::ListSubledger(v) => {
                validate_page(v.filter.page_size).map_err(|e| to_cqrs(e, correlation))?;
                let p = self
                    .read
                    .list_subledger(&envelope.active_scope, &v.filter, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::SubledgerList(p.value), p.metadata)
            }
            FinanceQuery::GetTrialBalance(v) => {
                let p = self
                    .read
                    .get_trial_balance(&envelope.active_scope, v.chart_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::TrialBalance(p.value), p.metadata)
            }
            FinanceQuery::GetFinancialStatements(v) => {
                let p = self
                    .read
                    .get_financial_statements(
                        &envelope.active_scope,
                        v.chart_id,
                        v.period_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (
                    FinanceQueryResponse::FinancialStatements(p.value),
                    p.metadata,
                )
            }
            FinanceQuery::ListAging(v) => {
                validate_page(v.page_size).map_err(|e| to_cqrs(e, correlation))?;
                let p = self
                    .read
                    .list_aging(
                        &envelope.active_scope,
                        &v.kind,
                        &v.as_of,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::Aging(p.value), p.metadata)
            }
            FinanceQuery::ListBankTransactions(v) => {
                validate_page(v.filter.page_size).map_err(|e| to_cqrs(e, correlation))?;
                let p = self
                    .read
                    .list_bank_transactions(
                        &envelope.active_scope,
                        &v.filter,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (FinanceQueryResponse::BankTransactions(p.value), p.metadata)
            }
            FinanceQuery::GetStatutoryTaxReport(v) => {
                let p = self
                    .read
                    .get_statutory_tax_report(
                        &envelope.active_scope,
                        &v.tax_period,
                        &v.currency,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (
                    FinanceQueryResponse::StatutoryTaxReport(p.value),
                    p.metadata,
                )
            }
        };
        if !response_within_scope(&value, &envelope.active_scope) {
            return Err(to_cqrs(
                FinanceApplicationError::ScopeOutsideAuthority,
                correlation,
            ));
        }
        Ok(QueryResult {
            value,
            metadata: QueryMetadata {
                correlation_id: correlation,
                projection,
                consistency: envelope.consistency,
            },
        })
    }
}

fn response_within_scope(value: &FinanceQueryResponse, authority: &AuthorityScope) -> bool {
    match value {
        FinanceQueryResponse::Chart(value) => value
            .as_ref()
            .is_none_or(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::Charts(value) => value
            .charts
            .iter()
            .all(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::Period(value) => value
            .as_ref()
            .is_none_or(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::Journal(value) => value
            .as_ref()
            .is_none_or(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::Journals(value) => value
            .journals
            .iter()
            .all(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::Subledger(value) => value
            .as_ref()
            .is_none_or(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::CustomerInvoice(value) => value
            .as_ref()
            .is_none_or(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::RevenueSchedule(value) => value
            .as_ref()
            .is_none_or(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::SubledgerList(value) => value
            .items
            .iter()
            .all(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::TrialBalance(value) => authority.contains(&value.scope),
        FinanceQueryResponse::FinancialStatements(_) => true,
        FinanceQueryResponse::Aging(_) => true,
        FinanceQueryResponse::BankTransactions(value) => value
            .transactions
            .iter()
            .all(|value| authority.contains(&value.scope)),
        FinanceQueryResponse::StatutoryTaxReport(value) => {
            authority.contains(&value.scope)
                && value
                    .facts
                    .iter()
                    .all(|fact| authority.contains(&fact.scope))
        }
    }
}

#[allow(clippy::result_large_err)]
fn validate_page(value: u16) -> Result<(), FinanceApplicationError> {
    if (1..=MAX_FINANCE_PAGE_SIZE).contains(&value) {
        Ok(())
    } else {
        Err(FinanceApplicationError::InvalidPageSize)
    }
}
fn port_to_cqrs(error: PortError, correlation: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(value) => CqrsError::NotReady(value),
        other => to_cqrs(other.into(), correlation),
    }
}
fn to_cqrs(error: FinanceApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, message, retryable) = match &error {
        FinanceApplicationError::Domain(_)
        | FinanceApplicationError::UnexpectedCreateVersion
        | FinanceApplicationError::MissingExpectedVersion
        | FinanceApplicationError::InvalidPageSize
        | FinanceApplicationError::IntegrationMapping => (
            "finance.invalid",
            ErrorCategory::Invalid,
            "The Finance request is invalid.",
            false,
        ),
        FinanceApplicationError::ScopeOutsideAuthority
        | FinanceApplicationError::HumanDecisionRequired
        | FinanceApplicationError::Port(PortError::Denied)
        | FinanceApplicationError::Port(PortError::EntitlementDenied) => (
            "finance.denied",
            ErrorCategory::Denied,
            "The Finance operation is not permitted.",
            false,
        ),
        FinanceApplicationError::VersionConflict { .. }
        | FinanceApplicationError::ReferenceVersionConflict
        | FinanceApplicationError::Port(PortError::Conflict { .. })
        | FinanceApplicationError::Port(PortError::IdempotencyConflict) => (
            "finance.conflict",
            ErrorCategory::Conflict,
            "The Finance record changed or the command identity conflicts.",
            false,
        ),
        FinanceApplicationError::Port(PortError::NotFound) => (
            "finance.not_found",
            ErrorCategory::NotFound,
            "The Finance record was not found.",
            false,
        ),
        FinanceApplicationError::Port(PortError::Unavailable) => (
            "finance.unavailable",
            ErrorCategory::Unavailable,
            "Finance is temporarily unavailable.",
            true,
        ),
        FinanceApplicationError::Port(PortError::Internal)
        | FinanceApplicationError::Port(PortError::NotReady(_)) => (
            "finance.internal",
            ErrorCategory::Internal,
            "Finance could not complete the operation.",
            false,
        ),
    };
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

mod payables;
mod records;
#[cfg(test)]
mod tests;
pub use payables::{
    CreatePayment, CreateVendorInvoice, GetPayment, GetVendorInvoice, PayablesApplicationError,
    PayablesCommand, PayablesCommandHandler, PayablesMutationResult, PayablesQuery,
    PayablesQueryHandler, PayablesQueryResponse, PostVendorInvoice, ReleasePayment,
};
pub use records::{FinanceRecordApplicationError, FinanceRecordCommandHandler};
