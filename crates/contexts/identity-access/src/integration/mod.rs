use erpx_integration_contracts::{DisplayName, ReferenceCode, identity_access};

use crate::domain::{DomainEvent, PrincipalStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    PrincipalIdentifier,
    RoleIdentifier,
    DisplayName,
    RoleCode,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<identity_access::Event>, IntegrationMappingError> {
    events.iter().map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Result<identity_access::Event, IntegrationMappingError> {
    match event {
        DomainEvent::PrincipalAccessChanged {
            principal_id,
            display_name,
            status,
            assigned_roles,
            at,
        } => {
            let _ = at;
            let principal = identity_access::PrincipalRef::try_from_uuid(principal_id.as_uuid())
                .map_err(|_| IntegrationMappingError::PrincipalIdentifier)?;
            let display_name = DisplayName::try_new(display_name.clone())
                .map_err(|_| IntegrationMappingError::DisplayName)?;
            let assigned_roles = assigned_roles
                .iter()
                .map(|role_id| {
                    identity_access::RoleRef::try_from_uuid(role_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::RoleIdentifier)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(identity_access::Event::PrincipalAccessChanged(
                identity_access::PrincipalAccessFact {
                    principal,
                    display_name,
                    status: map_status(*status),
                    assigned_roles,
                },
            ))
        }
        DomainEvent::PrincipalDisabled { principal_id, at } => {
            let _ = at;
            let principal = identity_access::PrincipalRef::try_from_uuid(principal_id.as_uuid())
                .map_err(|_| IntegrationMappingError::PrincipalIdentifier)?;
            Ok(identity_access::Event::PrincipalDisabled { principal })
        }
        DomainEvent::RoleDefinitionChanged { role_id, code, at } => {
            let _ = at;
            let role = identity_access::RoleRef::try_from_uuid(role_id.as_uuid())
                .map_err(|_| IntegrationMappingError::RoleIdentifier)?;
            let code = ReferenceCode::try_new(code.clone())
                .map_err(|_| IntegrationMappingError::RoleCode)?;
            Ok(identity_access::Event::RoleDefinitionChanged { role, code })
        }
    }
}

const fn map_status(status: PrincipalStatus) -> identity_access::PrincipalStatus {
    match status {
        PrincipalStatus::Invited => identity_access::PrincipalStatus::Invited,
        PrincipalStatus::Active => identity_access::PrincipalStatus::Active,
        PrincipalStatus::Suspended => identity_access::PrincipalStatus::Suspended,
        PrincipalStatus::Disabled => identity_access::PrincipalStatus::Disabled,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::identity_access::{Event, PrincipalStatus};
    use uuid::Uuid;

    use super::*;
    use crate::domain::{PrincipalId, RoleId};

    #[test]
    fn principal_fact_uses_owner_contract_without_exposing_private_events() {
        let mapped = published_events(&[DomainEvent::PrincipalAccessChanged {
            principal_id: PrincipalId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            display_name: "Ada Lovelace".into(),
            status: crate::domain::PrincipalStatus::Active,
            assigned_roles: vec![RoleId::try_from_uuid(Uuid::from_u128(2)).unwrap()],
            at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        }])
        .unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [Event::PrincipalAccessChanged(fact)]
                if fact.status == PrincipalStatus::Active && fact.assigned_roles.len() == 1
        ));
    }
}
