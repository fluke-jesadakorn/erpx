//! Pure Files attachment metadata policy tests. No database or HTTP authority.
use erpx_server_files::public::attachment_read::{
    AttachmentLinkFacts, AttachmentListFilter, AttachmentReader, AttachmentScanFacts,
    AttachmentSetFacts, AttachmentStatusFacts, AttachmentTargetScope, bindable, may_read_status,
    require_current, validate_set, validate_status,
};
use erpx_server_shared::public::{CompanyId, FailureKind};

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn reader() -> AttachmentReader {
    AttachmentReader::new("actor", company(), Some("branch".into()), None, 10_000).unwrap()
}

fn filter(target_type: &str) -> AttachmentListFilter {
    AttachmentListFilter::new(target_type, "record-1", None, None, None).unwrap()
}

fn link(id: &str, state: &str) -> AttachmentLinkFacts {
    AttachmentLinkFacts {
        id: id.into(),
        source_id: Some("source-1".into()),
        filename: "evidence.pdf".into(),
        media_type: "application/pdf".into(),
        bytes: Some(12),
        source_state: Some("available".into()),
        link_state: state.into(),
        caption: None,
        preview_available: true,
        created_at: 1_000,
        expires_at: 2_000,
        bound_at: (state == "bound").then_some(1_500),
    }
}

fn set(target_type: &str, links: Vec<AttachmentLinkFacts>) -> AttachmentSetFacts {
    AttachmentSetFacts {
        company_id: "company".into(),
        id: "set-1".into(),
        target_type: target_type.into(),
        target_id: "record-1".into(),
        component_kind: "record".into(),
        component_ref: None,
        purpose_key: "specification".into(),
        expected_target_version: 3,
        policy_version: 1,
        classification: "internal".into(),
        required_count: 1,
        max_files: 10,
        state: "draft".into(),
        created_at: 1_000,
        updated_at: 1_100,
        sanitization_attested: None,
        links,
    }
}

fn status(target_type: &str, state: &str) -> AttachmentStatusFacts {
    AttachmentStatusFacts {
        company_id: "company".into(),
        link_id: "link-1".into(),
        set_id: "set-1".into(),
        creator_id: "actor".into(),
        target_type: target_type.into(),
        target_id: "record-1".into(),
        component_kind: "record".into(),
        purpose_key: "specification".into(),
        expected_target_version: 3,
        source_id: Some("source-1".into()),
        source_state: Some("available".into()),
        link_state: state.into(),
        preview_available: true,
        expires_at: 2_000,
        bound_at: (state == "bound").then_some(1_500),
        withdrawn_at: (state == "withdrawn").then_some(1_600),
        scan: AttachmentScanFacts {
            attempts: 1,
            last_result: Some("clean".into()),
            last_error: None,
            signature: None,
            definition_updated_at: Some(1_400),
            scanned_at: Some(1_400),
        },
    }
}

fn scope() -> AttachmentTargetScope {
    AttachmentTargetScope::new(company(), Some("branch".into()), None, 3).unwrap()
}

#[test]
fn filter_requires_bounded_target_and_known_context() {
    assert_eq!(
        AttachmentListFilter::new("", "record-1", None, None, None)
            .unwrap_err()
            .code,
        "ATTACHMENT_TARGET_REQUIRED"
    );
    assert_eq!(
        AttachmentListFilter::new("asset", "", None, None, None)
            .unwrap_err()
            .code,
        "ATTACHMENT_TARGET_REQUIRED"
    );
    assert_eq!(
        AttachmentListFilter::new("asset", "id".repeat(181), None, None, None)
            .unwrap_err()
            .code,
        "ATTACHMENT_TARGET_REQUIRED"
    );
    assert_eq!(
        AttachmentListFilter::new("asset", "record", Some("purpose".repeat(12)), None, None)
            .unwrap_err()
            .code,
        "ATTACHMENT_POLICY_UNKNOWN"
    );
    assert_eq!(
        AttachmentListFilter::new("asset", "record", None, Some("document".into()), None)
            .unwrap_err()
            .code,
        "ATTACHMENT_COMPONENT_KIND"
    );
    assert_eq!(
        AttachmentListFilter::new("asset", "record", None, None, Some(0))
            .unwrap_err()
            .code,
        "ATTACHMENT_TARGET_VERSION"
    );
    let leave = AttachmentListFilter::new(
        "leave_request",
        "leave-1",
        Some("medical_certificate".into()),
        Some("record".into()),
        Some(7),
    )
    .unwrap();
    assert!(leave.leave_review());
    assert_eq!(leave.expected_target_version(), Some(7));
    assert!(!filter("asset").leave_review());
}

#[test]
fn reader_rejects_an_absent_or_expired_selected_context() {
    assert!(AttachmentReader::new("", company(), None, None, 10).is_err());
    assert!(AttachmentReader::new("bad\nactor", company(), None, None, 10).is_err());
    assert!(AttachmentReader::new("actor", company(), None, None, 0).is_err());
    assert!(AttachmentReader::new("actor", company(), Some(String::new()), None, 10).is_err());
    let reader = reader();
    assert_eq!(reader.user_id(), "actor");
    assert_eq!(reader.company_id().as_str(), "company");
    assert_eq!(reader.branch_id(), Some("branch"));
    assert!(require_current(&reader, 9_999).is_ok());
    assert_eq!(
        require_current(&reader, 10_000).unwrap_err().code,
        "WORK_CONTEXT_CHANGED"
    );
}

#[test]
fn set_rejects_foreign_company_wrong_target_and_invalid_links() {
    let valid = validate_set(
        &reader(),
        &filter("asset"),
        set("asset", vec![link("l1", "draft")]),
        5_000,
    )
    .unwrap();
    assert_eq!(valid.links.len(), 1);
    assert_eq!(valid.links[0].filename, "evidence.pdf");

    let mut foreign = set("asset", vec![]);
    foreign.company_id = "other".into();
    assert_eq!(
        validate_set(&reader(), &filter("asset"), foreign, 5_000)
            .unwrap_err()
            .code,
        "ATTACHMENT_READ_CHANGED"
    );

    let wrong_target = set("asset", vec![]);
    assert!(
        validate_set(
            &reader(),
            &filter("marketing_campaign"),
            wrong_target,
            5_000
        )
        .is_err()
    );

    let mut bad_counts = set("asset", vec![]);
    bad_counts.required_count = 11;
    bad_counts.max_files = 10;
    assert!(validate_set(&reader(), &filter("asset"), bad_counts, 5_000).is_err());

    let mut bad_time = set("asset", vec![]);
    bad_time.updated_at = 999;
    assert!(validate_set(&reader(), &filter("asset"), bad_time, 5_000).is_err());

    let duplicate = set("asset", vec![link("l1", "draft"), link("l1", "draft")]);
    assert!(validate_set(&reader(), &filter("asset"), duplicate, 5_000).is_err());

    let mut out_of_order = link("l2", "draft");
    out_of_order.created_at = 999;
    assert!(
        validate_set(
            &reader(),
            &filter("asset"),
            set("asset", vec![link("l1", "draft"), out_of_order]),
            5_000
        )
        .is_err()
    );

    let mut missing_bound = link("l1", "bound");
    missing_bound.bound_at = None;
    assert!(
        validate_set(
            &reader(),
            &filter("asset"),
            set("asset", vec![missing_bound]),
            5_000
        )
        .is_err()
    );

    let mut no_source_state = link("l1", "draft");
    no_source_state.source_state = None;
    assert!(
        validate_set(
            &reader(),
            &filter("asset"),
            set("asset", vec![no_source_state]),
            5_000
        )
        .is_err()
    );

    let mut unknown_state = link("l1", "draft");
    unknown_state.link_state = "approved".into();
    assert!(
        validate_set(
            &reader(),
            &filter("asset"),
            set("asset", vec![unknown_state]),
            5_000
        )
        .is_err()
    );

    let mut future_bound = link("l1", "bound");
    future_bound.bound_at = Some(6_000);
    assert!(
        validate_set(
            &reader(),
            &filter("asset"),
            set("asset", vec![future_bound]),
            5_000
        )
        .is_err()
    );
}

#[test]
fn sanitization_attestation_is_required_only_for_review_targets() {
    let mut review = set("iam_review", vec![]);
    assert!(validate_set(&reader(), &filter("iam_review"), review.clone(), 5_000).is_err());
    review.sanitization_attested = Some(true);
    let view = validate_set(&reader(), &filter("iam_review"), review, 5_000).unwrap();
    assert_eq!(view.sanitization_attested, Some(true));

    let mut ordinary = set("asset", vec![]);
    ordinary.sanitization_attested = Some(false);
    assert!(validate_set(&reader(), &filter("asset"), ordinary, 5_000).is_err());
    assert_eq!(
        validate_set(&reader(), &filter("asset"), set("asset", vec![]), 5_000)
            .unwrap()
            .sanitization_attested,
        None
    );
}

#[test]
fn draft_status_is_private_to_the_creator_except_leave_review() {
    let creator = status("asset", "draft");
    assert!(may_read_status(&reader(), &creator));
    let mut other = creator.clone();
    other.creator_id = "other".into();
    assert!(!may_read_status(&reader(), &other));
    let mut leave = other.clone();
    leave.target_type = "leave_request".into();
    assert!(may_read_status(&reader(), &leave));
}

#[test]
fn bindable_preserves_scan_state_and_exact_leave_version() {
    let draft = status("asset", "draft");
    assert!(bindable(&draft, 3));
    assert!(!bindable(&draft, 2));
    assert!(!bindable(&draft, 4));

    let mut pending = draft.clone();
    pending.source_state = Some("scanning".into());
    assert!(!bindable(&pending, 3));

    let mut bound = draft.clone();
    bound.link_state = "bound".into();
    bound.bound_at = Some(1_500);
    assert!(!bindable(&bound, 3));

    let mut leave = draft.clone();
    leave.target_type = "leave_request".into();
    assert!(bindable(&leave, 3));
    assert!(bindable(&leave, 4));
    assert!(!bindable(&leave, 2));
}

#[test]
fn status_rejects_foreign_company_inconsistent_times_and_scan_evidence() {
    let valid = validate_status(&reader(), status("asset", "draft"), &scope(), 5_000).unwrap();
    assert_eq!(valid.link_id, "link-1");
    assert!(valid.bindable);

    let mut foreign = status("asset", "draft");
    foreign.company_id = "other".into();
    assert_eq!(
        validate_status(&reader(), foreign, &scope(), 5_000)
            .unwrap_err()
            .kind,
        FailureKind::Forbidden
    );

    let mut missing = status("asset", "draft");
    missing.set_id = String::new();
    assert_eq!(
        validate_status(&reader(), missing, &scope(), 5_000)
            .unwrap_err()
            .code,
        "ATTACHMENT_READ_CHANGED"
    );

    let mut bad_bound = status("asset", "bound");
    bad_bound.bound_at = None;
    assert!(validate_status(&reader(), bad_bound, &scope(), 5_000).is_err());

    let mut bad_scan = status("asset", "draft");
    bad_scan.scan.attempts = 13;
    assert!(validate_status(&reader(), bad_scan, &scope(), 5_000).is_err());

    let mut bad_result = status("asset", "draft");
    bad_result.scan.last_result = Some("maybe".into());
    assert!(validate_status(&reader(), bad_result, &scope(), 5_000).is_err());

    let mut withdrawn = status("asset", "withdrawn");
    withdrawn.withdrawn_at = Some(5_000);
    assert!(validate_status(&reader(), withdrawn, &scope(), 5_000).is_ok());
    let mut missing_withdrawal = status("asset", "withdrawn");
    missing_withdrawal.withdrawn_at = None;
    assert!(validate_status(&reader(), missing_withdrawal, &scope(), 5_000).is_err());
}

#[test]
fn drafts_owned_by_another_person_are_not_visible_as_status() {
    let mut other = status("asset", "draft");
    other.creator_id = "someone-else".into();
    assert_eq!(
        validate_status(&reader(), other, &scope(), 5_000)
            .unwrap_err()
            .code,
        "ATTACHMENT_LINK_NOT_FOUND"
    );
}
