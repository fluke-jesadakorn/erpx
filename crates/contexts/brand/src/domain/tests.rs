use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
        .single()
        .unwrap()
}

fn platform_profile() -> BrandProfile {
    BrandProfile::create_draft(
        BrandProfileId::try_from_uuid(id(1)).unwrap(),
        BrandScope::Platform,
        BrandDefinition::neutral_platform(),
        None,
        "brand-admin".into(),
        at(9),
    )
    .unwrap()
}

#[test]
fn checked_revision_rehydration_rejects_invalid_wire_state() {
    let revision = BrandRevision::try_rehydrate(
        &BrandScope::Platform,
        1,
        BrandLifecycle::Published,
        BrandDefinition::neutral_platform(),
        None,
        "brand-reader",
        at(9),
    )
    .unwrap();
    assert_eq!(revision.revision(), 1);
    assert!(
        BrandRevision::try_rehydrate(
            &BrandScope::Platform,
            0,
            BrandLifecycle::Published,
            BrandDefinition::neutral_platform(),
            None,
            "brand-reader",
            at(9),
        )
        .is_err()
    );
}

fn holding_scope() -> BrandScope {
    BrandScope::Holding { holding_id: id(10) }
}

fn holding_parent(locked: impl IntoIterator<Item = BrandCategory>) -> ParentBrandWitness {
    ParentBrandWitness {
        scope: BrandScope::Platform,
        published_revision: 7,
        locked_categories: locked.into_iter().collect(),
    }
}

fn holding_definition() -> BrandDefinition {
    let mut definition = BrandDefinition::neutral_platform();
    definition.display_name = "Acme Holding".into();
    definition.short_name = "ACME".into();
    definition.overridden_categories = [BrandCategory::Identity, BrandCategory::Color]
        .into_iter()
        .collect();
    definition
}

#[test]
fn scope_maps_platform_and_business_unit_to_exact_authority() {
    assert_eq!(
        BrandScope::Platform.authority_scope().unwrap(),
        AuthorityScope::Platform
    );
    let scope = BrandScope::BusinessUnit {
        holding_id: id(10),
        business_unit: BusinessUnitKey::Finance,
    };
    assert_eq!(
        scope.authority_scope().unwrap(),
        AuthorityScope::Holding { holding_id: id(10) }
    );
}

#[test]
fn holding_and_business_unit_require_direct_parent_witnesses() {
    let unit = BrandScope::BusinessUnit {
        holding_id: id(10),
        business_unit: BusinessUnitKey::Finance,
    };
    let wrong_parent = holding_parent([]);
    let result = BrandProfile::create_draft(
        BrandProfileId::try_from_uuid(id(2)).unwrap(),
        unit,
        holding_definition(),
        Some(&wrong_parent),
        "brand-admin".into(),
        at(9),
    );
    assert_eq!(result, Err(BrandDomainError::InvalidParent));
}

#[test]
fn platform_must_define_every_category() {
    let mut definition = BrandDefinition::neutral_platform();
    definition
        .overridden_categories
        .remove(&BrandCategory::Motion);
    let result = BrandProfile::create_draft(
        BrandProfileId::try_from_uuid(id(1)).unwrap(),
        BrandScope::Platform,
        definition,
        None,
        "brand-admin".into(),
        at(9),
    );
    assert_eq!(
        result,
        Err(BrandDomainError::PlatformMustDefineEveryCategory)
    );
}

#[test]
fn asset_reference_is_digest_only_and_bound_to_scope_namespace() {
    let asset = BrandAssetReference {
        kind: BrandAssetKind::LogoMark,
        object_key: format!("branding/holding/{}/logo.svg", id(10)),
        sha256: "a".repeat(64),
        media_type: "image/svg+xml".into(),
        byte_length: 128,
        width: Some(24),
        height: Some(24),
    };
    assert!(asset.validate_for_scope(&holding_scope()).is_ok());
    let mut unsafe_asset = asset;
    unsafe_asset.object_key = "https://evil.invalid/logo.svg".into();
    assert_eq!(
        unsafe_asset.validate_for_scope(&holding_scope()),
        Err(BrandDomainError::InvalidAsset)
    );
}

#[test]
fn low_contrast_is_rejected_at_publication() {
    let mut definition = BrandDefinition::neutral_platform();
    definition.tokens.light.text = definition.tokens.light.canvas.clone();
    let mut profile = BrandProfile::create_draft(
        BrandProfileId::try_from_uuid(id(1)).unwrap(),
        BrandScope::Platform,
        definition,
        None,
        "brand-admin".into(),
        at(9),
    )
    .unwrap();
    assert_eq!(
        profile.publish_draft(None, "brand-admin".into(), at(10)),
        Err(BrandDomainError::InsufficientContrast)
    );
}

#[test]
fn draft_publish_lifecycle_preserves_revision_and_advances_aggregate() {
    let mut profile = platform_profile();
    assert_eq!(profile.version(), 1);
    assert_eq!(profile.draft().unwrap().lifecycle(), BrandLifecycle::Draft);
    profile
        .publish_draft(None, "publisher".into(), at(10))
        .unwrap();
    assert_eq!(profile.version(), 2);
    assert!(profile.draft().is_none());
    assert_eq!(profile.published().unwrap().revision(), 1);
    assert_eq!(
        profile.published().unwrap().lifecycle(),
        BrandLifecycle::Published
    );
}

#[test]
fn publication_rejects_parent_changed_after_draft_save() {
    let initial = holding_parent([]);
    let mut profile = BrandProfile::create_draft(
        BrandProfileId::try_from_uuid(id(2)).unwrap(),
        holding_scope(),
        holding_definition(),
        Some(&initial),
        "brand-admin".into(),
        at(9),
    )
    .unwrap();
    let changed = ParentBrandWitness {
        published_revision: 8,
        ..initial
    };
    assert_eq!(
        profile.publish_draft(Some(&changed), "publisher".into(), at(10)),
        Err(BrandDomainError::StaleParent)
    );
}

#[test]
fn parent_category_lock_prevents_child_override() {
    let parent = holding_parent([BrandCategory::Identity]);
    let result = BrandProfile::create_draft(
        BrandProfileId::try_from_uuid(id(2)).unwrap(),
        holding_scope(),
        holding_definition(),
        Some(&parent),
        "brand-admin".into(),
        at(9),
    );
    assert_eq!(result, Err(BrandDomainError::CategoryLockedByParent));
}

#[test]
fn business_unit_cannot_lock_categories() {
    let scope = BrandScope::BusinessUnit {
        holding_id: id(10),
        business_unit: BusinessUnitKey::Finance,
    };
    let parent = ParentBrandWitness {
        scope: holding_scope(),
        published_revision: 3,
        locked_categories: BTreeSet::new(),
    };
    let mut definition = holding_definition();
    definition.locked_categories.insert(BrandCategory::Color);
    let result = BrandProfile::create_draft(
        BrandProfileId::try_from_uuid(id(3)).unwrap(),
        scope,
        definition,
        Some(&parent),
        "brand-admin".into(),
        at(9),
    );
    assert_eq!(result, Err(BrandDomainError::BusinessUnitCannotLock));
}

#[test]
fn rollback_copies_owned_source_into_a_new_monotonic_revision() {
    let mut profile = platform_profile();
    profile
        .publish_draft(None, "publisher".into(), at(10))
        .unwrap();
    profile
        .set_category_locks(
            [BrandCategory::Color].into_iter().collect(),
            None,
            "locker".into(),
            at(11),
        )
        .unwrap();
    profile
        .rollback(
            ArchivedBrandRevision {
                profile_id: profile.id(),
                scope: BrandScope::Platform,
                source_revision: 1,
                definition: BrandDefinition::neutral_platform(),
            },
            None,
            "publisher".into(),
            at(12),
        )
        .unwrap();
    assert_eq!(profile.published().unwrap().revision(), 3);
    assert!(
        profile
            .published()
            .unwrap()
            .definition()
            .locked_categories
            .is_empty()
    );
}

#[test]
fn rollback_rejects_cross_profile_source() {
    let mut profile = platform_profile();
    profile
        .publish_draft(None, "publisher".into(), at(10))
        .unwrap();
    let result = profile.rollback(
        ArchivedBrandRevision {
            profile_id: BrandProfileId::try_from_uuid(id(999)).unwrap(),
            scope: BrandScope::Platform,
            source_revision: 1,
            definition: BrandDefinition::neutral_platform(),
        },
        None,
        "publisher".into(),
        at(11),
    );
    assert_eq!(result, Err(BrandDomainError::InvalidRollbackSource));
}

#[test]
fn draft_blocks_rollback_and_category_lock_changes() {
    let mut profile = platform_profile();
    let source = ArchivedBrandRevision {
        profile_id: profile.id(),
        scope: BrandScope::Platform,
        source_revision: 1,
        definition: BrandDefinition::neutral_platform(),
    };
    assert_eq!(
        profile.rollback(source, None, "publisher".into(), at(10)),
        Err(BrandDomainError::DraftBlocksRollback)
    );
    assert_eq!(
        profile.set_category_locks(
            [BrandCategory::Identity].into_iter().collect(),
            None,
            "locker".into(),
            at(10),
        ),
        Err(BrandDomainError::DraftBlocksLockChange)
    );
}

#[test]
fn published_hierarchy_resolves_only_explicit_unlocked_overrides() {
    let mut platform = platform_profile();
    platform
        .publish_draft(None, "publisher".into(), at(10))
        .unwrap();
    platform
        .set_category_locks(
            [BrandCategory::Shape].into_iter().collect(),
            None,
            "locker".into(),
            at(11),
        )
        .unwrap();
    let parent = ParentBrandWitness {
        scope: BrandScope::Platform,
        published_revision: platform.published().unwrap().revision(),
        locked_categories: [BrandCategory::Shape].into_iter().collect(),
    };
    let mut definition = holding_definition();
    definition.tokens.radius = BrandRadius::Rounded;
    let mut holding = BrandProfile::create_draft(
        BrandProfileId::try_from_uuid(id(2)).unwrap(),
        holding_scope(),
        definition,
        Some(&parent),
        "brand-admin".into(),
        at(12),
    )
    .unwrap();
    holding
        .publish_draft(Some(&parent), "publisher".into(), at(13))
        .unwrap();

    let resolved = resolve_published_brand(&platform, Some(&holding), None).unwrap();
    assert_eq!(resolved.definition.display_name, "Acme Holding");
    assert_eq!(resolved.definition.tokens.radius, BrandRadius::Soft);
    assert!(
        resolved
            .effective_locked_categories
            .contains(&BrandCategory::Shape)
    );
    assert!(
        resolved
            .inherited_categories
            .contains(&BrandCategory::Shape)
    );
}
