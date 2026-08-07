use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, ReferenceCode, customer::CustomerRef};

owned_uuid_reference!(SupportCaseRef);
owned_uuid_reference!(SupportIncidentRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportCaseStatus {
    Open,
    PendingCustomer,
    PendingInternal,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportCaseFact {
    pub support_case: SupportCaseRef,
    pub case_code: ReferenceCode,
    pub customer: CustomerRef,
    pub status: SupportCaseStatus,
    pub category: ReferenceCode,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportIncidentStatus {
    Investigating,
    Identified,
    Monitoring,
    Resolved,
    PostmortemComplete,
}

/// Public, secret-free incident state for composed operational projections.
/// Timeline entries, broadcasts, approvals, sessions, and capabilities remain
/// private to Support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportIncidentFact {
    pub incident: SupportIncidentRef,
    pub incident_code: ReferenceCode,
    pub status: SupportIncidentStatus,
    pub severity: ReferenceCode,
    pub affected_services: Vec<ReferenceCode>,
    pub affected_customer_accounts: Vec<crate::customer::CustomerAccountRef>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    SupportCaseChanged(SupportCaseFact),
    IncidentChanged(SupportIncidentFact),
}

impl_integration_event!(Event, BoundedContext::Support, {
    Self::SupportCaseChanged(_) => "support.case_changed",
    Self::IncidentChanged(_) => "support.incident_changed",
});
