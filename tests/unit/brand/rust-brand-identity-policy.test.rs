//! Retained Brand identity acceptance values: strict inner objects, trimming,
//! bounded lengths, integer versions and the canonical stored JSON order.
use erpx_server_brand::public::identity::*;
use serde_json::{Value, json};

fn identity(short_name: &str, tagline: Option<&str>, accent: &str, logo: Option<&str>) -> Value {
    let mut identity = json!({"shortName": short_name});
    if let Some(tagline) = tagline {
        identity["tagline"] = json!(tagline);
    }
    json!({
        "identity": identity,
        "visual": {"accentColor": accent, "logoPublicationId": logo},
    })
}

fn draft(input: Value) -> BrandDraftInput {
    BrandDraftInput::parse(&input).expect("valid draft")
}

#[test]
fn identity_values_are_trimmed_and_the_accent_is_normalised() {
    let parsed = BrandIdentity::parse(&identity(
        "  Engineering demo  ",
        Some("  Approved company identity  "),
        "#AABBCC",
        Some("publication"),
    ))
    .unwrap();
    assert_eq!(parsed.identity.short_name, "Engineering demo");
    assert_eq!(parsed.identity.tagline, "Approved company identity");
    assert_eq!(parsed.visual.accent_color, "#aabbcc");
    assert_eq!(
        parsed.visual.logo_publication_id.as_deref(),
        Some("publication")
    );
    let omitted = BrandIdentity::parse(&identity("Acme", None, "#224488", None)).unwrap();
    assert_eq!(omitted.identity.tagline, "");
    assert_eq!(omitted.visual.logo_publication_id, None);
    assert_eq!(
        omitted.canonical_json().unwrap(),
        r##"{"identity":{"shortName":"Acme","tagline":""},"visual":{"accentColor":"#224488","logoPublicationId":null}}"##
    );
}

#[test]
fn identity_rejects_unknown_fields_and_out_of_bounds_values() {
    let mut unknown = identity("Acme", Some("Tagline"), "#224488", None);
    unknown["identity"]["extra"] = json!("no");
    for invalid in [
        unknown,
        identity("", Some("Tagline"), "#224488", None),
        identity(&"x".repeat(41), Some("Tagline"), "#224488", None),
        identity("Acme", Some(&"x".repeat(161)), "#224488", None),
        identity("Acme", Some("Tagline"), "#22448", None),
        identity("Acme", Some("Tagline"), "#GGGGGG", None),
        identity("Acme", Some("Tagline"), "#224488", Some("")),
        json!({"identity": {"shortName": "Acme", "tagline": "Tagline"}, "visual": {"accentColor": "#224488"}}),
        json!({"identity": {"shortName": "Acme", "tagline": "Tagline"}, "visual": {"accentColor": "#224488", "logoPublicationId": null}, "extra": true}),
        json!({"identity": {"shortName": 40, "tagline": "Tagline"}, "visual": {"accentColor": "#224488", "logoPublicationId": null}}),
    ] {
        let error = BrandIdentity::parse(&invalid).unwrap_err();
        assert_eq!(error.code, "VALIDATION_FAILED", "{invalid}");
    }
}

#[test]
fn draft_input_requires_the_publication_base_and_bounded_reason() {
    let parsed = draft(json!({
        "action": "save",
        "expectedPublishedVersion": 0,
        "identity": identity("Acme", Some("Tagline"), "#224488", None),
        "reason": "  Reviewed identity values  ",
    }));
    assert_eq!(parsed.id, None);
    assert_eq!(parsed.expected_version, None);
    assert_eq!(parsed.expected_published_version, 0);
    assert_eq!(parsed.reason, "Reviewed identity values");
    assert_eq!(
        serde_json::to_value(&parsed).unwrap(),
        json!({
            "expectedPublishedVersion": 0,
            "identity": {
                "identity": {"shortName": "Acme", "tagline": "Tagline"},
                "visual": {"accentColor": "#224488", "logoPublicationId": null},
            },
            "reason": "Reviewed identity values",
        })
    );
    let with_version = draft(json!({
        "id": "brand-draft-key",
        "expectedVersion": 2,
        "expectedPublishedVersion": 1,
        "identity": identity("Acme", None, "#224488", None),
        "reason": "Reviewed change",
    }));
    assert_eq!(with_version.id.as_deref(), Some("brand-draft-key"));
    assert_eq!(with_version.expected_version, Some(2));
    for invalid in [
        json!({"identity": identity("Acme", None, "#224488", None), "reason": "Reviewed"}),
        json!({"expectedPublishedVersion": -1, "identity": identity("Acme", None, "#224488", None), "reason": "Reviewed"}),
        json!({"expectedVersion": 0, "expectedPublishedVersion": 0, "identity": identity("Acme", None, "#224488", None), "reason": "Reviewed"}),
        json!({"id": "", "expectedPublishedVersion": 0, "identity": identity("Acme", None, "#224488", None), "reason": "Reviewed"}),
        json!({"expectedPublishedVersion": 0, "identity": identity("Acme", None, "#224488", None), "reason": "  ab  "}),
        json!({"expectedPublishedVersion": 0, "identity": identity("Acme", None, "#224488", None), "reason": "x".repeat(4001)}),
        json!({"expectedPublishedVersion": 1.5, "identity": identity("Acme", None, "#224488", None), "reason": "Reviewed"}),
    ] {
        assert_eq!(
            BrandDraftInput::parse(&invalid).unwrap_err().code,
            "VALIDATION_FAILED",
            "{invalid}"
        );
    }
}

#[test]
fn publish_and_cancel_inputs_require_positive_draft_versions() {
    assert_eq!(
        BrandPublishInput::parse(&json!({
            "id": "ignored",
            "expectedVersion": 1,
            "expectedPublishedVersion": 0,
            "reason": "Independent review",
        }))
        .unwrap()
        .expected_version,
        1
    );
    assert_eq!(
        BrandCancelInput::parse(&json!({"expectedVersion": 1, "reason": "Withdraw"}))
            .unwrap()
            .reason,
        "Withdraw"
    );
    for invalid in [
        json!({"expectedVersion": 0, "expectedPublishedVersion": 0, "reason": "Independent"}),
        json!({"expectedVersion": 1, "expectedPublishedVersion": -1, "reason": "Independent"}),
        json!({"expectedVersion": 1, "reason": "No"}),
    ] {
        assert_eq!(
            BrandPublishInput::parse(&invalid).unwrap_err().code,
            "VALIDATION_FAILED"
        );
    }
    for invalid in [
        json!({"expectedVersion": 0, "reason": "Withdraw"}),
        json!({"expectedVersion": 1, "reason": ""}),
    ] {
        assert_eq!(
            BrandCancelInput::parse(&invalid).unwrap_err().code,
            "VALIDATION_FAILED"
        );
    }
}

#[test]
fn rollback_and_category_lock_inputs_preserve_their_bounds() {
    let rollback = BrandRollbackInput::parse(&json!({
        "publishedId": "brand-version-key",
        "expectedPublishedVersion": 2,
        "reason": "Restore approved identity",
    }))
    .unwrap();
    assert_eq!(rollback.published_id, "brand-version-key");
    assert_eq!(rollback.expected_published_version, 2);
    for invalid in [
        json!({"publishedId": "", "expectedPublishedVersion": 2, "reason": "Restore"}),
        json!({"publishedId": "brand-version-key", "expectedPublishedVersion": 0, "reason": "Restore"}),
    ] {
        assert_eq!(
            BrandRollbackInput::parse(&invalid).unwrap_err().code,
            "VALIDATION_FAILED"
        );
    }
    let lock = BrandCategoryLockInput::parse(&json!({
        "category": "visual",
        "locked": true,
        "expectedVersion": 0,
        "expectedPublishedVersion": 1,
        "reason": "Protect approved colors",
    }))
    .unwrap();
    assert_eq!(lock.category, BrandCategory::Visual);
    assert!(lock.locked);
    assert_eq!(lock.expected_version, 0);
    for invalid in [
        json!({"category": "layout", "locked": true, "expectedVersion": 0, "expectedPublishedVersion": 1, "reason": "Protect"}),
        json!({"category": "visual", "locked": "yes", "expectedVersion": 0, "expectedPublishedVersion": 1, "reason": "Protect"}),
        json!({"category": "visual", "locked": true, "expectedVersion": -1, "expectedPublishedVersion": 1, "reason": "Protect"}),
        json!({"category": "visual", "locked": true, "expectedVersion": 0, "expectedPublishedVersion": 0, "reason": "Protect"}),
    ] {
        assert_eq!(
            BrandCategoryLockInput::parse(&invalid).unwrap_err().code,
            "VALIDATION_FAILED"
        );
    }
}

#[test]
fn approved_category_locks_compare_canonical_sections() {
    let current = BrandIdentity::parse(&identity(
        "Acme",
        Some("Approved"),
        "#224488",
        Some("publication"),
    ))
    .unwrap();
    let locks = [
        BrandCategoryLock {
            company_id: "company".into(),
            category: "identity".into(),
            locked: true,
            approved_snapshot_json: json!(current.identity).to_string(),
            version: 1,
            updated_by: "owner".into(),
            reason: "Reviewed".into(),
            updated_at: 1,
        },
        BrandCategoryLock {
            company_id: "company".into(),
            category: "visual".into(),
            locked: true,
            approved_snapshot_json: json!(current.visual).to_string(),
            version: 1,
            updated_by: "owner".into(),
            reason: "Reviewed".into(),
            updated_at: 1,
        },
    ];
    assert_brand_category_locks(&locks, &current).unwrap();
    let mut changed = current.clone();
    changed.visual.logo_publication_id = None;
    let error = assert_brand_category_locks(&locks, &changed).unwrap_err();
    assert_eq!(error.code, "BRAND_CATEGORY_LOCKED");
    assert!(error.message.contains("visual"));
    let mut name = current;
    name.identity.short_name = "Another name".into();
    let error = assert_brand_category_locks(&locks, &name).unwrap_err();
    assert_eq!(error.code, "BRAND_CATEGORY_LOCKED");
    assert!(error.message.contains("identity"));
}

#[test]
fn publication_requires_an_independent_reviewer() {
    assert_brand_independent_publisher(false, false).unwrap();
    for (created, updated) in [(true, false), (false, true), (true, true)] {
        let error = assert_brand_independent_publisher(created, updated).unwrap_err();
        assert_eq!(error.code, "BRAND_INDEPENDENT_PUBLICATION");
    }
}
