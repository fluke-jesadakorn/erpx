//! Portable Operations staff-task record-source shape used by the
//! Collaboration workspace. Database behaviour lives in the PostgreSQL
//! companion test.
use erpx_server_operations::public::record_source::CONTRACT_VERSION;
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceRecord};

fn payload(title: &str, version: i64) -> serde_json::Value {
    serde_json::json!({
        "kind": "staff_task",
        "id": "task-1",
        "company_id": "company",
        "branch_id": "branch",
        "department_id": null,
        "version": version,
        "title": title,
        "status": "in_progress",
    })
}

#[test]
fn contract_version_names_the_operations_source_facts() {
    assert_eq!(CONTRACT_VERSION, "operations.records-source.v1");
}

#[test]
fn staff_task_payload_matches_the_portable_contract() {
    let source: SourceRecord = serde_json::from_value(payload("Synthetic task", 7)).unwrap();
    assert_eq!(source.kind, SourceKind::StaffTask);
    assert_eq!(source.id, "task-1");
    assert_eq!(source.company_id, "company");
    assert_eq!(source.branch_id.as_deref(), Some("branch"));
    assert_eq!(source.department_id, None);
    assert_eq!(source.version, 7);
    assert_eq!(source.title, "Synthetic task");
    assert_eq!(source.status, "in_progress");
    assert_eq!(
        SourceKind::parse("staff_task").unwrap(),
        SourceKind::StaffTask
    );
}

#[test]
fn invalid_payloads_are_rejected_instead_of_fabricated() {
    assert!(serde_json::from_value::<SourceRecord>(payload("", 7)).is_err());
    assert!(serde_json::from_value::<SourceRecord>(payload("Synthetic task", 0)).is_err());
}
