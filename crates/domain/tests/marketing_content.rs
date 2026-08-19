use std::collections::BTreeMap;

use chrono::Utc;
use erpx_domain::contracts::AuthorityScope;
use erpx_domain::marketing_content::{
    ContentAsset, ContentAssetVersion, ContentLocale, ContentTemplate, ContentTemplateVersion,
    MarketingContentError, SeoMetadata, SocialMetadata, TemplateVariant,
};
use uuid::Uuid;

fn scope() -> AuthorityScope {
    AuthorityScope::Holding {
        holding_id: Uuid::from_u128(1),
    }
}

fn version(template_id: Uuid, version: u64, created_by: &str) -> ContentTemplateVersion {
    let mut variants = BTreeMap::new();
    variants.insert(
        ContentLocale::En,
        TemplateVariant::new("English title", "English body").unwrap(),
    );
    variants.insert(
        ContentLocale::Th,
        TemplateVariant::new("ชื่อภาษาไทย", "เนื้อหาภาษาไทย").unwrap(),
    );
    let title = erpx_domain::marketing_content::LocalizedText::new("Title", "ชื่อ").unwrap();
    let description =
        erpx_domain::marketing_content::LocalizedText::new("Description", "คำอธิบาย").unwrap();
    ContentTemplateVersion::new(
        Uuid::new_v4(),
        template_id,
        scope(),
        version,
        variants,
        SeoMetadata::new(
            title.clone(),
            description.clone(),
            "/landing",
            false,
            vec![],
        )
        .unwrap(),
        SocialMetadata::new(title, description, None).unwrap(),
        vec![],
        created_by,
        Utc::now(),
    )
    .unwrap()
}

fn asset_version(asset_id: Uuid, version: u64) -> ContentAssetVersion {
    let asset_scope = scope();
    ContentAssetVersion::new(
        Uuid::new_v4(),
        asset_id,
        asset_scope.clone(),
        version,
        erpx_domain::marketing_content::StorageObjectRef::new(asset_scope, "asset/object").unwrap(),
        "a".repeat(64),
        1,
        "image/png",
        "asset-author",
        Utc::now(),
    )
    .unwrap()
}

#[test]
fn template_version_overflow_fails_closed_without_mutating_publication_state() {
    let template_id = Uuid::from_u128(10);
    let mut template = ContentTemplate::new(
        template_id,
        scope(),
        "launch-page",
        version(template_id, 1, "author"),
    )
    .unwrap();
    template.latest_version = u64::MAX;
    template.status = erpx_domain::marketing_content::PublicationStatus::Published;
    template.published_version = Some(u64::MAX);
    let before = template.clone();

    assert_eq!(
        template.add_version(version(template_id, u64::MAX, "author")),
        Err(MarketingContentError::VersionOverflow)
    );
    assert_eq!(template, before);
}

#[test]
fn asset_version_overflow_fails_closed_without_inserting_a_duplicate_version() {
    let asset_id = Uuid::from_u128(20);
    let asset_scope = scope();
    let mut asset = ContentAsset::new(
        asset_id,
        asset_scope.clone(),
        "hero-image",
        asset_version(asset_id, 1),
    )
    .unwrap();
    asset.versions = BTreeMap::from([(u64::MAX, asset_version(asset_id, u64::MAX))]);
    let before = asset.clone();

    assert_eq!(
        asset.add_version(asset_version(asset_id, u64::MAX)),
        Err(MarketingContentError::VersionOverflow)
    );
    assert_eq!(asset, before);
    assert_eq!(asset.versions.len(), 1);
    assert!(asset.versions.contains_key(&u64::MAX));
}
