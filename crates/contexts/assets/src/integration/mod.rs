use erpx_integration_contracts::{DisplayName, ReferenceCode, assets};

use crate::domain::DomainEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    AssetIdentifier,
    LocationIdentifier,
    Reference,
    Name,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<assets::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<assets::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::ConditionChanged {
            asset_id,
            asset_code,
            name,
            location_id,
            condition,
            at,
        } => {
            let _ = at;
            Some(
                assets::AssetRef::try_from_uuid(asset_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::AssetIdentifier)
                    .and_then(|asset| {
                        assets::AssetLocationRef::try_from_uuid(*location_id)
                            .map_err(|_| IntegrationMappingError::LocationIdentifier)
                            .map(|location| (asset, location))
                    })
                    .and_then(|(asset, location)| {
                        ReferenceCode::try_new(asset_code.clone())
                            .map_err(|_| IntegrationMappingError::Reference)
                            .map(|asset_code| (asset, location, asset_code))
                    })
                    .and_then(|(asset, location, asset_code)| {
                        DisplayName::try_new(name.clone())
                            .map_err(|_| IntegrationMappingError::Name)
                            .map(|name| {
                                assets::Event::AssetConditionChanged(assets::AssetConditionFact {
                                    asset,
                                    asset_code,
                                    name,
                                    location,
                                    condition: *condition,
                                })
                            })
                    }),
            )
        }
        DomainEvent::MaintenanceRequested {
            asset_id,
            request_code,
            title,
            at,
        } => {
            let _ = at;
            Some(
                assets::AssetRef::try_from_uuid(asset_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::AssetIdentifier)
                    .and_then(|asset| {
                        ReferenceCode::try_new(request_code.clone())
                            .map_err(|_| IntegrationMappingError::Reference)
                            .and_then(|request_code| {
                                DisplayName::try_new(title.clone())
                                    .map_err(|_| IntegrationMappingError::Name)
                                    .map(|title| assets::Event::MaintenanceRequested {
                                        asset,
                                        request_code,
                                        title: Some(title),
                                    })
                            })
                    }),
            )
        }
        DomainEvent::Created {
            asset_id,
            scope,
            asset_code,
            name,
            location_id,
            at,
        } => {
            let _ = (asset_id, scope, asset_code, name, location_id, at);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::assets::Event;
    use uuid::Uuid;

    use super::*;
    use crate::domain::AssetId;

    #[test]
    fn maintenance_request_maps_to_assets_owned_contract() {
        let mapped = published_events(&[DomainEvent::MaintenanceRequested {
            asset_id: AssetId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            request_code: "REQ-1".into(),
            title: "Replace drive belt".into(),
            at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        }])
        .unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [Event::MaintenanceRequested { asset, request_code, title }]
                if asset.as_uuid() == Uuid::from_u128(1)
                    && request_code.as_str() == "REQ-1"
                    && title.as_ref().is_some_and(|value| value.as_str() == "Replace drive belt")
        ));
    }
}
