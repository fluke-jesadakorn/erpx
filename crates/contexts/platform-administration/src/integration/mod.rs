use erpx_integration_contracts::{DisplayName, ReferenceCode, platform_administration};

use crate::domain::{DomainEvent, PublishedTenantState, TenantLifecycleState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    Identifier,
    Code,
    Name,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<platform_administration::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(
    event: &DomainEvent,
) -> Option<Result<platform_administration::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::TenantChanged { state, at } => {
            let _ = at;
            Some(tenant_fact(state))
        }
        DomainEvent::ConnectorChanged {
            connector_id,
            state,
            at,
        } => {
            let _ = (connector_id, state, at);
            None
        }
        DomainEvent::AdminContextIssued {
            context_id,
            tenant_id,
            at,
        }
        | DomainEvent::AdminContextRevoked {
            context_id,
            tenant_id,
            at,
        } => {
            let _ = (context_id, tenant_id, at);
            None
        }
    }
}

fn tenant_fact(
    state: &PublishedTenantState,
) -> Result<platform_administration::Event, IntegrationMappingError> {
    Ok(platform_administration::Event::TenantLifecycleChanged(
        platform_administration::TenantLifecycleFact {
            tenant: platform_administration::TenantRef::try_from_uuid(state.tenant_id.as_uuid())
                .map_err(|_| IntegrationMappingError::Identifier)?,
            code: ReferenceCode::try_new(state.code.clone())
                .map_err(|_| IntegrationMappingError::Code)?,
            name: DisplayName::try_new(state.name.clone())
                .map_err(|_| IntegrationMappingError::Name)?,
            environment: platform_administration::EnvironmentRef::try_from_uuid(
                state.environment_id.as_uuid(),
            )
            .map_err(|_| IntegrationMappingError::Identifier)?,
            status: match state.status {
                TenantLifecycleState::Provisioning => {
                    platform_administration::TenantStatus::Provisioning
                }
                TenantLifecycleState::Active => platform_administration::TenantStatus::Active,
                TenantLifecycleState::Suspended => platform_administration::TenantStatus::Suspended,
                TenantLifecycleState::Recovery => platform_administration::TenantStatus::Recovery,
                TenantLifecycleState::Retired => platform_administration::TenantStatus::Retired,
            },
        },
    ))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use super::*;
    use crate::domain::{EnvironmentId, PlatformTenant, PlatformTenantId};

    #[test]
    fn tenant_registration_maps_to_owner_contract() {
        let mut tenant = PlatformTenant::register(
            PlatformTenantId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            "ACME".into(),
            "Acme".into(),
            EnvironmentId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
            "holding/acme".into(),
            Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        )
        .unwrap();
        let events = published_events(&tenant.take_uncommitted_events()).unwrap();
        assert!(
            matches!(events.as_slice(), [platform_administration::Event::TenantLifecycleChanged(fact)] if fact.status == platform_administration::TenantStatus::Provisioning)
        );
    }
}
