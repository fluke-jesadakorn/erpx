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
fn citation_mode_matches_actual_typescript_switch() {
    use erpx_server_shared::public::{BusinessError, FailureKind};
    use std::cell::Cell;
    // Execute the real switch; only its trusted runtime lookup is mocked.
    let script = r#"import {mock} from 'bun:test';let environment='local',calls=0;mock.module(process.argv[1],()=>({runtimeEnv:async()=>{calls++;return {ERPX_ENVIRONMENT:environment}}}));const m=await import(process.argv[2]);const cases=[[null,'production'],['disabled','production'],['shadow','local'],['enforced','local'],['shadow','production'],['enforced','staging'],['','local'],['SHADOW','local'],[' enforced','local']];const out=[];for(const [raw,env] of cases){environment=env;calls=0;if(raw===null)delete process.env.ERPX_LEGAL_PACKAGE_ENFORCEMENT;else process.env.ERPX_LEGAL_PACKAGE_ENFORCEMENT=raw;try{out.push({mode:await m.legalPackageEnforcementMode(),calls})}catch(e){out.push({status:e.status,code:e.code,message:e.message,calls})}}process.stdout.write(JSON.stringify(out));"#;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let runtime = root.join("packages/server-runtime/src/public/runtime.ts");
    let switch =
        root.join("packages/server-commercial/src/adapters/development-package-enforcement.ts");
    let output =
        std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(root)
            .args(["--conditions=react-server", "-e", script])
            .arg(runtime)
            .arg(switch)
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "actual TypeScript mode oracle unavailable"
    );
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let cases = [
        (None, "production"),
        (Some("disabled"), "production"),
        (Some("shadow"), "local"),
        (Some("enforced"), "local"),
        (Some("shadow"), "production"),
        (Some("enforced"), "staging"),
        (Some(""), "local"),
        (Some("SHADOW"), "local"),
        (Some(" enforced"), "local"),
    ];
    for (index, (raw, environment)) in cases.into_iter().enumerate() {
        let calls = Cell::new(0);
        let result = resolve_legal_citation_mode(raw, || {
            calls.set(calls.get() + 1);
            Ok(environment)
        });
        let actual = match result {
            Ok(mode) => {
                serde_json::json!({"mode": match mode { LegalCommercialMode::Disabled => "disabled", LegalCommercialMode::Shadow => "shadow", LegalCommercialMode::Enforced => "enforced" }, "calls":calls.get()})
            }
            Err(e) => {
                serde_json::json!({"status":503,"code":e.code,"message":e.message,"calls":calls.get()})
            }
        };
        assert_eq!(actual, expected[index]);
    }
    let err = resolve_legal_citation_mode(Some("shadow"), || {
        Err(BusinessError::new(
            FailureKind::Unavailable,
            "TRUSTED_RUNTIME_UNAVAILABLE",
            "Unavailable",
        ))
    })
    .unwrap_err();
    assert_eq!(err.code, "TRUSTED_RUNTIME_UNAVAILABLE");
}

#[test]
fn citation_purchase_gates_preserve_ts_modes_and_error_precedence() {
    let a = actor();
    let denied = LegalCitationPurchaseObservations {
        kind_history: false,
        documents_history: false,
        scheduled_release: Some(false),
    };
    assert_eq!(
        citation_purchase_plan(LegalCommercialMode::Disabled, "not-a-kind").unwrap(),
        LegalCitationPurchasePlan::Skip
    );
    assert!(
        citation_document_kind_gate(LegalCommercialMode::Disabled, &a, "contract", None).is_ok()
    );
    assert!(citation_document_documents_gate(LegalCommercialMode::Disabled, None).is_ok());
    assert!(citation_law_gate(LegalCommercialMode::Disabled, &a, "rule", None).is_ok());
    assert_eq!(
        citation_document_kind_gate(LegalCommercialMode::Shadow, &a, "contract", None)
            .unwrap_err()
            .code,
        "LEGAL_CITATION_PURCHASE_SOURCE_REQUIRED"
    );
    assert!(
        citation_document_kind_gate(LegalCommercialMode::Shadow, &a, "contract", Some(denied))
            .is_ok()
    );
    assert!(citation_document_documents_gate(LegalCommercialMode::Shadow, Some(denied)).is_ok());
    assert!(citation_law_gate(LegalCommercialMode::Shadow, &a, "rule", Some(denied)).is_ok());
    assert!(
        citation_law_gate(
            LegalCommercialMode::Shadow,
            &a,
            "rule",
            Some(LegalCitationPurchaseObservations {
                scheduled_release: None,
                ..denied
            })
        )
        .is_ok()
    );
    assert_eq!(
        citation_document_kind_gate(LegalCommercialMode::Enforced, &a, "contract", Some(denied))
            .unwrap_err()
            .code,
        "PACKAGE_REQUIRED"
    );
    let released_denied = LegalCitationPurchaseObservations {
        kind_history: true,
        ..denied
    };
    assert_eq!(
        citation_document_kind_gate(
            LegalCommercialMode::Enforced,
            &a,
            "contract",
            Some(released_denied)
        )
        .unwrap_err()
        .code,
        "FEATURE_COMING_SOON"
    );
    let docs_denied = LegalCitationPurchaseObservations {
        scheduled_release: Some(true),
        ..released_denied
    };
    assert!(
        citation_document_kind_gate(
            LegalCommercialMode::Enforced,
            &a,
            "contract",
            Some(docs_denied)
        )
        .is_ok()
    );
    assert_eq!(
        citation_document_documents_gate(LegalCommercialMode::Enforced, Some(docs_denied))
            .unwrap_err()
            .code,
        "PACKAGE_REQUIRED"
    );
    assert_eq!(
        citation_law_gate(LegalCommercialMode::Enforced, &a, "rule", Some(denied))
            .unwrap_err()
            .code,
        "LEGAL_CITATION_UNAVAILABLE"
    );
    let mut manager = a.clone();
    manager.permissions.insert("legal.manage".into());
    assert!(
        citation_document_kind_gate(
            LegalCommercialMode::Enforced,
            &manager,
            "template",
            Some(released_denied)
        )
        .is_ok()
    );
    manager.branch_id = Some(id("branch"));
    assert_eq!(
        citation_document_kind_gate(
            LegalCommercialMode::Enforced,
            &manager,
            "template",
            Some(released_denied)
        )
        .unwrap_err()
        .code,
        "FEATURE_COMING_SOON"
    );
}

#[test]
fn citation_kind_mapping_matches_actual_typescript_policy() {
    let script = r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const {legalKindCapability}=await import('./packages/server-legal/src/domain/purchases.ts');process.stdout.write(JSON.stringify(['contract','template','request','matter','counsel','ip','obligation','compliance','license','corporate','privacy','rule'].map(legalKindCapability)));"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .arg("-e")
            .arg(script)
            .output()
            .unwrap();
    assert!(output.status.success());
    let actual: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let expected: Vec<String> = [
        "contract",
        "template",
        "request",
        "matter",
        "counsel",
        "ip",
        "obligation",
        "compliance",
        "license",
        "corporate",
        "privacy",
        "rule",
    ]
    .iter()
    .map(|kind| legal_kind_capability(kind).unwrap().to_owned())
    .collect();
    assert_eq!(actual, expected);
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
