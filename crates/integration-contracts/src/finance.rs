use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, MoneyAmount, ReferenceCode, customer::CustomerRef};

owned_uuid_reference!(JournalRef);
owned_uuid_reference!(ReceivableRef);
owned_uuid_reference!(PayableRef);

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
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    JournalPosted(JournalPostingFact),
    ReceivableChanged(ReceivableFact),
}

impl_integration_event!(Event, BoundedContext::Finance, {
    Self::JournalPosted(_) => "finance.journal_posted",
    Self::ReceivableChanged(_) => "finance.receivable_changed",
});
