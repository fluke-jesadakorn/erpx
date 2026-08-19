use erpx_integration_contracts::{DisplayName, ReferenceCode, platform_administration};

use crate::domain::{DomainEvent, HoldingLifecycleState, PublishedHoldingState};

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
        DomainEvent::HoldingChanged { state, at } => {
            let _ = at;
            Some(holding_fact(state))
        }
        DomainEvent::ConnectorChanged {
            connector_id,
            state,
            at,
        } => {
            let _ = (connector_id, state, at);
            None
        }
        DomainEvent::ConnectorReadinessObserved {
            connector_id,
            ready,
            at,
        } => {
            let _ = (connector_id, ready, at);
            None
        }
        DomainEvent::AdminContextIssued {
            context_id,
            holding_id,
            at,
        }
        | DomainEvent::AdminContextRevoked {
            context_id,
            holding_id,
            at,
        } => {
            let _ = (context_id, holding_id, at);
            None
        }
    }
}

fn holding_fact(
    state: &PublishedHoldingState,
) -> Result<platform_administration::Event, IntegrationMappingError> {
    Ok(platform_administration::Event::HoldingLifecycleChanged(
        platform_administration::HoldingLifecycleFact {
            holding: erpx_kernel::HoldingId::try_from_uuid(state.holding_id.as_uuid())
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
                HoldingLifecycleState::PendingSubscription => {
                    platform_administration::HoldingStatus::PendingSubscription
                }
                HoldingLifecycleState::Provisioning => {
                    platform_administration::HoldingStatus::Provisioning
                }
                HoldingLifecycleState::ProvisioningFailed => {
                    platform_administration::HoldingStatus::ProvisioningFailed
                }
                HoldingLifecycleState::Active => platform_administration::HoldingStatus::Active,
                HoldingLifecycleState::Suspended => {
                    platform_administration::HoldingStatus::Suspended
                }
                HoldingLifecycleState::Recovery => platform_administration::HoldingStatus::Recovery,
                HoldingLifecycleState::Retired => platform_administration::HoldingStatus::Retired,
            },
        },
    ))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use super::*;
    use crate::domain::{EnvironmentId, PlatformHolding, PlatformHoldingId};

    #[test]
    fn holding_registration_maps_to_owner_contract() {
        let mut holding = PlatformHolding::register(
            PlatformHoldingId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            "ACME".into(),
            "Acme".into(),
            EnvironmentId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
            "holding/acme".into(),
            Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        )
        .unwrap();
        let events = published_events(&holding.take_uncommitted_events()).unwrap();
        assert!(
            matches!(events.as_slice(), [platform_administration::Event::HoldingLifecycleChanged(fact)] if fact.status == platform_administration::HoldingStatus::PendingSubscription)
        );
    }
}
