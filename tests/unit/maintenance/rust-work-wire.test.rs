use erpx_server_maintenance::public::work_wire::*;
use serde_json::{Value, json};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/maintenance/rust-work/wire.v1.json"
    ))
    .unwrap()
}
fn failure(e: MaintenanceWireFailure) -> Value {
    if e.invalid_json {
        return json!({"ok":false,"json":false});
    }
    json!({"ok":false,"validation":true,"issues":e.issues.iter().map(|i|json!({"code":i.code,"path":i.path.iter().map(|p|match p{MaintenanceIssuePath::Field(s)=>json!(s),MaintenanceIssuePath::Index(n)=>json!(n)}).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
#[test]
fn actual_pinned_b0_schemas_match_ordered_receipt_json_and_validation_issue_order() {
    let f = fixture();
    assert_eq!(f["node"], "v26.11.1");
    assert_eq!(f["zod"]["version"], "4.6.7");
    let rows = f["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 54);
    for r in rows {
        let body = r["body"].as_str().unwrap().as_bytes();
        let result = if r["kind"] == "create" {
            normalize_maintenance_create(body).map(|v| v.receipt_input().as_str().to_owned())
        } else {
            normalize_maintenance_change(body).map(|v| v.receipt_input().as_str().to_owned())
        };
        let observed = match result {
            Ok(s) => json!({"ok":true,"normalized":s}),
            Err(e) => failure(e),
        };
        assert_eq!(observed, r["expected"], "{}", r["id"]);
    }
}
#[test]
fn typed_utf8_storage_never_overwrites_normalized_lone_unit_receipt_identity() {
    let f = fixture();
    let rows = f["cases"].as_array().unwrap();
    let get = |id| {
        let r = rows.iter().find(|r| r["id"] == id).unwrap();
        normalize_maintenance_change(r["body"].as_str().unwrap().as_bytes()).unwrap()
    };
    let high = get("lone-high");
    let low = get("lone-low");
    assert_eq!(high.input().reason, "�ab");
    assert_eq!(high.input().reason, low.input().reason);
    assert_ne!(high.receipt_input(), low.receipt_input());
    assert_eq!(get("js-nel").input().reason, "\u{85}x\u{85}");
    assert_eq!(get("js-trim").input().reason, "trim");
}
