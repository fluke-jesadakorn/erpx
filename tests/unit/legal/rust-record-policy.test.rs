use erpx_server_legal::public::record_policy::*;
use erpx_server_shared::public::{CompanyId, HoldingId, RecordId, UserId};
fn id(v: &str) -> RecordId {
    RecordId::new(v).unwrap()
}
fn actor() -> LegalRecordActor {
    LegalRecordActor {
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("reader").unwrap(),
        branch_id: None,
        department_id: None,
        role: "legal_counsel".into(),
        permissions: ["legal.read", "legal.edit", "legal.export"]
            .map(str::to_owned)
            .into(),
    }
}
fn facts() -> LegalRecordFacts {
    LegalRecordFacts {
        item: LegalRecordSummary {
            id: id("item"),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: Some(id("branch")),
            department_id: Some(id("department")),
            kind: "contract".into(),
            classification: "internal".into(),
            created_by: UserId::new("reader").unwrap(),
            state: "draft".into(),
            version: 1,
        },
        member: false,
        editor: false,
        enabled: true,
        ai_enabled: false,
        held: false,
        disposed: false,
        destruction_executing: false,
        destruction_completed: false,
        document: Some(LegalRecordDocument {
            id: id("document"),
            item_id: id("item"),
            version: 1,
            checksum_sha256: "a".repeat(64),
            media_type: "application/pdf".into(),
            bytes: 1,
            attachment_link_id: Some(id("link")),
            attachment_source_id: Some(id("source")),
        }),
    }
}
fn request(op: LegalRecordOperation) -> LegalRecordRequest {
    LegalRecordRequest {
        item_id: id("item"),
        document_id: Some(id("document")),
        operation: op,
        mode: LegalCommercialMode::Enforced,
    }
}
fn release() -> LegalReleaseObservation {
    LegalReleaseObservation {
        allowed: true,
        revision: "current-reviewed-release".into(),
    }
}
#[test]
fn confidentiality_scope_requester_and_editor_are_independent_requirements() {
    let a = actor();
    let mut f = facts();
    let r = request(LegalRecordOperation::Metadata);
    assert!(authorize_legal_record(&a, &r, &f, &release()).is_ok());
    for classification in ["confidential", "restricted"] {
        f.item.classification = classification.into();
        assert_eq!(
            authorize_legal_record(&a, &r, &f, &release())
                .unwrap_err()
                .code,
            "LEGAL_NOT_FOUND"
        );
    }
    f.member = true;
    assert!(authorize_legal_record(&a, &r, &f, &release()).is_ok());
    let mut scoped = a.clone();
    scoped.branch_id = Some(id("wrong"));
    assert_eq!(
        authorize_legal_record(&scoped, &r, &f, &release())
            .unwrap_err()
            .code,
        "LEGAL_NOT_FOUND"
    );
    scoped = a.clone();
    scoped.department_id = Some(id("wrong"));
    assert!(authorize_legal_record(&scoped, &r, &f, &release()).is_err());
    scoped = a.clone();
    scoped.role = "legal_requester".into();
    assert!(authorize_legal_record(&scoped, &r, &f, &release()).is_err());
    f.item.kind = "request".into();
    assert!(authorize_legal_record(&scoped, &r, &f, &release()).is_ok());
    f.item.created_by = UserId::new("other").unwrap();
    assert!(authorize_legal_record(&scoped, &r, &f, &release()).is_err());
    assert_eq!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Edit), &f, &release())
            .unwrap_err()
            .code,
        "LEGAL_EDITOR_REQUIRED"
    );
    f.editor = true;
    assert!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Edit), &f, &release()).is_ok()
    );
}
#[test]
fn holds_preserve_readable_evidence_but_disposal_and_destruction_block_content() {
    let a = actor();
    let mut f = facts();
    f.held = true;
    assert!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Document), &f, &release())
            .is_ok()
    );
    f.disposed = true;
    assert!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Metadata), &f, &release())
            .is_ok()
    );
    assert_eq!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Document), &f, &release())
            .unwrap_err()
            .code,
        "LEGAL_DOCUMENT_DISPOSED"
    );
    f.disposed = false;
    f.destruction_executing = true;
    assert_eq!(
        authorize_legal_record(
            &a,
            &request(LegalRecordOperation::Processing),
            &f,
            &release()
        )
        .unwrap_err()
        .code,
        "LEGAL_RETENTION_FROZEN"
    );
    f.destruction_executing = false;
    f.destruction_completed = true;
    assert_eq!(
        authorize_legal_record(
            &a,
            &request(LegalRecordOperation::Processing),
            &f,
            &release()
        )
        .unwrap_err()
        .code,
        "LEGAL_PROCESSING_UNAVAILABLE"
    );
}
#[test]
fn release_preparation_exception_never_releases_a_worker_or_turns_off_readiness() {
    let mut a = actor();
    let mut f = facts();
    let release = LegalReleaseObservation {
        allowed: false,
        revision: "unreleased".into(),
    };
    assert_eq!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Metadata), &f, &release)
            .unwrap_err()
            .code,
        "FEATURE_COMING_SOON"
    );
    a.permissions.insert("legal.manage".into());
    f.item.kind = "rule".into();
    assert!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Metadata), &f, &release).is_ok()
    );
    assert_eq!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Processing), &f, &release)
            .unwrap_err()
            .code,
        "LEGAL_PROCESSING_UNAVAILABLE"
    );
    let mut r = request(LegalRecordOperation::Processing);
    r.mode = LegalCommercialMode::Disabled;
    assert!(authorize_legal_record(&a, &r, &f, &release).is_err());
    f.enabled = false;
    assert!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Metadata), &f, &release).is_ok()
    );
    assert_eq!(
        authorize_legal_record(&a, &request(LegalRecordOperation::Export), &f, &release)
            .unwrap_err()
            .code,
        "LEGAL_DISABLED"
    );
}
#[test]
fn purchases_use_actual_kind_and_distinguish_history_queue_and_completion() {
    for (kind, cap) in [
        ("contract", "legal.contracts"),
        ("template", "legal.contracts"),
        ("request", "legal.matters"),
        ("matter", "legal.matters"),
        ("counsel", "legal.matters"),
        ("ip", "legal.matters"),
        ("obligation", "legal.obligations"),
        ("compliance", "legal.obligations"),
        ("license", "legal.obligations"),
        ("corporate", "legal.obligations"),
        ("privacy", "legal.obligations"),
        ("rule", "legal.obligations"),
    ] {
        let mut f = facts();
        f.item.kind = kind.into();
        assert_eq!(legal_kind_capability(kind).unwrap(), cap);
        for (op, purpose) in [
            (
                LegalRecordOperation::QueueProcessing,
                LegalPurchasePurpose::NewWork,
            ),
            (
                LegalRecordOperation::Processing,
                LegalPurchasePurpose::SettleExisting,
            ),
        ] {
            let actual = authorize_legal_record(&actor(), &request(op), &f, &release()).unwrap();
            assert_eq!(actual.len(), 4);
            for c in [cap, "legal.documents"] {
                assert!(actual.contains(&LegalPurchaseRequirement {
                    capability: c.into(),
                    purpose
                }));
                assert!(actual.contains(&LegalPurchaseRequirement {
                    capability: c.into(),
                    purpose: LegalPurchasePurpose::History
                }));
            }
        }
    }
    assert!(legal_kind_capability("user-selected-unknown").is_err());
}
