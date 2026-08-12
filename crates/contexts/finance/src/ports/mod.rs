use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::{finance, procurement, sales};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    Account, AccountId, AccountStatus, AccountingPeriod, Chart, ChartId, CustomerInvoice,
    CustomerInvoiceId, CustomerInvoiceStatus, FinanceAggregate, Journal, JournalId, JournalLine,
    JournalSource, JournalStatus, PeriodId, PeriodStatus, RevenueObligation,
    RevenueRecognitionEntry, RevenueSchedule, RevenueScheduleId, RevenueScheduleStatus,
    SubledgerId, SubledgerItem, SubledgerKind, SubledgerStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceAction {
    CreateChart,
    AddAccount,
    ChangeAccountStatus,
    CreatePeriod,
    BeginPeriodClose,
    ClosePeriod,
    ReopenPeriod,
    CreateJournal,
    PostJournal,
    CreateCustomerInvoice,
    PostCustomerInvoice,
    CancelCustomerInvoice,
    CreateRevenueSchedule,
    RecognizeRevenue,
    CreateSubledger,
    SettleSubledger,
    ViewChart,
    ViewPeriod,
    ViewJournal,
    ViewCustomerInvoice,
    ViewRevenueSchedule,
    ListJournals,
    ViewSubledger,
    ListSubledger,
    ViewTrialBalance,
    ViewBanking,
    ViewStatutoryTax,
}

impl FinanceAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(
            self,
            Self::ViewChart
                | Self::ViewPeriod
                | Self::ViewJournal
                | Self::ViewCustomerInvoice
                | Self::ViewRevenueSchedule
                | Self::ListJournals
                | Self::ViewSubledger
                | Self::ListSubledger
                | Self::ViewTrialBalance
                | Self::ViewBanking
                | Self::ViewStatutoryTax
        )
    }

    pub const fn is_high_risk(self) -> bool {
        matches!(
            self,
            Self::PostJournal
                | Self::PostCustomerInvoice
                | Self::RecognizeRevenue
                | Self::ClosePeriod
                | Self::ReopenPeriod
        )
    }
}

pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: FinanceAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_finance_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateChart {
        chart_id: ChartId,
        scope: TenantScope,
        code: String,
        name: String,
        currency: String,
        accounts: Vec<Account>,
    },
    AddAccount {
        chart_id: ChartId,
        account: Account,
    },
    ChangeAccountStatus {
        chart_id: ChartId,
        account_id: AccountId,
        status: AccountStatus,
    },
    CreatePeriod {
        period_id: PeriodId,
        scope: TenantScope,
        code: String,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
    },
    BeginPeriodClose {
        period_id: PeriodId,
    },
    ClosePeriod {
        period_id: PeriodId,
    },
    ReopenPeriod {
        period_id: PeriodId,
    },
    CreateJournal {
        journal_id: JournalId,
        scope: TenantScope,
        chart_id: ChartId,
        period_id: PeriodId,
        journal_code: String,
        entry_date: NaiveDate,
        currency: String,
        source: JournalSource,
        description: String,
        lines: Vec<JournalLine>,
    },
    PostJournal {
        journal_id: JournalId,
        expected_chart_version: u64,
        expected_period_version: u64,
    },
    CreateCustomerInvoice {
        invoice_id: CustomerInvoiceId,
        scope: TenantScope,
        invoice_code: String,
        source: erpx_integration_contracts::RecordRef,
        fact: sales::BillingEligibilityFact,
        due_on: NaiveDate,
    },
    PostCustomerInvoice {
        invoice_id: CustomerInvoiceId,
    },
    CancelCustomerInvoice {
        invoice_id: CustomerInvoiceId,
    },
    CreateRevenueSchedule {
        schedule_id: RevenueScheduleId,
        scope: TenantScope,
        source: erpx_integration_contracts::RecordRef,
        fact: sales::RevenueArrangementFact,
    },
    RecognizeRevenueFromFulfillment {
        schedule_id: RevenueScheduleId,
        source: erpx_integration_contracts::RecordRef,
        fact: sales::FulfillmentAcceptedFact,
    },
    CreateReceivableFromInvoice {
        subledger_id: SubledgerId,
        scope: TenantScope,
        fact: finance::CustomerInvoicePostedFact,
    },
    CreatePayableFromProcurement {
        subledger_id: SubledgerId,
        scope: TenantScope,
        fact: procurement::ReleasedPurchaseOrderFact,
    },
    ApplySubledgerSettlement {
        subledger_id: SubledgerId,
        amount_minor: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_chart(
        &self,
        authority_scope: &TenantScope,
        id: ChartId,
    ) -> Result<Option<Chart>, PortError>;
    async fn load_period(
        &self,
        authority_scope: &TenantScope,
        id: PeriodId,
    ) -> Result<Option<AccountingPeriod>, PortError>;
    async fn load_journal(
        &self,
        authority_scope: &TenantScope,
        id: JournalId,
    ) -> Result<Option<Journal>, PortError>;
    async fn load_subledger(
        &self,
        authority_scope: &TenantScope,
        id: SubledgerId,
    ) -> Result<Option<SubledgerItem>, PortError>;
    async fn load_customer_invoice(
        &self,
        authority_scope: &TenantScope,
        id: CustomerInvoiceId,
    ) -> Result<Option<CustomerInvoice>, PortError>;
    async fn load_revenue_schedule(
        &self,
        authority_scope: &TenantScope,
        id: RevenueScheduleId,
    ) -> Result<Option<RevenueSchedule>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    /// Immutable reference aggregates that the adapter must fence in the same
    /// transaction before accepting this commit (for example chart + period
    /// while posting a journal).
    pub reference_versions: Vec<ReferenceVersion>,
    pub aggregate: FinanceAggregate,
    pub integration_events: Vec<finance::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceVersion {
    pub aggregate_type: &'static str,
    pub aggregate_id: uuid::Uuid,
    pub expected_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: FinanceAggregate,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "finance".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("validated Finance aggregate identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

/// Atomic snapshot + stable receipt + audit + owner outbox boundary.
#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;
    async fn commit(&self, request: FinanceCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartView {
    pub chart_id: ChartId,
    pub version: u64,
    pub scope: TenantScope,
    pub code: String,
    pub name: String,
    pub currency: String,
    pub accounts: Vec<Account>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Chart> for ChartView {
    fn from(value: &Chart) -> Self {
        Self {
            chart_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            code: value.code().into(),
            name: value.name().into(),
            currency: value.currency().into(),
            accounts: value.accounts().cloned().collect(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodView {
    pub period_id: PeriodId,
    pub version: u64,
    pub scope: TenantScope,
    pub code: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub status: PeriodStatus,
    pub close_requested_by: Option<String>,
    pub closed_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&AccountingPeriod> for PeriodView {
    fn from(value: &AccountingPeriod) -> Self {
        Self {
            period_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            code: value.code().into(),
            starts_on: value.starts_on(),
            ends_on: value.ends_on(),
            status: value.status(),
            close_requested_by: value.close_requested_by().map(str::to_owned),
            closed_by: value.closed_by().map(str::to_owned),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalView {
    pub journal_id: JournalId,
    pub version: u64,
    pub scope: TenantScope,
    pub chart_id: ChartId,
    pub period_id: PeriodId,
    pub journal_code: String,
    pub entry_date: NaiveDate,
    pub currency: String,
    pub source: JournalSource,
    pub description: String,
    pub lines: Vec<JournalLine>,
    pub debit_total_minor: i64,
    pub credit_total_minor: i64,
    pub status: JournalStatus,
    pub posted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Journal> for JournalView {
    fn from(value: &Journal) -> Self {
        Self {
            journal_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            chart_id: value.chart_id(),
            period_id: value.period_id(),
            journal_code: value.journal_code().into(),
            entry_date: value.entry_date(),
            currency: value.currency().into(),
            source: value.source().clone(),
            description: value.description().into(),
            lines: value.lines().to_vec(),
            debit_total_minor: value.debit_total_minor(),
            credit_total_minor: value.credit_total_minor(),
            status: value.status(),
            posted_at: value.posted_at(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubledgerView {
    pub subledger_id: SubledgerId,
    pub version: u64,
    pub scope: TenantScope,
    pub kind: SubledgerKind,
    pub document_code: String,
    pub source_reference: String,
    pub currency: String,
    pub original_minor: i64,
    pub outstanding_minor: i64,
    pub status: SubledgerStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&SubledgerItem> for SubledgerView {
    fn from(value: &SubledgerItem) -> Self {
        Self {
            subledger_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            kind: value.kind().clone(),
            document_code: value.document_code().into(),
            source_reference: value.source_reference().into(),
            currency: value.currency().into(),
            original_minor: value.original_minor(),
            outstanding_minor: value.outstanding_minor(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerInvoiceView {
    pub invoice_id: CustomerInvoiceId,
    pub version: u64,
    pub scope: TenantScope,
    pub invoice_code: String,
    pub source: erpx_integration_contracts::RecordRef,
    pub billing: sales::BillingEligibilityFact,
    pub due_on: NaiveDate,
    pub prepared_by: String,
    pub posted_by: Option<String>,
    pub posted_at: Option<DateTime<Utc>>,
    pub receivable_minor: i64,
    pub status: CustomerInvoiceStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&CustomerInvoice> for CustomerInvoiceView {
    fn from(value: &CustomerInvoice) -> Self {
        Self {
            invoice_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            invoice_code: value.invoice_code().into(),
            source: value.source().clone(),
            billing: value.billing().clone(),
            due_on: value.due_on(),
            prepared_by: value.prepared_by().into(),
            posted_by: value.posted_by().map(str::to_owned),
            posted_at: value.posted_at(),
            receivable_minor: value
                .receivable_minor()
                .expect("validated Customer invoice amount"),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueScheduleView {
    pub schedule_id: RevenueScheduleId,
    pub version: u64,
    pub scope: TenantScope,
    pub arrangement_source: erpx_integration_contracts::RecordRef,
    pub last_source: erpx_integration_contracts::RecordRef,
    pub arrangement: sales::RevenueArrangementFact,
    pub currency: String,
    pub obligations: Vec<RevenueObligation>,
    pub recognitions: Vec<RevenueRecognitionEntry>,
    pub recognized_total_minor: i64,
    pub status: RevenueScheduleStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&RevenueSchedule> for RevenueScheduleView {
    fn from(value: &RevenueSchedule) -> Self {
        Self {
            schedule_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            arrangement_source: value.arrangement_source().clone(),
            last_source: value.last_source().clone(),
            arrangement: value.arrangement().clone(),
            currency: value.currency().into(),
            obligations: value.obligations().to_vec(),
            recognitions: value.recognitions().to_vec(),
            recognized_total_minor: value
                .recognized_total_minor()
                .expect("validated revenue recognition total"),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct JournalListFilter {
    pub status: Option<JournalStatus>,
    pub period_id: Option<PeriodId>,
    pub page_size: u16,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalList {
    pub journals: Vec<JournalView>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubledgerListFilter {
    pub status: Option<SubledgerStatus>,
    pub receivable_only: Option<bool>,
    pub page_size: u16,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubledgerList {
    pub items: Vec<SubledgerView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialBalanceLine {
    pub account_id: AccountId,
    pub account_code: String,
    pub account_name: String,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub net_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialBalanceView {
    pub scope: TenantScope,
    pub chart_id: ChartId,
    pub currency: String,
    pub lines: Vec<TrialBalanceLine>,
    pub total_debit_minor: i64,
    pub total_credit_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankTransactionView {
    pub scope: TenantScope,
    pub journal_id: JournalId,
    pub fact: crate::domain::BankTransactionFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BankTransactionListFilter {
    pub posted_from: Option<NaiveDate>,
    pub posted_through: Option<NaiveDate>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankTransactionList {
    pub transactions: Vec<BankTransactionView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatutoryTaxFactView {
    pub scope: TenantScope,
    pub journal_id: JournalId,
    pub fact: crate::domain::StatutoryTaxFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatutoryTaxReport {
    pub scope: TenantScope,
    pub tax_period: String,
    pub currency: String,
    pub facts: Vec<StatutoryTaxFactView>,
    pub output_vat_minor: i64,
    pub input_vat_minor: i64,
    pub withholding_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-model access only; there is intentionally no write-store fallback.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_chart(
        &self,
        scope: &TenantScope,
        id: ChartId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ChartView>>, PortError>;
    async fn get_period(
        &self,
        scope: &TenantScope,
        id: PeriodId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PeriodView>>, PortError>;
    async fn get_journal(
        &self,
        scope: &TenantScope,
        id: JournalId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<JournalView>>, PortError>;
    async fn list_journals(
        &self,
        scope: &TenantScope,
        filter: &JournalListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<JournalList>, PortError>;
    async fn get_subledger(
        &self,
        scope: &TenantScope,
        id: SubledgerId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SubledgerView>>, PortError>;
    async fn get_customer_invoice(
        &self,
        scope: &TenantScope,
        id: CustomerInvoiceId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CustomerInvoiceView>>, PortError>;
    async fn get_revenue_schedule(
        &self,
        scope: &TenantScope,
        id: RevenueScheduleId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RevenueScheduleView>>, PortError>;
    async fn list_subledger(
        &self,
        scope: &TenantScope,
        filter: &SubledgerListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SubledgerList>, PortError>;
    async fn get_trial_balance(
        &self,
        scope: &TenantScope,
        chart_id: ChartId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<TrialBalanceView>, PortError>;
    async fn list_bank_transactions(
        &self,
        scope: &TenantScope,
        filter: &BankTransactionListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<BankTransactionList>, PortError>;
    async fn get_statutory_tax_report(
        &self,
        scope: &TenantScope,
        tax_period: &str,
        currency: &str,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<StatutoryTaxReport>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Finance operation is denied")]
    Denied,
    #[error("Finance entitlement is not enabled")]
    EntitlementDenied,
    #[error("Finance record was not found")]
    NotFound,
    #[error("Finance aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("Finance command identity was reused with a different fingerprint")]
    IdempotencyConflict,
    #[error("Finance projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("Finance storage is temporarily unavailable")]
    Unavailable,
    #[error("Finance adapter failed")]
    Internal,
}
