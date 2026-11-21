//! Commerce record-source payload shape and portable contract validation.
use erpx_server_commerce::public::record_source::CONTRACT_VERSION;
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceRecord};

#[test]
fn commerce_source_payload_matches_the_shared_collaboration_contract() {
    let raw = r#"{"kind":"commerce_order","id":"sales-1","company_id":"company","branch_id":"branch-1","department_id":"department-1","version":3,"title":"SO-1","status":"confirmed"}"#;
    let record: SourceRecord = serde_json::from_str(raw).expect("owner payload shape");
    assert_eq!(record.kind, SourceKind::CommerceOrder);
    assert_eq!(record.kind.as_str(), "commerce_order");
    assert_eq!(record.id, "sales-1");
    assert_eq!(record.company_id, "company");
    assert_eq!(record.branch_id.as_deref(), Some("branch-1"));
    assert_eq!(record.department_id.as_deref(), Some("department-1"));
    assert_eq!(record.version, 3);
    assert_eq!(record.title, "SO-1");
    assert_eq!(record.status, "confirmed");
    record.validate().unwrap();
    assert_eq!(CONTRACT_VERSION, "records.source-facts.v1");
}

#[test]
fn commerce_source_payload_allows_a_null_department() {
    let raw = r#"{"kind":"commerce_order","id":"quote-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"QT-1","status":"draft"}"#;
    let record: SourceRecord = serde_json::from_str(raw).expect("owner payload shape");
    assert_eq!(record.department_id, None);
    record.validate().unwrap();
}

#[test]
fn commerce_source_payload_rejects_invalid_rows() {
    for raw in [
        r#"{"kind":"commerce_order","id":"","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"SO-1","status":"draft"}"#,
        r#"{"kind":"commerce_order","id":"sales-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":0,"title":"SO-1","status":"draft"}"#,
        r#"{"kind":"commerce_order","id":"sales-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"","status":"draft"}"#,
        r#"{"kind":"commerce_order","id":"sales-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"SO-1","status":""}"#,
        r#"{"kind":"commerce_order","id":"sales-1","company_id":"","branch_id":"branch-1","department_id":null,"version":1,"title":"SO-1","status":"draft"}"#,
        r#"{"kind":"unknown","id":"sales-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"SO-1","status":"draft"}"#,
        r#"{"kind":"commerce_order","id":"sales-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"SO-1\n","status":"draft"}"#,
    ] {
        assert!(
            serde_json::from_str::<SourceRecord>(raw).is_err(),
            "invalid payload was accepted: {raw}"
        );
    }
}
