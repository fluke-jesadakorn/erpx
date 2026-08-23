use super::{
    models::{FinanceItem, FinanceItems},
    presenter::FinanceContent,
};
use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_finance::{
    CustomerInvoiceStatus, FinanceCommand, FinanceMutationResult, FinanceQuery,
    FinanceQueryResponse, JournalStatus, PayablesCommand, PayablesMutationResult, PayablesQuery,
    PayablesQueryResponse, PaymentStatus, PeriodStatus, RevenueScheduleStatus, SubledgerStatus,
    VendorInvoiceStatus,
    ports::{
        ChartView, CustomerInvoiceView, JournalView, PaymentView, PeriodView, RevenueScheduleView,
        SubledgerView, VendorInvoiceView,
    },
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

pub fn present_mutation(v: &FinanceMutationResult) -> FinanceContent {
    match v {
        FinanceMutationResult::Chart(x) => FinanceContent::Item(chart(x.clone())),
        FinanceMutationResult::Period(x) => FinanceContent::Item(period(x.clone())),
        FinanceMutationResult::Journal(x) => FinanceContent::Item(journal(x.clone())),
        FinanceMutationResult::Subledger(x) => FinanceContent::Item(subledger(x.clone())),
        FinanceMutationResult::CustomerInvoice(x) => {
            FinanceContent::Item(customer_invoice(x.clone()))
        }
        FinanceMutationResult::RevenueSchedule(x) => {
            FinanceContent::Item(revenue_schedule(x.clone()))
        }
    }
}

pub fn present_payables_mutation(v: &PayablesMutationResult) -> FinanceContent {
    match v {
        PayablesMutationResult::VendorInvoice(x) => FinanceContent::Item(vendor_invoice(x.clone())),
        PayablesMutationResult::Payment(x) => FinanceContent::Item(payment(x.clone())),
    }
}
pub fn present_query(v: QueryResult<FinanceQueryResponse>) -> QueryResult<Option<FinanceContent>> {
    let metadata = v.metadata;
    let value = match v.value {
        FinanceQueryResponse::Chart(x) => x.map(chart).map(FinanceContent::Item),
        FinanceQueryResponse::Charts(x) => {
            let items = x.charts.into_iter().map(chart).collect::<Vec<_>>();
            (!items.is_empty()).then_some(FinanceContent::Items(FinanceItems {
                heading: "Charts of accounts".into(),
                items,
                next_cursor: x.next_cursor,
            }))
        }
        FinanceQueryResponse::Period(x) => x.map(period).map(FinanceContent::Item),
        FinanceQueryResponse::Journal(x) => x.map(journal).map(FinanceContent::Item),
        FinanceQueryResponse::Journals(x) => {
            let items = x.journals.into_iter().map(journal).collect::<Vec<_>>();
            (!items.is_empty()).then_some(FinanceContent::Items(FinanceItems {
                heading: "Journals".into(),
                items,
                next_cursor: x.next_cursor,
            }))
        }
        FinanceQueryResponse::Subledger(x) => x.map(subledger).map(FinanceContent::Item),
        FinanceQueryResponse::CustomerInvoice(x) => {
            x.map(customer_invoice).map(FinanceContent::Item)
        }
        FinanceQueryResponse::RevenueSchedule(x) => {
            x.map(revenue_schedule).map(FinanceContent::Item)
        }
        FinanceQueryResponse::SubledgerList(x) => {
            let items = x.items.into_iter().map(subledger).collect::<Vec<_>>();
            (!items.is_empty()).then_some(FinanceContent::Items(FinanceItems {
                heading: "Subledger".into(),
                items,
                next_cursor: x.next_cursor,
            }))
        }
        FinanceQueryResponse::TrialBalance(x) => Some(FinanceContent::Summary {
            heading: "Trial balance".into(),
            amount_minor: x.total_debit_minor,
            currency: x.currency,
        }),
        FinanceQueryResponse::BankTransactions(x) => {
            let items = x
                .transactions
                .into_iter()
                .map(|v| FinanceItem {
                    id: v.fact.transaction_id.to_string(),
                    version: 0,
                    scope: format!("{:?}", v.scope),
                    code: v.fact.reference,
                    kind: "bank_transaction".into(),
                    status: "recorded".into(),
                    currency: Some(v.fact.currency),
                    amount_minor: Some(v.fact.amount_minor),
                    updated_at: Some(v.fact.posted_on.to_string()),
                })
                .collect::<Vec<_>>();
            (!items.is_empty()).then_some(FinanceContent::Items(FinanceItems {
                heading: "Bank transactions".into(),
                items,
                next_cursor: x.next_cursor,
            }))
        }
        FinanceQueryResponse::StatutoryTaxReport(x) => Some(FinanceContent::Summary {
            heading: x.tax_period,
            amount_minor: x.output_vat_minor - x.input_vat_minor + x.withholding_minor,
            currency: x.currency,
        }),
    };
    QueryResult { value, metadata }
}

pub fn present_payables_query(
    v: QueryResult<PayablesQueryResponse>,
) -> QueryResult<Option<FinanceContent>> {
    let metadata = v.metadata;
    let value = match v.value {
        PayablesQueryResponse::VendorInvoice {
            vendor_invoice: view,
            ..
        } => view.map(vendor_invoice).map(FinanceContent::Item),
        PayablesQueryResponse::Payment(view) => view.map(payment).map(FinanceContent::Item),
    };
    QueryResult { value, metadata }
}
fn chart(v: ChartView) -> FinanceItem {
    FinanceItem {
        id: v.chart_id.as_uuid().to_string(),
        version: v.version,
        scope: format!("{:?}", v.scope),
        code: v.code,
        kind: "chart".into(),
        status: "active".into(),
        currency: Some(v.currency),
        amount_minor: None,
        updated_at: Some(v.updated_at.to_rfc3339()),
    }
}
fn period(v: PeriodView) -> FinanceItem {
    FinanceItem {
        id: v.period_id.as_uuid().to_string(),
        version: v.version,
        scope: format!("{:?}", v.scope),
        code: v.code,
        kind: "period".into(),
        status: match v.status {
            PeriodStatus::Open => "open",
            PeriodStatus::Closing => "closing",
            PeriodStatus::Closed => "closed",
        }
        .into(),
        currency: None,
        amount_minor: None,
        updated_at: Some(v.updated_at.to_rfc3339()),
    }
}
fn journal(v: JournalView) -> FinanceItem {
    FinanceItem {
        id: v.journal_id.as_uuid().to_string(),
        version: v.version,
        scope: format!("{:?}", v.scope),
        code: v.journal_code,
        kind: "journal".into(),
        status: match v.status {
            JournalStatus::Draft => "draft",
            JournalStatus::Posted => "posted",
        }
        .into(),
        currency: Some(v.currency),
        amount_minor: Some(v.debit_total_minor),
        updated_at: Some(v.updated_at.to_rfc3339()),
    }
}
fn subledger(v: SubledgerView) -> FinanceItem {
    FinanceItem {
        id: v.subledger_id.as_uuid().to_string(),
        version: v.version,
        scope: format!("{:?}", v.scope),
        code: v.document_code,
        kind: "subledger".into(),
        status: match v.status {
            SubledgerStatus::Open => "open",
            SubledgerStatus::PartiallySettled => "partially_settled",
            SubledgerStatus::Settled => "settled",
        }
        .into(),
        currency: Some(v.currency),
        amount_minor: Some(v.outstanding_minor),
        updated_at: Some(v.updated_at.to_rfc3339()),
    }
}

fn customer_invoice(v: CustomerInvoiceView) -> FinanceItem {
    FinanceItem {
        id: v.invoice_id.as_uuid().to_string(),
        version: v.version,
        scope: format!("{:?}", v.scope),
        code: v.invoice_code,
        kind: "customer_invoice".into(),
        status: match v.status {
            CustomerInvoiceStatus::Draft => "draft",
            CustomerInvoiceStatus::Posted => "posted",
            CustomerInvoiceStatus::Cancelled => "cancelled",
        }
        .into(),
        currency: Some(v.billing.billable_amount.currency().as_str().into()),
        amount_minor: Some(v.receivable_minor),
        updated_at: Some(v.updated_at.to_rfc3339()),
    }
}

fn revenue_schedule(v: RevenueScheduleView) -> FinanceItem {
    FinanceItem {
        id: v.schedule_id.as_uuid().to_string(),
        version: v.version,
        scope: format!("{:?}", v.scope),
        code: v.arrangement.order_code.as_str().into(),
        kind: "revenue_schedule".into(),
        status: match v.status {
            RevenueScheduleStatus::Active => "active",
            RevenueScheduleStatus::Completed => "completed",
        }
        .into(),
        currency: Some(v.currency),
        amount_minor: Some(v.recognized_total_minor),
        updated_at: Some(v.updated_at.to_rfc3339()),
    }
}

fn vendor_invoice(v: VendorInvoiceView) -> FinanceItem {
    FinanceItem {
        id: v.vendor_invoice_id.as_uuid().to_string(),
        version: v.version,
        scope: format!("{:?}", v.scope),
        code: v.invoice_number,
        kind: "vendor_invoice".into(),
        status: match v.status {
            VendorInvoiceStatus::Draft => "draft",
            VendorInvoiceStatus::Posted => "posted",
        }
        .into(),
        currency: Some(v.total_amount.currency().as_str().into()),
        amount_minor: Some(v.total_amount.minor_units()),
        updated_at: Some(v.updated_at.to_rfc3339()),
    }
}

fn payment(v: PaymentView) -> FinanceItem {
    FinanceItem {
        id: v.payment_id.as_uuid().to_string(),
        version: v.version,
        scope: format!("{:?}", v.scope),
        code: v.payment_code,
        kind: "payment".into(),
        status: match v.status {
            PaymentStatus::Draft => "draft",
            PaymentStatus::Released => "released",
        }
        .into(),
        currency: Some(v.amount.currency().as_str().into()),
        amount_minor: Some(v.amount.minor_units()),
        updated_at: Some(v.updated_at.to_rfc3339()),
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait FinanceProtoTransport: Send + Sync {
    async fn command(
        &self,
        envelope: CommandEnvelope<FinanceCommand>,
    ) -> Result<CommandReceipt<FinanceMutationResult>, UiError>;
    async fn query(
        &self,
        envelope: QueryEnvelope<FinanceQuery>,
    ) -> Result<QueryResult<FinanceQueryResponse>, UiError>;
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait FinancePayablesProtoTransport: Send + Sync {
    async fn payables_command(
        &self,
        envelope: CommandEnvelope<PayablesCommand>,
    ) -> Result<CommandReceipt<PayablesMutationResult>, UiError>;
    async fn payables_query(
        &self,
        envelope: QueryEnvelope<PayablesQuery>,
    ) -> Result<QueryResult<PayablesQueryResponse>, UiError>;
}
#[derive(Debug, Clone)]
pub struct FinanceGateway<T> {
    transport: T,
}
impl<T> FinanceGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: FinanceProtoTransport> CommandGateway<FinanceCommand> for FinanceGateway<T> {
    type Response = FinanceMutationResult;
    async fn execute(
        &self,
        e: CommandEnvelope<FinanceCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.command(e).await
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: FinanceProtoTransport> QueryGateway<FinanceQuery> for FinanceGateway<T> {
    type Response = FinanceQueryResponse;
    async fn execute(
        &self,
        e: QueryEnvelope<FinanceQuery>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.query(e).await
    }
}

#[derive(Debug, Clone)]
pub struct FinancePayablesGateway<T> {
    transport: T,
}
impl<T> FinancePayablesGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: FinancePayablesProtoTransport> CommandGateway<PayablesCommand>
    for FinancePayablesGateway<T>
{
    type Response = PayablesMutationResult;
    async fn execute(
        &self,
        e: CommandEnvelope<PayablesCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.payables_command(e).await
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: FinancePayablesProtoTransport> QueryGateway<PayablesQuery> for FinancePayablesGateway<T> {
    type Response = PayablesQueryResponse;
    async fn execute(
        &self,
        e: QueryEnvelope<PayablesQuery>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.payables_query(e).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gateway_is_the_only_core_mapping_boundary() {
        let source = include_str!("gateway.rs");
        assert!(source.contains("erpx_finance"));
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("ton", "ic::")));
    }
}
