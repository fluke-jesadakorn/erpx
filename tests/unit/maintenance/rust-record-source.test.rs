//! Portable Maintenance record-source shape used by the Collaboration
//! workspace. Database behaviour lives in the PostgreSQL companion test.
use erpx_server_maintenance::public::record_source::CONTRACT_VERSION;
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceRecord};

fn payload(kind: &str, title: &str, version: i64) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "id": "source-1",
        "company_id": "company",
        "branch_id": "branch",
        "department_id": null,
        "version": version,
        "title": title,
        "status": "active",
    })
}

#[test]
fn contract_version_names_the_maintenance_source_facts() {
    assert_eq!(CONTRACT_VERSION, "maintenance.records-source.v1");
}

#[test]
fn plan_and_work_payloads_match_the_portable_contract() {
    for (wire, kind) in [
        ("maintenance_plan", SourceKind::MaintenancePlan),
        ("maintenance_work", SourceKind::MaintenanceWork),
    ] {
        let source: SourceRecord =
            serde_json::from_value(payload(wire, "Synthetic source", 3)).unwrap();
        assert_eq!(source.kind, kind);
        assert_eq!(source.id, "source-1");
        assert_eq!(source.company_id, "company");
        assert_eq!(source.branch_id.as_deref(), Some("branch"));
        assert_eq!(source.department_id, None);
        assert_eq!(source.version, 3);
        assert_eq!(source.title, "Synthetic source");
        assert_eq!(source.status, "active");
        assert_eq!(SourceKind::parse(wire).unwrap(), kind);
    }
}

#[test]
fn invalid_payloads_are_rejected_instead_of_fabricated() {
    assert!(serde_json::from_value::<SourceRecord>(payload("maintenance_plan", "", 3)).is_err());
    assert!(
        serde_json::from_value::<SourceRecord>(payload("maintenance_work", "Title", 0)).is_err()
    );
    assert!(
        serde_json::from_value::<SourceRecord>(payload("staff_task", "Title", 3)).is_ok(),
        "the portable contract itself keeps every owner kind"
    );
}
