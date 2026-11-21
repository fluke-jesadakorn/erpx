//! Support record-source payload shape and portable contract validation.
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceRecord};
use erpx_server_support::public::record_source::CONTRACT_VERSION;

#[test]
fn support_source_payload_matches_the_shared_collaboration_contract() {
    let raw = r#"{"kind":"support_ticket","id":"ticket-1","company_id":"company","branch_id":"branch-1","department_id":"department-1","version":2,"title":"Printer offline","status":"open"}"#;
    let record: SourceRecord = serde_json::from_str(raw).expect("owner payload shape");
    assert_eq!(record.kind, SourceKind::SupportTicket);
    assert_eq!(record.kind.as_str(), "support_ticket");
    assert_eq!(record.id, "ticket-1");
    assert_eq!(record.company_id, "company");
    assert_eq!(record.branch_id.as_deref(), Some("branch-1"));
    assert_eq!(record.department_id.as_deref(), Some("department-1"));
    assert_eq!(record.version, 2);
    assert_eq!(record.title, "Printer offline");
    assert_eq!(record.status, "open");
    record.validate().unwrap();
    assert_eq!(CONTRACT_VERSION, "records.source-facts.v1");
}

#[test]
fn support_source_payload_allows_null_branch_and_department() {
    let raw = r#"{"kind":"support_ticket","id":"ticket-2","company_id":"company","branch_id":null,"department_id":null,"version":1,"title":"General request","status":"triage"}"#;
    let record: SourceRecord = serde_json::from_str(raw).expect("owner payload shape");
    assert_eq!(record.branch_id, None);
    assert_eq!(record.department_id, None);
    record.validate().unwrap();
}

#[test]
fn support_source_payload_rejects_invalid_rows() {
    for raw in [
        r#"{"kind":"support_ticket","id":"","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"Printer offline","status":"open"}"#,
        r#"{"kind":"support_ticket","id":"ticket-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":0,"title":"Printer offline","status":"open"}"#,
        r#"{"kind":"support_ticket","id":"ticket-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"","status":"open"}"#,
        r#"{"kind":"support_ticket","id":"ticket-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"Printer offline","status":""}"#,
        r#"{"kind":"unknown","id":"ticket-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"Printer offline","status":"open"}"#,
        r#"{"kind":"support_ticket","id":"ticket-1","company_id":"company","branch_id":"branch-1","department_id":null,"version":1,"title":"Printer\noffline","status":"open"}"#,
    ] {
        assert!(
            serde_json::from_str::<SourceRecord>(raw).is_err(),
            "invalid payload was accepted: {raw}"
        );
    }
}
