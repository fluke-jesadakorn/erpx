use erpx_integration_contracts::{MoneyAmount, ReferenceCode, commercial, customer};

use crate::domain::{DomainEvent, SubscriptionStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidIdentifier,
    InvalidCapability,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<commercial::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<commercial::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::CatalogPlanCreated | DomainEvent::CatalogPlanPublished => None,
        DomainEvent::SubscriptionChanged {
            subscription_id,
            customer_id,
            plan_id,
            status,
            amount_minor,
            currency,
            effective_at,
        } => Some(
            commercial::SubscriptionRef::try_from_uuid(subscription_id.as_uuid())
                .and_then(|subscription| {
                    customer::CustomerRef::try_from_uuid(*customer_id)
                        .map(|customer| (subscription, customer))
                })
                .and_then(|(subscription, customer)| {
                    commercial::PlanRef::try_from_uuid(plan_id.as_uuid())
                        .map(|plan| (subscription, customer, plan))
                })
                .map(|(subscription, customer, plan)| {
                    commercial::Event::SubscriptionChanged(commercial::SubscriptionFact {
                        subscription,
                        customer,
                        plan,
                        status: map_subscription_status(*status),
                        recurring_amount: MoneyAmount::new(*amount_minor, *currency),
                        effective_at: *effective_at,
                    })
                })
                .map_err(|_| IntegrationMappingError::InvalidIdentifier),
        ),
        DomainEvent::EntitlementChanged {
            entitlement_id,
            subscription_id,
            capability,
            enabled,
        } => Some(
            commercial::EntitlementRef::try_from_uuid(entitlement_id.as_uuid())
                .and_then(|entitlement| {
                    commercial::SubscriptionRef::try_from_uuid(subscription_id.as_uuid())
                        .map(|subscription| (entitlement, subscription))
                })
                .map_err(|_| IntegrationMappingError::InvalidIdentifier)
                .and_then(|(entitlement, subscription)| {
                    ReferenceCode::try_new(capability.clone())
                        .map_err(|_| IntegrationMappingError::InvalidCapability)
                        .map(|capability| {
                            commercial::Event::EntitlementChanged(commercial::EntitlementFact {
                                entitlement,
                                subscription,
                                capability,
                                enabled: *enabled,
                            })
                        })
                }),
        ),
    }
}

const fn map_subscription_status(value: SubscriptionStatus) -> commercial::SubscriptionStatus {
    match value {
        SubscriptionStatus::Trial => commercial::SubscriptionStatus::Trial,
        SubscriptionStatus::Active => commercial::SubscriptionStatus::Active,
        SubscriptionStatus::PastDue => commercial::SubscriptionStatus::PastDue,
        SubscriptionStatus::Suspended => commercial::SubscriptionStatus::Suspended,
        SubscriptionStatus::Ended => commercial::SubscriptionStatus::Ended,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{CurrencyCode, commercial::Event};
    use uuid::Uuid;

    use super::*;
    use crate::domain::{CatalogPlanId, DomainEvent, SubscriptionId};

    #[test]
    fn subscription_fact_uses_owner_contract_types() {
        let events = published_events(&[DomainEvent::SubscriptionChanged {
            subscription_id: SubscriptionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            customer_id: Uuid::from_u128(2),
            plan_id: CatalogPlanId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            status: SubscriptionStatus::Active,
            amount_minor: 50_000,
            currency: CurrencyCode::try_new("THB").unwrap(),
            effective_at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        }])
        .unwrap();
        assert!(
            matches!(events.as_slice(), [Event::SubscriptionChanged(fact)] if fact.status == erpx_integration_contracts::commercial::SubscriptionStatus::Active)
        );
    }
}
