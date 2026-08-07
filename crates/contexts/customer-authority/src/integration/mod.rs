use erpx_integration_contracts::{DisplayName, ReferenceCode, customer, organization};

use crate::domain::{CustomerStatus, DomainEvent, PublishedCustomerProfile};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    CustomerIdentifier,
    CustomerCode,
    DisplayName,
    CompanyIdentifier,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<customer::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<customer::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::ProfileChanged { profile, at } => {
            let _ = at;
            Some(profile_fact(profile).map(customer::Event::CustomerProfileChanged))
        }
        DomainEvent::Merged {
            retired_customer_id,
            surviving_customer_id,
            at,
        } => {
            let _ = at;
            Some(
                customer::CustomerRef::try_from_uuid(surviving_customer_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::CustomerIdentifier)
                    .and_then(|surviving_customer| {
                        customer::CustomerRef::try_from_uuid(retired_customer_id.as_uuid())
                            .map_err(|_| IntegrationMappingError::CustomerIdentifier)
                            .map(|retired_customer| customer::Event::CustomerMerged {
                                surviving_customer,
                                retired_customer,
                            })
                    }),
            )
        }
        DomainEvent::AuthorityLinked {
            customer_id,
            scope,
            at,
        }
        | DomainEvent::AuthorityUnlinked {
            customer_id,
            scope,
            at,
        } => {
            // The frozen contract has no customer-authority-link fact yet.
            // Keep these private until the contract owner applies the Phase A
            // delta instead of leaking a domain aggregate across contexts.
            let _ = (customer_id, scope, at);
            None
        }
    }
}

fn profile_fact(
    profile: &PublishedCustomerProfile,
) -> Result<customer::CustomerProfileFact, IntegrationMappingError> {
    let customer = customer::CustomerRef::try_from_uuid(profile.customer_id.as_uuid())
        .map_err(|_| IntegrationMappingError::CustomerIdentifier)?;
    let account = customer::CustomerAccountRef::try_from_uuid(profile.customer_id.as_uuid())
        .map_err(|_| IntegrationMappingError::CustomerIdentifier)?;
    let customer_code = ReferenceCode::try_new(profile.code.as_str())
        .map_err(|_| IntegrationMappingError::CustomerCode)?;
    let display_name = DisplayName::try_new(profile.display_name.clone())
        .map_err(|_| IntegrationMappingError::DisplayName)?;
    let owning_company = organization::CompanyRef::try_from_uuid(profile.owning_company_id)
        .map_err(|_| IntegrationMappingError::CompanyIdentifier)?;
    Ok(customer::CustomerProfileFact {
        customer,
        account,
        customer_code,
        display_name,
        owning_company,
        status: map_status(profile.status),
    })
}

const fn map_status(status: CustomerStatus) -> customer::CustomerStatus {
    match status {
        CustomerStatus::Prospect | CustomerStatus::Onboarding => customer::CustomerStatus::Prospect,
        CustomerStatus::Active => customer::CustomerStatus::Active,
        CustomerStatus::Suspended => customer::CustomerStatus::Blocked,
        CustomerStatus::Churned => customer::CustomerStatus::Closed,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_kernel::TenantScope;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{CustomerAccount, CustomerCode, CustomerId, CustomerIdentity};

    #[test]
    fn customer_profile_maps_to_owner_contract_without_sensitive_identity() {
        let mut aggregate = CustomerAccount::create(
            CustomerId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            CustomerCode::try_new("CUS-1").unwrap(),
            CustomerIdentity::try_new(
                "Acme",
                Some("Acme Legal".into()),
                Some("registration-secret".into()),
                Some("tax-secret".into()),
                "TH",
            )
            .unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        )
        .unwrap();
        let mapped = published_events(&aggregate.take_uncommitted_events()).unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [customer::Event::CustomerProfileChanged(fact)]
                if fact.status == customer::CustomerStatus::Prospect
        ));
        let wire = format!("{mapped:?}");
        assert!(!wire.contains("registration-secret"));
        assert!(!wire.contains("tax-secret"));
    }
}
