//! Typed, storage-agnostic gRPC inbound adapter for Finance CQRS.
use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_finance::{
    Account, AccountCategory, AccountId, AccountStatus, AddAccount, ApplySubledgerSettlement,
    BankTransactionFact, BeginPeriodClose, ChangeAccountStatus, ChartId, ClosePeriod, CreateChart,
    CreateJournal, CreatePayment, CreatePeriod, CreateVendorInvoice, FinanceCommand,
    FinanceMutationResult, FinanceQuery, FinanceQueryResponse, GetChart, GetJournal, GetPayment,
    GetPeriod, GetStatutoryTaxReport, GetSubledger, GetTrialBalance, GetVendorInvoice, JournalId,
    JournalLine, JournalLineId, JournalSource, JournalStatus, ListBankTransactions, ListCharts,
    ListJournals, ListSubledger, NormalBalance, PayableView, PayablesCommand,
    PayablesMutationResult, PayablesQuery, PayablesQueryResponse, PaymentAllocation, PaymentId,
    PaymentStatus, PaymentView, PeriodId, PeriodStatus, PostJournal, PostVendorInvoice,
    ReleasePayment, ReopenPeriod, StatutoryTaxFact, StatutoryTaxKind, SubledgerId, SubledgerStatus,
    ThreeWayMatchTolerance, VendorInvoiceId, VendorInvoiceLine, VendorInvoiceLineId,
    VendorInvoiceStatus, VendorInvoiceView,
    ports::{
        BankTransactionListFilter, ChartListFilter, ChartView, JournalView, PeriodView,
        SubledgerListFilter, SubledgerView,
    },
};
use erpx_integration_contracts::{
    BoundedContext, CurrencyCode, MoneyAmount, Quantity, RecordRef, procurement,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, SessionId,
};
use erpx_proto::{common, erpx::finance as platform};
use platform::finance_command_service_server::FinanceCommandService;
use platform::finance_payables_command_service_server::FinancePayablesCommandService;
use platform::finance_payables_query_service_server::FinancePayablesQueryService;
use platform::finance_query_service_server::FinanceQueryService;
use std::sync::Arc;
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;
const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceHoldingRoute {
    pub holding_id: String,
    pub data_profile_key: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceAccessMode {
    Read,
    Write,
}
pub struct FinanceAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    presented_route: FinanceHoldingRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: FinanceAccessMode,
}
impl FinanceAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &AuthorityScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &FinanceHoldingRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> FinanceAccessMode {
        self.access_mode
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedFinanceRequest {
    actor: ActorAttribution,
    scope: AuthorityScope,
    route: FinanceHoldingRoute,
}
impl AuthenticatedFinanceRequest {
    pub fn try_new(
        actor: ActorAttribution,
        scope: AuthorityScope,
        route: FinanceHoldingRoute,
    ) -> Result<Self, FinanceAuthenticationError> {
        actor
            .validate()
            .map_err(|_| FinanceAuthenticationError::Internal)?;
        scope
            .validate()
            .map_err(|_| FinanceAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| FinanceAuthenticationError::Internal)?;
        validate_route_scope(&route, &scope).map_err(|_| FinanceAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn active_scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn route(&self) -> &FinanceHoldingRoute {
        &self.route
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}
#[async_trait]
pub trait FinanceRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: FinanceAuthenticationRequest,
    ) -> Result<AuthenticatedFinanceRequest, FinanceAuthenticationError>;
}
#[async_trait]
pub trait FinanceCommandApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: FinanceHoldingRoute,
        command: CommandEnvelope<FinanceCommand>,
    ) -> Result<CommandReceipt<FinanceMutationResult>, CqrsError>;
}
#[async_trait]
pub trait FinanceQueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: FinanceHoldingRoute,
        query: QueryEnvelope<FinanceQuery>,
    ) -> Result<QueryResult<FinanceQueryResponse>, CqrsError>;
}
#[async_trait]
pub trait FinancePayablesCommandApplication: Send + Sync + 'static {
    async fn execute_payables_command(
        &self,
        route: FinanceHoldingRoute,
        command: CommandEnvelope<PayablesCommand>,
    ) -> Result<CommandReceipt<PayablesMutationResult>, CqrsError>;
}
#[async_trait]
pub trait FinancePayablesQueryApplication: Send + Sync + 'static {
    async fn execute_payables_query(
        &self,
        route: FinanceHoldingRoute,
        query: QueryEnvelope<PayablesQuery>,
    ) -> Result<QueryResult<PayablesQueryResponse>, CqrsError>;
}
#[derive(Debug, Clone)]
pub struct FinanceCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> FinanceCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct FinanceQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> FinanceQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinancePayablesCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> FinancePayablesCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinancePayablesQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> FinancePayablesQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A: FinancePayablesCommandApplication, G: FinanceRequestAuthenticator>
    FinancePayablesCommandService for FinancePayablesCommandRpc<A, G>
{
    async fn create_vendor_invoice(
        &self,
        request: Request<platform::CreateFinanceVendorInvoiceRequest>,
    ) -> Result<Response<platform::FinanceVendorInvoiceMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = PayablesCommand::CreateVendorInvoice(CreateVendorInvoice {
            vendor_invoice_id: vendor_invoice_id(&input.vendor_invoice_id)?,
            scope: decode_scope(
                input
                    .target_scope
                    .ok_or_else(|| invalid("target_scope is required"))?,
            )?,
            invoice_number: input.invoice_number,
            supplier: supplier_ref(&input.supplier_id)?,
            purchase_order: decode_procurement_record_ref(
                input
                    .purchase_order
                    .ok_or_else(|| invalid("purchase_order is required"))?,
                "purchase_order",
            )?,
            receipt: decode_procurement_record_ref(
                input
                    .receipt
                    .ok_or_else(|| invalid("receipt is required"))?,
                "receipt",
            )?,
            tolerance: decode_match_tolerance(
                input
                    .tolerance
                    .ok_or_else(|| invalid("tolerance is required"))?,
            )?,
            lines: input
                .lines
                .into_iter()
                .map(decode_vendor_invoice_line)
                .collect::<Result<_, _>>()?,
            tax_amount: decode_money(
                input
                    .tax_amount
                    .ok_or_else(|| invalid("tax_amount is required"))?,
            )?,
            due_on: date(&input.due_on, "due_on")?,
        });
        let receipt = self.command(transport, input.metadata, command).await?;
        vendor_invoice_mutation_response(receipt)
    }

    async fn post_vendor_invoice(
        &self,
        request: Request<platform::PostFinanceVendorInvoiceRequest>,
    ) -> Result<Response<platform::FinanceVendorInvoiceMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = PayablesCommand::PostVendorInvoice(PostVendorInvoice {
            vendor_invoice_id: vendor_invoice_id(&input.vendor_invoice_id)?,
            purchase_order: decode_procurement_record_ref(
                input
                    .purchase_order
                    .ok_or_else(|| invalid("purchase_order is required"))?,
                "purchase_order",
            )?,
            receipt: decode_procurement_record_ref(
                input
                    .receipt
                    .ok_or_else(|| invalid("receipt is required"))?,
                "receipt",
            )?,
        });
        let receipt = self.command(transport, input.metadata, command).await?;
        vendor_invoice_mutation_response(receipt)
    }

    async fn create_payment(
        &self,
        request: Request<platform::CreateFinancePaymentRequest>,
    ) -> Result<Response<platform::FinancePaymentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = PayablesCommand::CreatePayment(CreatePayment {
            payment_id: payment_id(&input.payment_id)?,
            scope: decode_scope(
                input
                    .target_scope
                    .ok_or_else(|| invalid("target_scope is required"))?,
            )?,
            payment_code: input.payment_code,
            supplier: supplier_ref(&input.supplier_id)?,
            amount: decode_money(input.amount.ok_or_else(|| invalid("amount is required"))?)?,
            allocations: input
                .allocations
                .into_iter()
                .map(decode_payment_allocation)
                .collect::<Result<_, _>>()?,
        });
        let receipt = self.command(transport, input.metadata, command).await?;
        payment_mutation_response(receipt)
    }

    async fn release_payment(
        &self,
        request: Request<platform::ReleaseFinancePaymentRequest>,
    ) -> Result<Response<platform::FinancePaymentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = PayablesCommand::ReleasePayment(ReleasePayment {
            payment_id: payment_id(&input.payment_id)?,
        });
        let receipt = self.command(transport, input.metadata, command).await?;
        payment_mutation_response(receipt)
    }
}

impl<A: FinancePayablesCommandApplication, G: FinanceRequestAuthenticator>
    FinancePayablesCommandRpc<A, G>
{
    async fn command(
        &self,
        transport: MetadataMap,
        metadata: Option<common::CommandRequestMetadata>,
        command: PayablesCommand,
    ) -> Result<CommandReceipt<PayablesMutationResult>, Status> {
        let decoded = decode_command(metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute_payables_command(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

#[async_trait]
impl<A: FinancePayablesQueryApplication, G: FinanceRequestAuthenticator> FinancePayablesQueryService
    for FinancePayablesQueryRpc<A, G>
{
    async fn get_vendor_invoice(
        &self,
        request: Request<platform::GetFinanceVendorInvoiceRequest>,
    ) -> Result<Response<platform::GetFinanceVendorInvoiceResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PayablesQuery::GetVendorInvoice(GetVendorInvoice {
                    vendor_invoice_id: vendor_invoice_id(&input.vendor_invoice_id)?,
                }),
            )
            .await?;
        let PayablesQueryResponse::VendorInvoice {
            vendor_invoice,
            payable,
        } = result.value
        else {
            return Err(Status::internal("Finance payables response type mismatch"));
        };
        Ok(Response::new(platform::GetFinanceVendorInvoiceResponse {
            vendor_invoice: vendor_invoice.as_ref().map(encode_vendor_invoice),
            payable: payable.as_ref().map(encode_payable),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn get_payment(
        &self,
        request: Request<platform::GetFinancePaymentRequest>,
    ) -> Result<Response<platform::GetFinancePaymentResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PayablesQuery::GetPayment(GetPayment {
                    payment_id: payment_id(&input.payment_id)?,
                }),
            )
            .await?;
        let PayablesQueryResponse::Payment(payment) = result.value else {
            return Err(Status::internal("Finance payables response type mismatch"));
        };
        Ok(Response::new(platform::GetFinancePaymentResponse {
            payment: payment.as_ref().map(encode_payment),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

impl<A: FinancePayablesQueryApplication, G: FinanceRequestAuthenticator>
    FinancePayablesQueryRpc<A, G>
{
    async fn query(
        &self,
        transport: MetadataMap,
        metadata: Option<common::QueryRequestMetadata>,
        query: PayablesQuery,
    ) -> Result<QueryResult<PayablesQueryResponse>, Status> {
        let decoded = decode_query(metadata, query)?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute_payables_query(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

#[async_trait]
impl<A: FinanceCommandApplication, G: FinanceRequestAuthenticator> FinanceCommandService
    for FinanceCommandRpc<A, G>
{
    async fn create_chart(
        &self,
        r: Request<platform::CreateFinanceChartRequest>,
    ) -> Result<Response<platform::FinanceChartMutationResponse>, Status> {
        let transport = r.metadata().clone();
        let i = r.into_inner();
        let command = FinanceCommand::CreateChart(CreateChart {
            chart_id: chart_id(&i.chart_id)?,
            scope: decode_scope(
                i.target_scope
                    .ok_or_else(|| invalid("target_scope is required"))?,
            )?,
            code: i.code,
            name: i.name,
            currency: i.currency,
            accounts: i
                .accounts
                .into_iter()
                .map(decode_account)
                .collect::<Result<_, _>>()?,
        });
        chart_response(self.command(transport, i.metadata, command).await?)
    }
    async fn add_account(
        &self,
        r: Request<platform::AddFinanceAccountRequest>,
    ) -> Result<Response<platform::FinanceChartMutationResponse>, Status> {
        let transport = r.metadata().clone();
        let i = r.into_inner();
        let command = FinanceCommand::AddAccount(AddAccount {
            chart_id: chart_id(&i.chart_id)?,
            account: decode_account(i.account.ok_or_else(|| invalid("account is required"))?)?,
        });
        chart_response(self.command(transport, i.metadata, command).await?)
    }
    async fn change_account_status(
        &self,
        r: Request<platform::ChangeFinanceAccountStatusRequest>,
    ) -> Result<Response<platform::FinanceChartMutationResponse>, Status> {
        let transport = r.metadata().clone();
        let i = r.into_inner();
        let command = FinanceCommand::ChangeAccountStatus(ChangeAccountStatus {
            chart_id: chart_id(&i.chart_id)?,
            account_id: account_id(&i.account_id)?,
            status: decode_account_status(i.status)?,
        });
        chart_response(self.command(transport, i.metadata, command).await?)
    }
    async fn create_period(
        &self,
        r: Request<platform::CreateFinancePeriodRequest>,
    ) -> Result<Response<platform::FinancePeriodMutationResponse>, Status> {
        let transport = r.metadata().clone();
        let i = r.into_inner();
        let command = FinanceCommand::CreatePeriod(CreatePeriod {
            period_id: period_id(&i.period_id)?,
            scope: decode_scope(
                i.target_scope
                    .ok_or_else(|| invalid("target_scope is required"))?,
            )?,
            code: i.code,
            starts_on: date(&i.starts_on, "starts_on")?,
            ends_on: date(&i.ends_on, "ends_on")?,
        });
        period_response(self.command(transport, i.metadata, command).await?)
    }
    async fn begin_period_close(
        &self,
        r: Request<platform::FinancePeriodIdentityRequest>,
    ) -> Result<Response<platform::FinancePeriodMutationResponse>, Status> {
        self.period_identity(r, |period_id| {
            FinanceCommand::BeginPeriodClose(BeginPeriodClose { period_id })
        })
        .await
    }
    async fn close_period(
        &self,
        r: Request<platform::FinancePeriodIdentityRequest>,
    ) -> Result<Response<platform::FinancePeriodMutationResponse>, Status> {
        self.period_identity(r, |period_id| {
            FinanceCommand::ClosePeriod(ClosePeriod { period_id })
        })
        .await
    }
    async fn reopen_period(
        &self,
        r: Request<platform::FinancePeriodIdentityRequest>,
    ) -> Result<Response<platform::FinancePeriodMutationResponse>, Status> {
        self.period_identity(r, |period_id| {
            FinanceCommand::ReopenPeriod(ReopenPeriod { period_id })
        })
        .await
    }
    async fn create_journal(
        &self,
        r: Request<platform::CreateTypedFinanceJournalRequest>,
    ) -> Result<Response<platform::FinanceJournalMutationResponse>, Status> {
        let transport = r.metadata().clone();
        let i = r.into_inner();
        let source = i
            .source
            .and_then(|v| v.source)
            .ok_or_else(|| invalid("journal source is required"))?;
        let platform::finance_journal_source::Source::Manual(source) = source else {
            return Err(invalid(
                "public Finance journal creation accepts manual source only",
            ));
        };
        let command = FinanceCommand::CreateJournal(CreateJournal {
            journal_id: journal_id(&i.journal_id)?,
            scope: decode_scope(
                i.target_scope
                    .ok_or_else(|| invalid("target_scope is required"))?,
            )?,
            chart_id: chart_id(&i.chart_id)?,
            period_id: period_id(&i.period_id)?,
            journal_code: i.journal_code,
            entry_date: date(&i.entry_date, "entry_date")?,
            currency: i.currency,
            source: JournalSource::Manual {
                reference: source.reference,
            },
            description: i.description,
            lines: i
                .lines
                .into_iter()
                .map(decode_line)
                .collect::<Result<_, _>>()?,
        });
        journal_response(self.command(transport, i.metadata, command).await?)
    }
    async fn post_journal(
        &self,
        r: Request<platform::PostTypedFinanceJournalRequest>,
    ) -> Result<Response<platform::FinanceJournalMutationResponse>, Status> {
        let transport = r.metadata().clone();
        let i = r.into_inner();
        journal_response(
            self.command(
                transport,
                i.metadata,
                FinanceCommand::PostJournal(PostJournal {
                    journal_id: journal_id(&i.journal_id)?,
                    expected_chart_version: i.expected_chart_version,
                    expected_period_version: i.expected_period_version,
                }),
            )
            .await?,
        )
    }
    async fn apply_subledger_settlement(
        &self,
        r: Request<platform::ApplyFinanceSubledgerSettlementRequest>,
    ) -> Result<Response<platform::FinanceSubledgerMutationResponse>, Status> {
        let transport = r.metadata().clone();
        let i = r.into_inner();
        subledger_response(
            self.command(
                transport,
                i.metadata,
                FinanceCommand::ApplySubledgerSettlement(ApplySubledgerSettlement {
                    subledger_id: subledger_id(&i.subledger_id)?,
                    amount_minor: i.amount_minor,
                }),
            )
            .await?,
        )
    }
}
impl<A: FinanceCommandApplication, G: FinanceRequestAuthenticator> FinanceCommandRpc<A, G> {
    async fn command(
        &self,
        t: MetadataMap,
        m: Option<common::CommandRequestMetadata>,
        c: FinanceCommand,
    ) -> Result<CommandReceipt<FinanceMutationResult>, Status> {
        let d = decode_command(m, c)?;
        let (route, envelope) = authenticate_command(self.authenticator.as_ref(), t, d).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
    async fn period_identity<F>(
        &self,
        r: Request<platform::FinancePeriodIdentityRequest>,
        make: F,
    ) -> Result<Response<platform::FinancePeriodMutationResponse>, Status>
    where
        F: FnOnce(PeriodId) -> FinanceCommand + Send,
    {
        let t = r.metadata().clone();
        let i = r.into_inner();
        period_response(
            self.command(t, i.metadata, make(period_id(&i.period_id)?))
                .await?,
        )
    }
}
fn chart_response(
    r: CommandReceipt<FinanceMutationResult>,
) -> Result<Response<platform::FinanceChartMutationResponse>, Status> {
    let FinanceMutationResult::Chart(v) = &r.response else {
        return Err(Status::internal("Finance command response mismatch"));
    };
    Ok(Response::new(platform::FinanceChartMutationResponse {
        receipt: Some(encode_receipt(&r)),
        chart: Some(encode_chart(v)),
    }))
}
fn period_response(
    r: CommandReceipt<FinanceMutationResult>,
) -> Result<Response<platform::FinancePeriodMutationResponse>, Status> {
    let FinanceMutationResult::Period(v) = &r.response else {
        return Err(Status::internal("Finance command response mismatch"));
    };
    Ok(Response::new(platform::FinancePeriodMutationResponse {
        receipt: Some(encode_receipt(&r)),
        period: Some(encode_period(v)),
    }))
}
fn journal_response(
    r: CommandReceipt<FinanceMutationResult>,
) -> Result<Response<platform::FinanceJournalMutationResponse>, Status> {
    let FinanceMutationResult::Journal(v) = &r.response else {
        return Err(Status::internal("Finance command response mismatch"));
    };
    Ok(Response::new(platform::FinanceJournalMutationResponse {
        receipt: Some(encode_receipt(&r)),
        journal: Some(encode_journal(v)),
    }))
}
fn subledger_response(
    r: CommandReceipt<FinanceMutationResult>,
) -> Result<Response<platform::FinanceSubledgerMutationResponse>, Status> {
    let FinanceMutationResult::Subledger(v) = &r.response else {
        return Err(Status::internal("Finance command response mismatch"));
    };
    Ok(Response::new(platform::FinanceSubledgerMutationResponse {
        receipt: Some(encode_receipt(&r)),
        subledger: Some(encode_subledger(v)),
    }))
}

#[async_trait]
impl<A: FinanceQueryApplication, G: FinanceRequestAuthenticator> FinanceQueryService
    for FinanceQueryRpc<A, G>
{
    async fn get_chart(
        &self,
        r: Request<platform::GetFinanceChartRecordRequest>,
    ) -> Result<Response<platform::GetFinanceChartRecordResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::GetChart(GetChart {
                    chart_id: chart_id(&i.chart_id)?,
                }),
            )
            .await?;
        let FinanceQueryResponse::Chart(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(platform::GetFinanceChartRecordResponse {
            chart: v.as_ref().map(encode_chart),
            metadata: Some(encode_query_metadata(&x.metadata)),
        }))
    }
    async fn list_charts(
        &self,
        r: Request<platform::ListFinanceChartRecordsRequest>,
    ) -> Result<Response<platform::ListFinanceChartRecordsResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let filter = ChartListFilter {
            company_id: i
                .company_id
                .as_deref()
                .map(|value| parse_uuid(value, "company_id"))
                .transpose()?,
            page_size: u16::try_from(i.page_size).map_err(|_| invalid("page_size is invalid"))?,
            cursor: i.cursor,
        };
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::ListCharts(ListCharts { filter }),
            )
            .await?;
        let FinanceQueryResponse::Charts(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(platform::ListFinanceChartRecordsResponse {
            charts: v.charts.iter().map(encode_chart).collect(),
            next_cursor: v.next_cursor,
            metadata: Some(encode_query_metadata(&x.metadata)),
        }))
    }
    async fn get_period(
        &self,
        r: Request<platform::GetFinancePeriodRecordRequest>,
    ) -> Result<Response<platform::GetFinancePeriodRecordResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::GetPeriod(GetPeriod {
                    period_id: period_id(&i.period_id)?,
                }),
            )
            .await?;
        let FinanceQueryResponse::Period(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(platform::GetFinancePeriodRecordResponse {
            period: v.as_ref().map(encode_period),
            metadata: Some(encode_query_metadata(&x.metadata)),
        }))
    }
    async fn get_journal(
        &self,
        r: Request<platform::GetFinanceJournalRecordRequest>,
    ) -> Result<Response<platform::GetFinanceJournalRecordResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::GetJournal(GetJournal {
                    journal_id: journal_id(&i.journal_id)?,
                }),
            )
            .await?;
        let FinanceQueryResponse::Journal(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(platform::GetFinanceJournalRecordResponse {
            journal: v.as_ref().map(encode_journal),
            metadata: Some(encode_query_metadata(&x.metadata)),
        }))
    }
    async fn list_journals(
        &self,
        r: Request<platform::ListTypedFinanceJournalsRequest>,
    ) -> Result<Response<platform::ListTypedFinanceJournalsResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let filter = erpx_finance::JournalListFilter {
            status: i.status.map(decode_journal_status).transpose()?,
            period_id: i.period_id.as_deref().map(period_id).transpose()?,
            page_size: u16::try_from(i.page_size).map_err(|_| invalid("page_size is invalid"))?,
            cursor: i.cursor,
        };
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::ListJournals(ListJournals { filter }),
            )
            .await?;
        let FinanceQueryResponse::Journals(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(platform::ListTypedFinanceJournalsResponse {
            journals: v.journals.iter().map(encode_journal).collect(),
            next_cursor: v.next_cursor,
            metadata: Some(encode_query_metadata(&x.metadata)),
        }))
    }
    async fn get_subledger(
        &self,
        r: Request<platform::GetFinanceSubledgerRecordRequest>,
    ) -> Result<Response<platform::GetFinanceSubledgerRecordResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::GetSubledger(GetSubledger {
                    subledger_id: subledger_id(&i.subledger_id)?,
                }),
            )
            .await?;
        let FinanceQueryResponse::Subledger(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(platform::GetFinanceSubledgerRecordResponse {
            subledger: v.as_ref().map(encode_subledger),
            metadata: Some(encode_query_metadata(&x.metadata)),
        }))
    }
    async fn list_subledger(
        &self,
        r: Request<platform::ListTypedFinanceSubledgerRequest>,
    ) -> Result<Response<platform::ListTypedFinanceSubledgerResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let filter = SubledgerListFilter {
            status: i.status.map(decode_subledger_status).transpose()?,
            receivable_only: i.receivable_only,
            page_size: u16::try_from(i.page_size).map_err(|_| invalid("page_size is invalid"))?,
            cursor: i.cursor,
        };
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::ListSubledger(ListSubledger { filter }),
            )
            .await?;
        let FinanceQueryResponse::SubledgerList(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(platform::ListTypedFinanceSubledgerResponse {
            items: v.items.iter().map(encode_subledger).collect(),
            next_cursor: v.next_cursor,
            metadata: Some(encode_query_metadata(&x.metadata)),
        }))
    }
    async fn get_trial_balance(
        &self,
        r: Request<platform::GetTypedFinanceTrialBalanceRequest>,
    ) -> Result<Response<platform::GetTypedFinanceTrialBalanceResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::GetTrialBalance(GetTrialBalance {
                    chart_id: chart_id(&i.chart_id)?,
                }),
            )
            .await?;
        let FinanceQueryResponse::TrialBalance(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(
            platform::GetTypedFinanceTrialBalanceResponse {
                trial_balance: Some(platform::FinanceTrialBalanceRecord {
                    scope: Some(encode_scope(&v.scope)),
                    chart_id: v.chart_id.as_uuid().to_string(),
                    currency: v.currency,
                    lines: v
                        .lines
                        .into_iter()
                        .map(|l| platform::FinanceTrialBalanceLineRecord {
                            account_id: l.account_id.as_uuid().to_string(),
                            account_code: l.account_code,
                            account_name: l.account_name,
                            debit_minor: l.debit_minor,
                            credit_minor: l.credit_minor,
                            net_minor: l.net_minor,
                        })
                        .collect(),
                    total_debit_minor: v.total_debit_minor,
                    total_credit_minor: v.total_credit_minor,
                }),
                metadata: Some(encode_query_metadata(&x.metadata)),
            },
        ))
    }
    async fn list_bank_transactions(
        &self,
        r: Request<platform::ListTypedFinanceBankTransactionsRequest>,
    ) -> Result<Response<platform::ListTypedFinanceBankTransactionsResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let filter = BankTransactionListFilter {
            posted_from: i
                .posted_from
                .as_deref()
                .map(|v| date(v, "posted_from"))
                .transpose()?,
            posted_through: i
                .posted_through
                .as_deref()
                .map(|v| date(v, "posted_through"))
                .transpose()?,
            page_size: u16::try_from(i.page_size).map_err(|_| invalid("page_size is invalid"))?,
            cursor: i.cursor,
        };
        let x = self
            .query(
                t,
                i.metadata,
                FinanceQuery::ListBankTransactions(ListBankTransactions { filter }),
            )
            .await?;
        let FinanceQueryResponse::BankTransactions(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(
            platform::ListTypedFinanceBankTransactionsResponse {
                transactions: v
                    .transactions
                    .iter()
                    .map(|x| platform::FinanceBankTransactionRecord {
                        scope: Some(encode_scope(&x.scope)),
                        journal_id: x.journal_id.as_uuid().to_string(),
                        fact: Some(encode_bank_fact(&x.fact)),
                    })
                    .collect(),
                next_cursor: v.next_cursor,
                metadata: Some(encode_query_metadata(&x.metadata)),
            },
        ))
    }
    async fn get_statutory_tax_report(
        &self,
        r: Request<platform::GetTypedFinanceStatutoryTaxReportRequest>,
    ) -> Result<Response<platform::GetTypedFinanceStatutoryTaxReportResponse>, Status> {
        let t = r.metadata().clone();
        let i = r.into_inner();
        let x = self
            .query_with_mfa(
                t,
                i.metadata,
                i.mfa_code,
                FinanceQuery::GetStatutoryTaxReport(GetStatutoryTaxReport {
                    tax_period: i.tax_period,
                    currency: i.currency,
                }),
            )
            .await?;
        let FinanceQueryResponse::StatutoryTaxReport(v) = x.value else {
            return Err(Status::internal("Finance query response mismatch"));
        };
        Ok(Response::new(
            platform::GetTypedFinanceStatutoryTaxReportResponse {
                report: Some(platform::FinanceStatutoryTaxReportRecord {
                    scope: Some(encode_scope(&v.scope)),
                    tax_period: v.tax_period,
                    currency: v.currency,
                    facts: v
                        .facts
                        .iter()
                        .map(|x| platform::FinanceStatutoryTaxFactRecord {
                            scope: Some(encode_scope(&x.scope)),
                            journal_id: x.journal_id.as_uuid().to_string(),
                            fact: Some(encode_tax_fact(&x.fact)),
                        })
                        .collect(),
                    output_vat_minor: v.output_vat_minor,
                    input_vat_minor: v.input_vat_minor,
                    withholding_minor: v.withholding_minor,
                }),
                metadata: Some(encode_query_metadata(&x.metadata)),
            },
        ))
    }
}
impl<A: FinanceQueryApplication, G: FinanceRequestAuthenticator> FinanceQueryRpc<A, G> {
    async fn query(
        &self,
        t: MetadataMap,
        m: Option<common::QueryRequestMetadata>,
        q: FinanceQuery,
    ) -> Result<QueryResult<FinanceQueryResponse>, Status> {
        let d = decode_query(m, q)?;
        let (route, envelope) = authenticate_query(self.authenticator.as_ref(), t, d).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }

    async fn query_with_mfa(
        &self,
        t: MetadataMap,
        m: Option<common::QueryRequestMetadata>,
        mfa_code: Option<String>,
        q: FinanceQuery,
    ) -> Result<QueryResult<FinanceQueryResponse>, Status> {
        let d = decode_query(m, q)?;
        let (route, envelope) =
            authenticate_query_with_mfa(self.authenticator.as_ref(), t, d, mfa_code).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

struct DecodedCommand<C> {
    route: FinanceHoldingRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: FinanceHoldingRoute,
    envelope: QueryEnvelope<Q>,
}
async fn authenticate_command<C, G: FinanceRequestAuthenticator>(
    auth: &G,
    t: MetadataMap,
    d: DecodedCommand<C>,
) -> Result<(FinanceHoldingRoute, CommandEnvelope<C>), Status> {
    let a = auth
        .authenticate(FinanceAuthenticationRequest {
            transport_metadata: t,
            presented_actor: d.envelope.actor.clone(),
            presented_scope: d.envelope.active_scope.clone(),
            presented_route: d.route,
            subscription_id: d.subscription_id,
            mfa_code: d.mfa_code,
            access_mode: FinanceAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut e = d.envelope;
    e.actor = a.actor().clone();
    e.active_scope = a.active_scope().clone();
    Ok((a.route().clone(), e))
}
async fn authenticate_query<Q, G: FinanceRequestAuthenticator>(
    auth: &G,
    t: MetadataMap,
    d: DecodedQuery<Q>,
) -> Result<(FinanceHoldingRoute, QueryEnvelope<Q>), Status> {
    let a = auth
        .authenticate(FinanceAuthenticationRequest {
            transport_metadata: t,
            presented_actor: d.envelope.actor.clone(),
            presented_scope: d.envelope.active_scope.clone(),
            presented_route: d.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: FinanceAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut e = d.envelope;
    e.actor = a.actor().clone();
    e.active_scope = a.active_scope().clone();
    Ok((a.route().clone(), e))
}

async fn authenticate_query_with_mfa<Q, G: FinanceRequestAuthenticator>(
    auth: &G,
    t: MetadataMap,
    d: DecodedQuery<Q>,
    mfa_code: Option<String>,
) -> Result<(FinanceHoldingRoute, QueryEnvelope<Q>), Status> {
    let a = auth
        .authenticate(FinanceAuthenticationRequest {
            transport_metadata: t,
            presented_actor: d.envelope.actor.clone(),
            presented_scope: d.envelope.active_scope.clone(),
            presented_route: d.route,
            subscription_id: None,
            mfa_code,
            access_mode: FinanceAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut e = d.envelope;
    e.actor = a.actor().clone();
    e.active_scope = a.active_scope().clone();
    Ok((a.route().clone(), e))
}
fn decode_command<C>(
    m: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let m = m.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(m.holding_route)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(m.contract_version)
            .map_err(|_| invalid("contract version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&m.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(m.idempotency_key)
            .map_err(|_| invalid("idempotency key is invalid"))?,
        correlation_id: correlation(&m.correlation_id)?,
        causation_id: m.causation_id.as_deref().map(event_id).transpose()?,
        actor: decode_actor(m.actor)?,
        active_scope: decode_scope(
            m.active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(m.audit)?,
        expected_aggregate_version: m.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    Ok(DecodedCommand {
        route,
        envelope,
        subscription_id: clean(m.subscription_id),
        mfa_code: clean(m.mfa_code),
    })
}
fn decode_query<Q>(
    m: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let m = m.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(m.holding_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(m.contract_version)
            .map_err(|_| invalid("contract version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&m.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&m.correlation_id)?,
        actor: decode_actor(m.actor)?,
        active_scope: decode_scope(
            m.active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(m.audit)?,
        consistency: decode_consistency(m.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    Ok(DecodedQuery { route, envelope })
}

fn decode_procurement_record_ref(
    value: platform::FinanceVersionedRecordRef,
    expected_kind: &'static str,
) -> Result<RecordRef, Status> {
    if value.context != "procurement" || value.aggregate_kind != expected_kind {
        return Err(invalid("Finance Procurement record reference is invalid"));
    }
    RecordRef::try_new(
        BoundedContext::Procurement,
        value.aggregate_kind,
        parse_uuid(&value.aggregate_id, "aggregate_id")?,
        value.aggregate_version,
    )
    .map_err(|_| invalid("Finance Procurement record reference is invalid"))
}

fn decode_money(value: platform::FinanceMoneyAmount) -> Result<MoneyAmount, Status> {
    let currency = CurrencyCode::try_new(&value.currency)
        .map_err(|_| invalid("Finance money currency is invalid"))?;
    Ok(MoneyAmount::new(value.minor_units, currency))
}

fn decode_quantity(value: platform::FinanceQuantity) -> Result<Quantity, Status> {
    let scale = u8::try_from(value.scale).map_err(|_| invalid("Finance quantity is invalid"))?;
    Quantity::try_new(value.units, scale).map_err(|_| invalid("Finance quantity is invalid"))
}

fn decode_match_tolerance(
    value: platform::FinanceThreeWayMatchTolerance,
) -> Result<ThreeWayMatchTolerance, Status> {
    let tolerance = ThreeWayMatchTolerance {
        quantity_basis_points: u16::try_from(value.quantity_basis_points)
            .map_err(|_| invalid("quantity tolerance is invalid"))?,
        amount_basis_points: u16::try_from(value.amount_basis_points)
            .map_err(|_| invalid("amount tolerance is invalid"))?,
        absolute_amount_minor: value.absolute_amount_minor,
    };
    tolerance
        .validate()
        .map_err(|_| invalid("three-way match tolerance is invalid"))?;
    Ok(tolerance)
}

fn decode_vendor_invoice_line(
    value: platform::FinanceVendorInvoiceLine,
) -> Result<VendorInvoiceLine, Status> {
    Ok(VendorInvoiceLine {
        line_id: VendorInvoiceLineId::try_from_uuid(parse_uuid(&value.line_id, "line_id")?)
            .map_err(|_| invalid("line_id is invalid"))?,
        purchase_order_line: procurement::PurchaseOrderLineRef::try_from_uuid(parse_uuid(
            &value.purchase_order_line_id,
            "purchase_order_line_id",
        )?)
        .map_err(|_| invalid("purchase_order_line_id is invalid"))?,
        receipt_line_id: parse_uuid(&value.receipt_line_id, "receipt_line_id")?,
        invoiced_quantity: decode_quantity(
            value
                .invoiced_quantity
                .ok_or_else(|| invalid("invoiced_quantity is required"))?,
        )?,
        invoiced_amount: decode_money(
            value
                .invoiced_amount
                .ok_or_else(|| invalid("invoiced_amount is required"))?,
        )?,
    })
}

fn decode_payment_allocation(
    value: platform::FinancePaymentAllocation,
) -> Result<PaymentAllocation, Status> {
    Ok(PaymentAllocation {
        payable_id: erpx_finance::PayableId::try_from_uuid(parse_uuid(
            &value.payable_id,
            "payable_id",
        )?)
        .map_err(|_| invalid("payable_id is invalid"))?,
        payable_version: value.payable_version,
        amount: decode_money(
            value
                .amount
                .ok_or_else(|| invalid("allocation amount is required"))?,
        )?,
    })
}

fn supplier_ref(value: &str) -> Result<procurement::SupplierRef, Status> {
    procurement::SupplierRef::try_from_uuid(parse_uuid(value, "supplier_id")?)
        .map_err(|_| invalid("supplier_id is invalid"))
}

fn vendor_invoice_id(value: &str) -> Result<VendorInvoiceId, Status> {
    VendorInvoiceId::try_from_uuid(parse_uuid(value, "vendor_invoice_id")?)
        .map_err(|_| invalid("vendor_invoice_id is invalid"))
}

fn payment_id(value: &str) -> Result<PaymentId, Status> {
    PaymentId::try_from_uuid(parse_uuid(value, "payment_id")?)
        .map_err(|_| invalid("payment_id is invalid"))
}

fn encode_finance_record_ref(value: &RecordRef) -> platform::FinanceVersionedRecordRef {
    platform::FinanceVersionedRecordRef {
        context: match value.context {
            BoundedContext::Procurement => "procurement".into(),
            _ => "unsupported".into(),
        },
        aggregate_kind: value.aggregate_kind.as_str().into(),
        aggregate_id: value.aggregate_id.to_string(),
        aggregate_version: value.aggregate_version,
    }
}

fn encode_money(value: MoneyAmount) -> platform::FinanceMoneyAmount {
    platform::FinanceMoneyAmount {
        minor_units: value.minor_units(),
        currency: value.currency().as_str().into(),
    }
}

fn encode_quantity(value: Quantity) -> platform::FinanceQuantity {
    platform::FinanceQuantity {
        units: value.units(),
        scale: u32::from(value.scale()),
    }
}

fn encode_match_tolerance(
    value: ThreeWayMatchTolerance,
) -> platform::FinanceThreeWayMatchTolerance {
    platform::FinanceThreeWayMatchTolerance {
        quantity_basis_points: u32::from(value.quantity_basis_points),
        amount_basis_points: u32::from(value.amount_basis_points),
        absolute_amount_minor: value.absolute_amount_minor,
    }
}

fn encode_vendor_invoice_line(value: &VendorInvoiceLine) -> platform::FinanceVendorInvoiceLine {
    platform::FinanceVendorInvoiceLine {
        line_id: value.line_id.as_uuid().to_string(),
        purchase_order_line_id: value.purchase_order_line.as_uuid().to_string(),
        receipt_line_id: value.receipt_line_id.to_string(),
        invoiced_quantity: Some(encode_quantity(value.invoiced_quantity)),
        invoiced_amount: Some(encode_money(value.invoiced_amount)),
    }
}

fn encode_vendor_invoice(value: &VendorInvoiceView) -> platform::FinanceVendorInvoiceRecord {
    platform::FinanceVendorInvoiceRecord {
        vendor_invoice_id: value.vendor_invoice_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        invoice_number: value.invoice_number.clone(),
        supplier_id: value.supplier.as_uuid().to_string(),
        purchase_order: Some(encode_finance_record_ref(&value.purchase_order)),
        receipt: Some(encode_finance_record_ref(&value.receipt)),
        tolerance: Some(encode_match_tolerance(value.tolerance)),
        lines: value.lines.iter().map(encode_vendor_invoice_line).collect(),
        net_amount: Some(encode_money(value.net_amount)),
        tax_amount: Some(encode_money(value.tax_amount)),
        total_amount: Some(encode_money(value.total_amount)),
        payable_id: value.payable_id.map(|id| id.as_uuid().to_string()),
        due_on: value.due_on.to_string(),
        status: match value.status {
            VendorInvoiceStatus::Draft => platform::FinanceVendorInvoiceStatus::Draft as i32,
            VendorInvoiceStatus::Posted => platform::FinanceVendorInvoiceStatus::Posted as i32,
        },
        posted_at: value.posted_at.map(|at| at.to_rfc3339()),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: None,
    }
}

fn encode_payable(value: &PayableView) -> platform::FinancePayableRecord {
    platform::FinancePayableRecord {
        payable_id: value.payable_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        vendor_invoice_id: value.vendor_invoice_id.as_uuid().to_string(),
        vendor_invoice_version: value.vendor_invoice_version,
        supplier_id: value.supplier.as_uuid().to_string(),
        amount: Some(encode_money(value.amount)),
        posted_at: value.posted_at.to_rfc3339(),
    }
}

fn encode_payment_allocation(value: &PaymentAllocation) -> platform::FinancePaymentAllocation {
    platform::FinancePaymentAllocation {
        payable_id: value.payable_id.as_uuid().to_string(),
        payable_version: value.payable_version,
        amount: Some(encode_money(value.amount)),
    }
}

fn encode_payment(value: &PaymentView) -> platform::FinancePaymentRecord {
    platform::FinancePaymentRecord {
        payment_id: value.payment_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        payment_code: value.payment_code.clone(),
        supplier_id: value.supplier.as_uuid().to_string(),
        amount: Some(encode_money(value.amount)),
        allocations: value
            .allocations
            .iter()
            .map(encode_payment_allocation)
            .collect(),
        status: match value.status {
            PaymentStatus::Draft => platform::FinancePaymentStatus::Draft as i32,
            PaymentStatus::Released => platform::FinancePaymentStatus::Released as i32,
        },
        released_at: value.released_at.map(|at| at.to_rfc3339()),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: None,
    }
}

fn vendor_invoice_mutation_response(
    receipt: CommandReceipt<PayablesMutationResult>,
) -> Result<Response<platform::FinanceVendorInvoiceMutationResponse>, Status> {
    let PayablesMutationResult::VendorInvoice(value) = &receipt.response else {
        return Err(Status::internal("Finance payables response type mismatch"));
    };
    let payable = value.posted_payable();
    Ok(Response::new(
        platform::FinanceVendorInvoiceMutationResponse {
            receipt: Some(encode_receipt(&receipt)),
            vendor_invoice: Some(encode_vendor_invoice(value)),
            payable: payable.as_ref().map(encode_payable),
        },
    ))
}

fn payment_mutation_response(
    receipt: CommandReceipt<PayablesMutationResult>,
) -> Result<Response<platform::FinancePaymentMutationResponse>, Status> {
    let PayablesMutationResult::Payment(value) = &receipt.response else {
        return Err(Status::internal("Finance payables response type mismatch"));
    };
    Ok(Response::new(platform::FinancePaymentMutationResponse {
        receipt: Some(encode_receipt(&receipt)),
        payment: Some(encode_payment(value)),
    }))
}

fn decode_account(v: platform::FinanceAccountRecord) -> Result<Account, Status> {
    let a = Account {
        account_id: account_id(&v.account_id)?,
        code: v.code,
        name: v.name,
        category: decode_account_category(v.category)?,
        normal_balance: decode_normal_balance(v.normal_balance)?,
        parent_account_id: v.parent_account_id.as_deref().map(account_id).transpose()?,
        status: decode_account_status(v.status)?,
    };
    a.validate()
        .map_err(|_| invalid("Finance account is invalid"))?;
    Ok(a)
}
fn decode_line(v: platform::FinanceJournalLineRecord) -> Result<JournalLine, Status> {
    let l = JournalLine {
        line_id: JournalLineId::try_from_uuid(parse_uuid(&v.line_id, "line_id")?)
            .map_err(|_| invalid("line_id is invalid"))?,
        account_id: account_id(&v.account_id)?,
        debit_minor: v.debit_minor,
        credit_minor: v.credit_minor,
        currency: v.currency,
        memo: v.memo,
    };
    l.validate()
        .map_err(|_| invalid("Finance journal line is invalid"))?;
    Ok(l)
}
fn encode_chart(v: &ChartView) -> platform::FinanceChartRecord {
    platform::FinanceChartRecord {
        chart_id: v.chart_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        code: v.code.clone(),
        name: v.name.clone(),
        currency: v.currency.clone(),
        accounts: v.accounts.iter().map(encode_account).collect(),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_account(v: &Account) -> platform::FinanceAccountRecord {
    platform::FinanceAccountRecord {
        account_id: v.account_id.as_uuid().to_string(),
        code: v.code.clone(),
        name: v.name.clone(),
        category: encode_account_category(v.category),
        normal_balance: encode_normal_balance(v.normal_balance),
        parent_account_id: v.parent_account_id.map(|x| x.as_uuid().to_string()),
        status: encode_account_status(v.status),
    }
}
fn encode_period(v: &PeriodView) -> platform::FinancePeriodRecord {
    platform::FinancePeriodRecord {
        period_id: v.period_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        code: v.code.clone(),
        starts_on: v.starts_on.to_string(),
        ends_on: v.ends_on.to_string(),
        status: encode_period_status(v.status),
        close_requested_by: v.close_requested_by.clone(),
        closed_by: v.closed_by.clone(),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_journal(v: &JournalView) -> platform::FinanceJournalRecord {
    platform::FinanceJournalRecord {
        journal_id: v.journal_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        chart_id: v.chart_id.as_uuid().to_string(),
        period_id: v.period_id.as_uuid().to_string(),
        journal_code: v.journal_code.clone(),
        entry_date: v.entry_date.to_string(),
        currency: v.currency.clone(),
        manual_source: match &v.source {
            JournalSource::Manual { reference } => Some(platform::FinanceManualJournalSource {
                reference: reference.clone(),
            }),
            _ => None,
        },
        description: v.description.clone(),
        lines: v
            .lines
            .iter()
            .map(|l| platform::FinanceJournalLineRecord {
                line_id: l.line_id.as_uuid().to_string(),
                account_id: l.account_id.as_uuid().to_string(),
                debit_minor: l.debit_minor,
                credit_minor: l.credit_minor,
                currency: l.currency.clone(),
                memo: l.memo.clone(),
            })
            .collect(),
        debit_total_minor: v.debit_total_minor,
        credit_total_minor: v.credit_total_minor,
        status: encode_journal_status(v.status),
        posted_at: v.posted_at.map(|x| x.to_rfc3339()),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_subledger(v: &SubledgerView) -> platform::FinanceSubledgerRecord {
    let (kind, party) = match &v.kind {
        erpx_finance::SubledgerKind::Receivable(x) => (
            platform::FinanceSubledgerKind::Receivable as i32,
            x.as_uuid(),
        ),
        erpx_finance::SubledgerKind::Payable(x) => {
            (platform::FinanceSubledgerKind::Payable as i32, x.as_uuid())
        }
    };
    platform::FinanceSubledgerRecord {
        subledger_id: v.subledger_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        kind,
        party_id: party.to_string(),
        document_code: v.document_code.clone(),
        source_reference: v.source_reference.clone(),
        currency: v.currency.clone(),
        original_minor: v.original_minor,
        outstanding_minor: v.outstanding_minor,
        status: encode_subledger_status(v.status),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_bank_fact(v: &BankTransactionFact) -> platform::FinanceBankTransactionFact {
    platform::FinanceBankTransactionFact {
        import_id: v.import_id.to_string(),
        transaction_id: v.transaction_id.to_string(),
        reference: v.reference.clone(),
        posted_on: v.posted_on.to_string(),
        amount_minor: v.amount_minor,
        currency: v.currency.clone(),
    }
}
fn encode_tax_fact(v: &StatutoryTaxFact) -> platform::FinanceStatutoryTaxFact {
    platform::FinanceStatutoryTaxFact {
        tax_fact_id: v.tax_fact_id.to_string(),
        reference: v.reference.clone(),
        tax_date: v.tax_date.to_string(),
        tax_period: v.tax_period.clone(),
        kind: match v.kind {
            StatutoryTaxKind::OutputVat => platform::FinanceStatutoryTaxKind::OutputVat as i32,
            StatutoryTaxKind::InputVat => platform::FinanceStatutoryTaxKind::InputVat as i32,
            StatutoryTaxKind::Withholding => platform::FinanceStatutoryTaxKind::Withholding as i32,
        },
        currency: v.currency.clone(),
        base_minor: v.base_minor,
        tax_minor: v.tax_minor,
        tax_rate_basis_points: v.tax_rate_basis_points,
        tax_identifier: v.tax_identifier.clone(),
    }
}

macro_rules! decode_enum{($fn:ident,$wire:ty,$domain:ty,{$($wv:ident=>$dv:expr),+})=>{fn $fn(v:i32)->Result<$domain,Status>{match <$wire>::try_from(v).ok(){$(Some(<$wire>::$wv)=>Ok($dv),)+_=>Err(invalid(concat!(stringify!($domain)," is invalid")))}}}}
decode_enum!(decode_account_category,platform::FinanceAccountCategory,AccountCategory,{Asset=>AccountCategory::Asset,Liability=>AccountCategory::Liability,Equity=>AccountCategory::Equity,Revenue=>AccountCategory::Revenue,Expense=>AccountCategory::Expense});
decode_enum!(decode_normal_balance,platform::FinanceNormalBalance,NormalBalance,{Debit=>NormalBalance::Debit,Credit=>NormalBalance::Credit});
decode_enum!(decode_account_status,platform::FinanceAccountStatus,AccountStatus,{Active=>AccountStatus::Active,Inactive=>AccountStatus::Inactive});
decode_enum!(decode_journal_status,platform::FinanceJournalStatus,JournalStatus,{Draft=>JournalStatus::Draft,Posted=>JournalStatus::Posted});
decode_enum!(decode_subledger_status,platform::FinanceSubledgerStatus,SubledgerStatus,{Open=>SubledgerStatus::Open,PartiallySettled=>SubledgerStatus::PartiallySettled,Settled=>SubledgerStatus::Settled});
fn encode_account_category(v: AccountCategory) -> i32 {
    match v {
        AccountCategory::Asset => platform::FinanceAccountCategory::Asset as i32,
        AccountCategory::Liability => platform::FinanceAccountCategory::Liability as i32,
        AccountCategory::Equity => platform::FinanceAccountCategory::Equity as i32,
        AccountCategory::Revenue => platform::FinanceAccountCategory::Revenue as i32,
        AccountCategory::Expense => platform::FinanceAccountCategory::Expense as i32,
    }
}
fn encode_normal_balance(v: NormalBalance) -> i32 {
    match v {
        NormalBalance::Debit => platform::FinanceNormalBalance::Debit as i32,
        NormalBalance::Credit => platform::FinanceNormalBalance::Credit as i32,
    }
}
fn encode_account_status(v: AccountStatus) -> i32 {
    match v {
        AccountStatus::Active => platform::FinanceAccountStatus::Active as i32,
        AccountStatus::Inactive => platform::FinanceAccountStatus::Inactive as i32,
    }
}
fn encode_period_status(v: PeriodStatus) -> i32 {
    match v {
        PeriodStatus::Open => platform::FinancePeriodStatus::Open as i32,
        PeriodStatus::Closing => platform::FinancePeriodStatus::Closing as i32,
        PeriodStatus::Closed => platform::FinancePeriodStatus::Closed as i32,
    }
}
fn encode_journal_status(v: JournalStatus) -> i32 {
    match v {
        JournalStatus::Draft => platform::FinanceJournalStatus::Draft as i32,
        JournalStatus::Posted => platform::FinanceJournalStatus::Posted as i32,
    }
}
fn encode_subledger_status(v: SubledgerStatus) -> i32 {
    match v {
        SubledgerStatus::Open => platform::FinanceSubledgerStatus::Open as i32,
        SubledgerStatus::PartiallySettled => {
            platform::FinanceSubledgerStatus::PartiallySettled as i32
        }
        SubledgerStatus::Settled => platform::FinanceSubledgerStatus::Settled as i32,
    }
}

fn chart_id(v: &str) -> Result<ChartId, Status> {
    ChartId::try_from_uuid(parse_uuid(v, "chart_id")?).map_err(|_| invalid("chart_id is invalid"))
}
fn account_id(v: &str) -> Result<AccountId, Status> {
    AccountId::try_from_uuid(parse_uuid(v, "account_id")?)
        .map_err(|_| invalid("account_id is invalid"))
}
fn period_id(v: &str) -> Result<PeriodId, Status> {
    PeriodId::try_from_uuid(parse_uuid(v, "period_id")?)
        .map_err(|_| invalid("period_id is invalid"))
}
fn journal_id(v: &str) -> Result<JournalId, Status> {
    JournalId::try_from_uuid(parse_uuid(v, "journal_id")?)
        .map_err(|_| invalid("journal_id is invalid"))
}
fn subledger_id(v: &str) -> Result<SubledgerId, Status> {
    SubledgerId::try_from_uuid(parse_uuid(v, "subledger_id")?)
        .map_err(|_| invalid("subledger_id is invalid"))
}
fn date(v: &str, f: &str) -> Result<chrono::NaiveDate, Status> {
    chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d")
        .map_err(|_| invalid(&format!("{f} is invalid")))
}
fn encode_receipt<R>(v: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: v.command_id.to_string(),
        correlation_id: v.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: v.aggregate.context.clone(),
            aggregate_type: v.aggregate.aggregate_type.clone(),
            aggregate_id: v.aggregate.aggregate_id.to_string(),
            version: v.aggregate.version,
        }),
        committed_events: v
            .committed_events
            .iter()
            .map(|p| common::EventPosition {
                event_id: p.event_id.to_string(),
                global_sequence: p.global_sequence,
            })
            .collect(),
        committed_at: v.committed_at.to_rfc3339(),
        replayed: v.replayed,
    }
}
fn encode_query_metadata(v: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: v.correlation_id.to_string(),
        projection: Some(common::ProjectionMetadata {
            checkpoint: v
                .projection
                .checkpoint
                .as_ref()
                .map(|c| common::ProjectionCheckpoint {
                    projection: c.projection.clone(),
                    partition: c.partition.clone(),
                    event_id: c.event_id.to_string(),
                    global_sequence: c.global_sequence,
                    updated_at: c.updated_at.to_rfc3339(),
                }),
            freshness: match v.projection.freshness {
                FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
                FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
                FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
                FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
            },
            lag_millis: v.projection.lag_millis,
        }),
        consistency: Some(encode_consistency(&v.consistency)),
    }
}
fn decode_consistency(v: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match v.and_then(|x| x.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(x)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: x.projection,
                partition: x.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: event_id(&x.event_id)?,
                    global_sequence: x.global_sequence,
                },
                timeout_millis: x.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate-version reads are unsupported"))
        }
    }
}
fn encode_consistency(v: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    common::QueryConsistency {
        requirement: Some(match v {
            QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
            QueryConsistency::ReadYourWrites(x) => {
                let w = common::ReadYourWrites {
                    projection: x.projection.clone(),
                    partition: x.partition.clone(),
                    event_id: x.target.event_id.to_string(),
                    global_sequence: x.target.global_sequence,
                    timeout_millis: x.timeout_millis,
                    ..Default::default()
                };
                Requirement::ReadYourWrites(w)
            }
        }),
    }
}
fn decode_route(v: Option<common::HoldingRoute>) -> Result<FinanceHoldingRoute, Status> {
    let v = v.ok_or_else(|| invalid("holding route is required"))?;
    let r = FinanceHoldingRoute {
        holding_id: v.holding_id.trim().into(),
        data_profile_key: v.data_profile_key.trim().into(),
    };
    validate_route(&r)?;
    Ok(r)
}
fn validate_route(v: &FinanceHoldingRoute) -> Result<(), Status> {
    if [&v.holding_id, &v.data_profile_key]
        .iter()
        .any(|x| x.is_empty() || x.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("holding route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_route_scope(r: &FinanceHoldingRoute, s: &AuthorityScope) -> Result<(), Status> {
    let h = match s {
        AuthorityScope::Platform => return Ok(()),
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&r.data_profile_key, "data_profile_key")? == *h {
        Ok(())
    } else {
        Err(invalid("holding route does not match Finance scope"))
    }
}
fn decode_actor(v: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let v = v.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(v.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::HoldingUser) => ActorKind::HoldingUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: v.principal_ref,
        kind,
        session_id: optional_uuid(&v.session_id)?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session is invalid"))?,
        represented_principal_ref: nonempty(v.represented_principal_ref),
        support_capability_id: optional_uuid(&v.support_capability_id)?,
    })
}
fn decode_audit(v: Option<common::AuditContext>) -> Result<AuditContext, Status> {
    let v = v.ok_or_else(|| invalid("audit is required"))?;
    Ok(AuditContext {
        purpose: v.purpose,
        customer_visible: v.customer_visible,
        module: nonempty(v.module),
    })
}
fn decode_scope(v: common::AuthorityScope) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match v.scope.ok_or_else(|| invalid("scope is required"))? {
        Scope::Platform(_) => Ok(AuthorityScope::Platform),
        Scope::Holding(x) => Ok(AuthorityScope::Holding {
            holding_id: parse_uuid(&x.holding_id, "holding_id")?,
        }),
        Scope::Company(x) => Ok(AuthorityScope::Company {
            holding_id: parse_uuid(&x.holding_id, "holding_id")?,
            company_id: parse_uuid(&x.company_id, "company_id")?,
        }),
        Scope::Branch(x) => Ok(AuthorityScope::Branch {
            holding_id: parse_uuid(&x.holding_id, "holding_id")?,
            company_id: parse_uuid(&x.company_id, "company_id")?,
            branch_id: parse_uuid(&x.branch_id, "branch_id")?,
        }),
        Scope::Subtree(x) => Ok(AuthorityScope::Subtree {
            holding_id: parse_uuid(&x.holding_id, "holding_id")?,
            organization_node_id: parse_uuid(&x.organization_node_id, "organization_node_id")?,
        }),
    }
}
fn encode_scope(v: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match v {
            AuthorityScope::Platform => Scope::Platform(common::PlatformScope {}),
            AuthorityScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                organization_node_id: organization_node_id.to_string(),
            }),
        }),
    }
}
fn parse_uuid(v: &str, f: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(v)
        .map_err(|_| invalid(&format!("{f} is invalid")))
        .and_then(|id| {
            if id.is_nil() {
                Err(invalid(&format!("{f} is invalid")))
            } else {
                Ok(id)
            }
        })
}
fn correlation(v: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(v, "correlation_id")?)
        .map_err(|_| invalid("correlation is invalid"))
}
fn event_id(v: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(v, "event_id")?).map_err(|_| invalid("event is invalid"))
}
fn optional_uuid(v: &str) -> Result<Option<Uuid>, Status> {
    if v.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(v, "optional UUID").map(Some)
    }
}
fn nonempty(v: String) -> Option<String> {
    if v.trim().is_empty() { None } else { Some(v) }
}
fn clean(v: Option<String>) -> Option<String> {
    v.and_then(nonempty)
}
fn status_from_cqrs(v: CqrsError) -> Status {
    match v.category() {
        ErrorCategory::Invalid => Status::invalid_argument(v.to_string()),
        ErrorCategory::Denied => Status::permission_denied(v.to_string()),
        ErrorCategory::NotFound => Status::not_found(v.to_string()),
        ErrorCategory::Conflict => Status::aborted(v.to_string()),
        ErrorCategory::NotReady => Status::failed_precondition(v.to_string()),
        ErrorCategory::Unavailable => Status::unavailable(v.to_string()),
        ErrorCategory::Internal => Status::internal(v.to_string()),
    }
}
fn status_from_auth(v: FinanceAuthenticationError) -> Status {
    match v {
        FinanceAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        FinanceAuthenticationError::Denied => Status::permission_denied("Finance access denied"),
        FinanceAuthenticationError::Unavailable => {
            Status::unavailable("authentication unavailable")
        }
        FinanceAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}
fn invalid(v: &str) -> Status {
    Status::invalid_argument(v.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actor() -> ActorAttribution {
        ActorAttribution {
            principal_ref: "finance-route-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }
    #[test]
    fn route_holding_must_match_scope() {
        let h = Uuid::from_u128(1);
        let s = AuthorityScope::Company {
            holding_id: h,
            company_id: Uuid::from_u128(2),
        };
        assert!(
            AuthenticatedFinanceRequest::try_new(
                actor(),
                s.clone(),
                FinanceHoldingRoute {
                    holding_id: "holding".into(),
                    data_profile_key: h.to_string()
                }
            )
            .is_ok()
        );
        assert!(
            AuthenticatedFinanceRequest::try_new(
                actor(),
                s,
                FinanceHoldingRoute {
                    holding_id: "holding".into(),
                    data_profile_key: Uuid::from_u128(9).to_string()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn public_journal_source_is_manual_only() {
        let source = include_str!("finance_cqrs.rs");
        assert!(source.contains("public Finance journal creation accepts manual source only"));
        assert!(!source.contains(concat!("decode_sales_", "order_fact")));
    }
    #[test]
    fn auth_request_does_not_derive_debug() {
        let source = include_str!("finance_cqrs.rs");
        let declaration = source
            .find("pub struct FinanceAuthenticationRequest")
            .unwrap();
        let attrs = source[..declaration]
            .lines()
            .rev()
            .take_while(|l| l.trim_start().starts_with("#["))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!attrs.contains("Debug"));
    }
}
