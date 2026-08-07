use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, ReferenceCode, customer::CustomerRef};

owned_uuid_reference!(LegalCaseRef);
owned_uuid_reference!(ObligationRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationStatus {
    Open,
    Satisfied,
    Waived,
    Breached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalObligationFact {
    pub obligation: ObligationRef,
    pub legal_case: LegalCaseRef,
    pub customer: Option<CustomerRef>,
    pub obligation_code: ReferenceCode,
    pub title: DisplayName,
    pub status: ObligationStatus,
    pub due_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    ObligationChanged(LegalObligationFact),
}

impl_integration_event!(Event, BoundedContext::Legal, {
    Self::ObligationChanged(_) => "legal.obligation_changed",
});
