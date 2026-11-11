//! Files-side Messaging quarantine, scan and variant policy. No database.
use erpx_server_files::public::messaging_files::{
    MAX_MESSAGING_FILE_BYTES, MessagingFileVariant, valid_messaging_attachment_source,
};

fn checksum() -> String {
    "a".repeat(64)
}

#[test]
fn only_original_and_preview_variants_are_accepted() {
    assert_eq!(
        MessagingFileVariant::parse("original").unwrap(),
        MessagingFileVariant::Original
    );
    assert_eq!(
        MessagingFileVariant::parse("preview").unwrap(),
        MessagingFileVariant::Preview
    );
    let error = MessagingFileVariant::parse("thumbnail").unwrap_err();
    assert_eq!(error.code, "MESSAGING_INPUT_INVALID");
    assert_eq!(MAX_MESSAGING_FILE_BYTES, 10 * 1024 * 1024);
}

#[test]
fn only_clean_available_sources_are_readable() {
    let checksum = checksum();
    assert!(valid_messaging_attachment_source(
        Some("available"),
        Some(1),
        Some(1),
        None,
        None,
        Some(512),
        Some(&checksum),
        Some("application/pdf"),
    ));
    // Quarantined, unscanned, malware-rejected and destroyed sources all fail.
    for state in [
        "quarantined",
        "scanning",
        "scan_error",
        "malware_rejected",
        "destroyed",
    ] {
        assert!(!valid_messaging_attachment_source(
            Some(state),
            Some(1),
            Some(1),
            None,
            None,
            Some(512),
            Some(&checksum),
            Some("application/pdf"),
        ));
    }
    assert!(!valid_messaging_attachment_source(
        Some("available"),
        None,
        Some(1),
        None,
        None,
        Some(512),
        Some(&checksum),
        Some("application/pdf"),
    ));
    assert!(!valid_messaging_attachment_source(
        Some("available"),
        Some(1),
        None,
        None,
        None,
        Some(512),
        Some(&checksum),
        Some("application/pdf"),
    ));
    assert!(!valid_messaging_attachment_source(
        Some("available"),
        Some(1),
        Some(1),
        Some(1),
        None,
        Some(512),
        Some(&checksum),
        Some("application/pdf"),
    ));
    assert!(!valid_messaging_attachment_source(
        Some("available"),
        Some(1),
        Some(1),
        None,
        Some(1),
        Some(512),
        Some(&checksum),
        Some("application/pdf"),
    ));
}

#[test]
fn source_metadata_is_bounded_and_typed() {
    let checksum = checksum();
    assert!(!valid_messaging_attachment_source(
        Some("available"),
        Some(1),
        Some(1),
        None,
        None,
        Some(0),
        Some(&checksum),
        Some("application/pdf"),
    ));
    assert!(!valid_messaging_attachment_source(
        Some("available"),
        Some(1),
        Some(1),
        None,
        None,
        Some(MAX_MESSAGING_FILE_BYTES as i64 + 1),
        Some(&checksum),
        Some("application/pdf"),
    ));
    assert!(!valid_messaging_attachment_source(
        Some("available"),
        Some(1),
        Some(1),
        None,
        None,
        Some(512),
        Some("not-a-checksum"),
        Some("application/pdf"),
    ));
    assert!(!valid_messaging_attachment_source(
        Some("available"),
        Some(1),
        Some(1),
        None,
        None,
        Some(512),
        Some(&checksum),
        Some(""),
    ));
}
