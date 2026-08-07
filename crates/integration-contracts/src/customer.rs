use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, ReferenceCode, organization::CompanyRef};

owned_uuid_reference!(CustomerRef);
owned_uuid_reference!(CustomerAccountRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerStatus {
    Prospect,
    Active,
    Dormant,
    Blocked,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerProfileFact {
    pub customer: CustomerRef,
    pub account: CustomerAccountRef,
    pub customer_code: ReferenceCode,
    pub display_name: DisplayName,
    pub owning_company: CompanyRef,
    pub status: CustomerStatus,
}

/// Owner-qualified inputs from which the Customer 360 read model is rebuilt.
/// The enum deliberately contains facts, not foreign aggregates or repositories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "fact", rename_all = "snake_case")]
pub enum Customer360SourceFact {
    Customer(CustomerProfileFact),
    Commercial(crate::commercial::SubscriptionFact),
    Crm(crate::crm::OpportunityFact),
    Sales(crate::sales::SalesOrderFact),
    Support(crate::support::SupportCaseFact),
}

impl Customer360SourceFact {
    pub const fn source_context(&self) -> BoundedContext {
        match self {
            Self::Customer(_) => BoundedContext::Customer,
            Self::Commercial(_) => BoundedContext::Commercial,
            Self::Crm(_) => BoundedContext::Crm,
            Self::Sales(_) => BoundedContext::Sales,
            Self::Support(_) => BoundedContext::Support,
        }
    }

    pub const fn customer(&self) -> CustomerRef {
        match self {
            Self::Customer(fact) => fact.customer,
            Self::Commercial(fact) => fact.customer,
            Self::Crm(fact) => fact.customer,
            Self::Sales(fact) => fact.customer,
            Self::Support(fact) => fact.customer,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    CustomerProfileChanged(CustomerProfileFact),
    CustomerMerged {
        surviving_customer: CustomerRef,
        retired_customer: CustomerRef,
    },
}

impl_integration_event!(Event, BoundedContext::Customer, {
    Self::CustomerProfileChanged(_) => "customer.profile_changed",
    Self::CustomerMerged { .. } => "customer.merged",
});
