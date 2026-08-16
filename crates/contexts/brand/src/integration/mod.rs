use erpx_integration_contracts::{
    DisplayName, RecordTypeId, ReferenceCode, ReferenceValidationError, StableRecordRef, brand,
    catalog::ContextId,
};
use erpx_kernel::RecordId;
use uuid::Uuid;

use crate::domain::{BrandAssetReference, BrandProfileId, DomainEvent, PublishedBrandState};

/// Stable first-class identity for an immutable Brand object. The identity is
/// content-bound, so replacing an object creates a new asset record while old
/// published/history references remain resolvable.
pub fn asset_record_ref(
    profile_id: BrandProfileId,
    asset: &BrandAssetReference,
) -> Result<StableRecordRef, ReferenceValidationError> {
    let label = format!(
        "brand.asset\0{}\0{}\0{}",
        asset.kind.as_str(),
        asset.object_key,
        asset.sha256
    );
    stable_record_ref(profile_id, label.as_bytes(), "brand.asset")
}

/// Stable first-class identity for the immutable publication at an exact
/// owner revision. A later publication never changes the earlier identity.
pub fn publication_record_ref(
    profile_id: BrandProfileId,
    revision: u64,
) -> Result<StableRecordRef, ReferenceValidationError> {
    if revision == 0 {
        return Err(ReferenceValidationError::ZeroVersion("revision"));
    }
    stable_record_ref(
        profile_id,
        format!("brand.publication\0{revision}").as_bytes(),
        "brand.publication",
    )
}

fn stable_record_ref(
    profile_id: BrandProfileId,
    label: &[u8],
    record_type: &str,
) -> Result<StableRecordRef, ReferenceValidationError> {
    let mut bytes = *Uuid::new_v5(&profile_id.as_uuid(), label).as_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let id = Uuid::from_bytes(bytes);
    StableRecordRef::try_new(
        RecordId::try_from_uuid(id).expect("derived Brand record identity is non-nil"),
        ContextId::Brand,
        RecordTypeId::try_new(record_type)?,
    )
}

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
    use crate::domain::{
        BrandAssetKind, BrandAssetReference, BrandDefinition, BrandProfile, BrandProfileId,
        BrandScope,
    };

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

    #[test]
    fn derived_record_identities_are_stable_and_family_specific() {
        let profile_id = BrandProfileId::try_from_uuid(Uuid::from_u128(10)).unwrap();
        let asset = BrandAssetReference {
            kind: BrandAssetKind::LogoMark,
            object_key: "branding/platform/logo.svg".into(),
            sha256: "a".repeat(64),
            media_type: "image/svg+xml".into(),
            byte_length: 128,
            width: Some(24),
            height: Some(24),
        };
        let first = asset_record_ref(profile_id, &asset).unwrap();
        assert_eq!(first, asset_record_ref(profile_id, &asset).unwrap());
        assert_eq!(first.record_type().as_str(), "brand.asset");
        let publication = publication_record_ref(profile_id, 1).unwrap();
        assert_eq!(publication.record_type().as_str(), "brand.publication");
        assert_ne!(first.id(), publication.id());
        assert!(publication_record_ref(profile_id, 0).is_err());
    }
}
