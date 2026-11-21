use erpx_server_files::public::field_visit_intake::{
    classification, component_ref, validate_component_ref,
};
#[test]
fn field_purposes_keep_the_legacy_classification() {
    assert_eq!(
        classification("field_service_visit", "record", "site_photo").unwrap(),
        "internal"
    );
    for purpose in [
        "technician_report",
        "signed_job_sheet",
        "exception_evidence",
    ] {
        assert_eq!(
            classification("field_service_visit", "record", purpose).unwrap(),
            "confidential"
        );
    }
    assert!(classification("field_service_visit", "action", "site_photo").is_err());
    assert!(classification("field_service_visit", "record", "unknown").is_err());
}

#[test]
fn component_context_matches_javascript_utf16_limit() {
    assert_eq!(component_ref(Some("   ")), None);
    assert_eq!(component_ref(Some("  room  ")), Some("room"));
    assert_eq!(component_ref(Some("\u{feff}room\u{feff}")), Some("room"));
    assert_eq!(
        component_ref(Some("\u{0085}room\u{0085}")),
        Some("\u{0085}room\u{0085}")
    );
    assert_eq!(component_ref(Some("\u{feff}")), None);
    assert!(validate_component_ref(Some(&"😀".repeat(90))).is_ok());
    assert!(validate_component_ref(Some(&"😀".repeat(91))).is_err());
}
