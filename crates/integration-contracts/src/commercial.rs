use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, MoneyAmount, ReferenceCode, customer::CustomerRef};

owned_uuid_reference!(SubscriptionRef);
owned_uuid_reference!(EntitlementRef);
owned_uuid_reference!(PlanRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Trial,
    Active,
    PastDue,
    Suspended,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionFact {
    pub subscription: SubscriptionRef,
    pub customer: CustomerRef,
    pub plan: PlanRef,
    pub status: SubscriptionStatus,
    pub recurring_amount: MoneyAmount,
    pub effective_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitlementFact {
    pub entitlement: EntitlementRef,
    pub subscription: SubscriptionRef,
    pub capability: ReferenceCode,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    SubscriptionChanged(SubscriptionFact),
    EntitlementChanged(EntitlementFact),
}

impl_integration_event!(Event, BoundedContext::Commercial, {
    Self::SubscriptionChanged(_) => "commercial.subscription_changed",
    Self::EntitlementChanged(_) => "commercial.entitlement_changed",
});
