use erpx_integration_contracts::{commercial, crm, customer, sales, support};
use serde::{Deserialize, Serialize};

use crate::domain::{
    Customer360DomainError, Customer360ProjectionInput, Customer360ProjectionStream,
    ProjectionPosition,
};

/// Typed owner events accepted by the Customer 360 process manager. Mapping
/// returns only the public facts that carry an unambiguous CustomerRef.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "owner", content = "event", rename_all = "snake_case")]
pub enum Customer360SourceEvent {
    Customer(customer::Event),
    Commercial(commercial::Event),
    Crm(crm::Event),
    Sales(Box<sales::Event>),
    Support(support::Event),
}

impl Customer360SourceEvent {
    pub fn into_fact(self) -> Option<customer::Customer360SourceFact> {
        match self {
            Self::Customer(customer::Event::CustomerProfileChanged(fact)) => {
                Some(customer::Customer360SourceFact::Customer(fact))
            }
            Self::Commercial(commercial::Event::SubscriptionChanged(fact)) => {
                Some(customer::Customer360SourceFact::Commercial(fact))
            }
            Self::Crm(crm::Event::OpportunityChanged(fact)) => {
                Some(customer::Customer360SourceFact::Crm(fact))
            }
            Self::Sales(event) => match *event {
                sales::Event::SalesOrderChanged(fact) => {
                    Some(customer::Customer360SourceFact::Sales(fact))
                }
                sales::Event::QuoteApprovalRequested(_)
                | sales::Event::RevenueArrangementActivated(_)
                | sales::Event::FulfillmentAccepted(_)
                | sales::Event::BillingEligible(_) => None,
            },
            Self::Support(support::Event::SupportCaseChanged(fact)) => {
                Some(customer::Customer360SourceFact::Support(fact))
            }
            Self::Customer(customer::Event::CustomerMerged { .. })
            | Self::Commercial(commercial::Event::EntitlementChanged(_))
            | Self::Crm(crm::Event::LeadAccepted(_))
            | Self::Crm(crm::Event::OpportunityForecastChanged(_))
            | Self::Support(support::Event::IncidentChanged(_)) => None,
        }
    }

    /// Build the typed process-manager input without exposing a generic event
    /// payload. Owner events that cannot be routed by `CustomerRef` are
    /// deliberately acknowledged as non-projecting inputs.
    pub fn into_projection_input(
        self,
        stream: Customer360ProjectionStream,
        position: ProjectionPosition,
    ) -> Result<Option<Customer360ProjectionInput>, Customer360DomainError> {
        self.into_fact()
            .map(|fact| Customer360ProjectionInput::try_new(stream, position, fact))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{ReferenceCode, customer::CustomerAccountRef};
    use uuid::Uuid;

    use super::*;

    #[test]
    fn events_without_a_direct_customer_fact_are_not_guessed_into_a_projection() {
        let merged = Customer360SourceEvent::Customer(customer::Event::CustomerMerged {
            surviving_customer: customer::CustomerRef::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            retired_customer: customer::CustomerRef::try_from_uuid(Uuid::from_u128(2)).unwrap(),
        });
        assert_eq!(merged.into_fact(), None);

        let incident = Customer360SourceEvent::Support(support::Event::IncidentChanged(
            support::SupportIncidentFact {
                incident: support::SupportIncidentRef::try_from_uuid(Uuid::from_u128(3)).unwrap(),
                incident_code: ReferenceCode::try_new("INC-3").unwrap(),
                status: support::SupportIncidentStatus::Investigating,
                severity: ReferenceCode::try_new("high").unwrap(),
                affected_services: vec![],
                affected_customer_accounts: vec![
                    CustomerAccountRef::try_from_uuid(Uuid::from_u128(4)).unwrap(),
                ],
                updated_at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
            },
        ));
        assert_eq!(incident.into_fact(), None);
    }
}
