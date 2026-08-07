use erpx_integration_contracts::{DisplayName, ReferenceCode, brand};

use crate::domain::{DomainEvent, PublishedBrandState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    Identifier,
    Code,
    DisplayName,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<brand::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<brand::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::DraftSaved => None,
        DomainEvent::Published { state, at } | DomainEvent::CategoryLocksChanged { state, at } => {
            let _ = at;
            Some(fact(state).map(brand::Event::BrandProfileChanged))
        }
        DomainEvent::RolledBack {
            source_revision,
            state,
            at,
        } => {
            let _ = (source_revision, at);
            Some(fact(state).map(brand::Event::BrandProfileChanged))
        }
    }
}

fn fact(state: &PublishedBrandState) -> Result<brand::BrandProfileFact, IntegrationMappingError> {
    let _ = (&state.scope, state.revision);
    Ok(brand::BrandProfileFact {
        brand: brand::BrandRef::try_from_uuid(state.profile_id.as_uuid())
            .map_err(|_| IntegrationMappingError::Identifier)?,
        code: ReferenceCode::try_new(state.profile_id.as_uuid().to_string())
            .map_err(|_| IntegrationMappingError::Code)?,
        display_name: DisplayName::try_new(state.display_name.clone())
            .map_err(|_| IntegrationMappingError::DisplayName)?,
        active: true,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use super::*;
    use crate::domain::{BrandDefinition, BrandProfile, BrandProfileId, BrandScope};

    #[test]
    fn draft_is_private_and_publication_uses_owner_contract() {
        let mut profile = BrandProfile::create_draft(
            BrandProfileId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            BrandScope::Platform,
            BrandDefinition::neutral_platform(),
            None,
            "brand-admin".into(),
            Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        )
        .unwrap();
        assert!(
            published_events(&profile.take_uncommitted_events())
                .unwrap()
                .is_empty()
        );
        profile
            .publish_draft(
                None,
                "brand-admin".into(),
                Utc.with_ymd_and_hms(2026, 8, 5, 10, 0, 0).single().unwrap(),
            )
            .unwrap();
        let events = published_events(&profile.take_uncommitted_events()).unwrap();
        assert!(matches!(
            events.as_slice(),
            [brand::Event::BrandProfileChanged(fact)] if fact.active
                && fact.display_name.as_str() == "ERPX Platform"
        ));
    }
}
