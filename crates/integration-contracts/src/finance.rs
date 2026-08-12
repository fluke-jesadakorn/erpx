use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    BoundedContext, DimensionSet, MoneyAmount, RecordRef, ReferenceCode, customer::CustomerRef,
    marketing::CampaignRef, sales,
};

owned_uuid_reference!(JournalRef);
owned_uuid_reference!(ReceivableRef);
owned_uuid_reference!(PayableRef);
owned_uuid_reference!(CustomerInvoiceRef);
owned_uuid_reference!(RevenueScheduleRef);
owned_uuid_reference!(CashReceiptRef);
owned_uuid_reference!(MarketingSpendRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalPostingFact {
    pub journal: JournalRef,
    pub journal_code: ReferenceCode,
    pub source_context: BoundedContext,
    pub source_reference: ReferenceCode,
    pub debit_total: MoneyAmount,
    pub credit_total: MoneyAmount,
    pub posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivableFact {
    pub receivable: ReceivableRef,
    pub customer: CustomerRef,
    pub document_code: ReferenceCode,
    pub outstanding_amount: MoneyAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerInvoicePostedFact {
    pub invoice: CustomerInvoiceRef,
    pub customer: CustomerRef,
    pub invoice_code: ReferenceCode,
    pub source: RecordRef,
    pub net_amount: MoneyAmount,
    pub tax_amount: MoneyAmount,
    pub receivable_amount: MoneyAmount,
    pub dimensions: DimensionSet,
    pub posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognizedRevenueFact {
    pub schedule: RevenueScheduleRef,
    pub customer: CustomerRef,
    pub source: RecordRef,
    pub fulfillment: sales::FulfillmentRef,
    pub obligation: sales::PerformanceObligationRef,
    pub recognized_amount: MoneyAmount,
    pub dimensions: DimensionSet,
    pub recognized_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashCollectionFact {
    pub receipt: CashReceiptRef,
    pub customer: CustomerRef,
    pub invoice: Option<CustomerInvoiceRef>,
    pub collected_amount: MoneyAmount,
    pub dimensions: DimensionSet,
    pub collected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingSpendStage {
    Accrued,
    Invoiced,
    Paid,
    PostedExpense,
    Prepaid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingSpendFact {
    pub spend: MarketingSpendRef,
    pub campaign: CampaignRef,
    pub source: RecordRef,
    pub stage: MarketingSpendStage,
    pub amount: MoneyAmount,
    pub dimensions: DimensionSet,
    pub posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    JournalPosted(JournalPostingFact),
    ReceivableChanged(ReceivableFact),
    CustomerInvoicePosted(CustomerInvoicePostedFact),
    RevenueRecognized(RecognizedRevenueFact),
    CashApplied(CashCollectionFact),
    MarketingSpendPosted(MarketingSpendFact),
}

impl_integration_event!(Event, BoundedContext::Finance, {
    Self::JournalPosted(_) => "finance.journal_posted",
    Self::ReceivableChanged(_) => "finance.receivable_changed",
    Self::CustomerInvoicePosted(_) => "finance.customer_invoice_posted",
    Self::RevenueRecognized(_) => "finance.revenue_recognized",
    Self::CashApplied(_) => "finance.cash_applied",
    Self::MarketingSpendPosted(_) => "finance.marketing_spend_posted",
});
