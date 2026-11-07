use erpx_server_files::public::field_visit_upload::{
    FIELD_UPLOAD_MAX_BYTES, FieldFileReservation, field_reservation_replay_matches,
    field_upload_id, field_upload_link_id, normalize_fresh_field_file, validate_field_upload_bytes,
};
fn input(media: &str) -> FieldFileReservation {
    FieldFileReservation {
        filename: "\u{feff}photo.png \u{feff}".into(),
        media_type: media.into(),
        bytes: 20,
        caption: Some(" \u{feff}".into()),
        supersedes_link_id: None,
    }
}
#[test]
fn field_purpose_media_and_metadata_match_retained_policy() {
    let value = normalize_fresh_field_file("site_photo", &input("image/png")).unwrap();
    assert_eq!(value.filename, "photo.png");
    assert_eq!(value.caption, None);
    assert!(normalize_fresh_field_file("site_photo", &input("application/pdf")).is_err());
    for purpose in [
        "technician_report",
        "signed_job_sheet",
        "exception_evidence",
    ] {
        assert!(normalize_fresh_field_file(purpose, &input("application/pdf")).is_ok());
    }
    assert!(normalize_fresh_field_file("other", &input("image/png")).is_err());
    let mut over = input("image/png");
    over.bytes = FIELD_UPLOAD_MAX_BYTES + 1;
    assert!(normalize_fresh_field_file("site_photo", &over).is_err());
    let mut long = input("image/png");
    long.filename = "😀".repeat(128);
    assert!(normalize_fresh_field_file("site_photo", &long).is_err());
}
#[test]
fn reservation_identity_matches_scoped_legacy_derivation() {
    let id = field_upload_id("company", "user", "request-key");
    assert_eq!(id.len(), "attachment-upload-".len() + 32);
    assert_ne!(id, field_upload_id("other", "user", "request-key"));
    assert_ne!(id, field_upload_id("company", "other", "request-key"));
    assert_eq!(
        field_upload_link_id(&id),
        Some(id.replacen("attachment-upload-", "attachment-link-", 1))
    );
    assert!(field_upload_link_id("attachment-upload-not-hex").is_none());
}

#[test]
fn upload_requires_exact_length_and_complete_media_signature() {
    let png = b"\x89PNGabcdefgh\0\0\0\0IEND\xaeB`\x82";
    assert!(validate_field_upload_bytes("image/png", png.len() as i64, png).is_ok());
    assert!(validate_field_upload_bytes("image/png", (png.len() + 1) as i64, png).is_err());
    assert!(validate_field_upload_bytes("image/jpeg", png.len() as i64, png).is_err());
    assert!(validate_field_upload_bytes("image/png", 4, b"\x89PNG").is_err());
}

#[test]
fn replay_matches_original_metadata_without_reapplying_fresh_policy() {
    let original = input("image/png");
    let stored = normalize_fresh_field_file("site_photo", &original).unwrap();
    assert!(field_reservation_replay_matches(&stored, &original));
    let mut changed = original.clone();
    changed.bytes += 1;
    assert!(!field_reservation_replay_matches(&stored, &changed));
    changed = original.clone();
    changed.supersedes_link_id = Some("prior".into());
    assert!(!field_reservation_replay_matches(&stored, &changed));
}
