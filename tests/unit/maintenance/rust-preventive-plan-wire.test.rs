//! Preventive-plan create/revise wire parity with the retained Zod object:
//! trimmed strings, `leadDays` default, bounded integers, unknown-key stripping
//! and canonical receipt bytes.
use erpx_server_maintenance::public::preventive_plan::*;
use serde_json::{Value, json};

fn failure(error: MaintenanceWireFailure) -> Value {
    if error.invalid_json {
        return json!({"ok": false, "json": false});
    }
    json!({
        "ok": false,
        "validation": true,
        "issues": error.issues.iter().map(|issue| json!({
            "code": issue.code,
            "path": issue.path.iter().map(|path| match path {
                MaintenanceIssuePath::Field(name) => json!(name),
                MaintenanceIssuePath::Index(index) => json!(index),
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn observed(body: &str) -> Value {
    match normalize_preventive_plan_save(body.as_bytes()) {
        Ok(save) => json!({"ok": true, "normalized": save.receipt_input().as_str()}),
        Err(error) => failure(error),
    }
}

fn ok(normalized: &str) -> Value {
    json!({"ok": true, "normalized": normalized})
}

fn bad(code: &str, path: &[&str]) -> Value {
    json!({
        "ok": false,
        "validation": true,
        "issues": [{"code": code, "path": path}],
    })
}

#[test]
fn plan_save_normalization_matches_the_retained_schema() {
    let cases: Vec<(&str, String, Value)> = vec![
        (
            "valid-create-default-lead",
            r#"{"assetId":"asset","assetVersion":1,"title":"  Daily safety inspection  ","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":" America/New_York ","reason":" Recurring inspection ","unknown":"stripped"}"#.into(),
            ok(r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"America/New_York","leadDays":7,"reason":"Recurring inspection"}"#),
        ),
        (
            "valid-revise",
            r#"{"id":"plan","assetId":"asset","assetVersion":2,"title":"Weekly safety inspection","priority":"high","ownerId":"user","intervalDays":7,"firstDueAt":1772868659000,"timeZone":"Asia/Bangkok","leadDays":0,"expectedVersion":5,"reason":"Reviewed weekly interval"}"#.into(),
            ok(r#"{"assetId":"asset","assetVersion":2,"title":"Weekly safety inspection","priority":"high","ownerId":"user","intervalDays":7,"firstDueAt":1772868659000,"timeZone":"Asia/Bangkok","leadDays":0,"expectedVersion":5,"reason":"Reviewed weekly interval"}"#),
        ),
        (
            "priority",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"urgent","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"UTC","reason":"Recurring inspection"}"#.into(),
            bad("invalid_value", &["priority"]),
        ),
        (
            "interval-low",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":0,"firstDueAt":1772868659000,"timeZone":"UTC","reason":"Recurring inspection"}"#.into(),
            bad("too_small", &["intervalDays"]),
        ),
        (
            "interval-high",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":3651,"firstDueAt":1772868659000,"timeZone":"UTC","reason":"Recurring inspection"}"#.into(),
            bad("too_big", &["intervalDays"]),
        ),
        (
            "interval-float",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1.5,"firstDueAt":1772868659000,"timeZone":"UTC","reason":"Recurring inspection"}"#.into(),
            bad("invalid_type", &["intervalDays"]),
        ),
        (
            "first-due-zero",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":0,"timeZone":"UTC","reason":"Recurring inspection"}"#.into(),
            bad("too_small", &["firstDueAt"]),
        ),
        (
            "first-due-unsafe",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":9007199254740992,"timeZone":"UTC","reason":"Recurring inspection"}"#.into(),
            // `z.number().int().positive().safe()` reports the base safe bound
            // and the explicit `.safe()` refinement, exactly as the B0 oracle.
            json!({
                "ok": false,
                "validation": true,
                "issues": [
                    {"code": "too_big", "path": ["firstDueAt"]},
                    {"code": "too_big", "path": ["firstDueAt"]},
                ],
            }),
        ),
        (
            "lead-null",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"UTC","leadDays":null,"reason":"Recurring inspection"}"#.into(),
            bad("invalid_type", &["leadDays"]),
        ),
        (
            "lead-high",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"UTC","leadDays":61,"reason":"Recurring inspection"}"#.into(),
            bad("too_big", &["leadDays"]),
        ),
        (
            "expected-null",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"UTC","expectedVersion":null,"reason":"Recurring inspection"}"#.into(),
            bad("invalid_type", &["expectedVersion"]),
        ),
        (
            "expected-zero",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"UTC","expectedVersion":0,"reason":"Recurring inspection"}"#.into(),
            bad("too_small", &["expectedVersion"]),
        ),
        (
            "time-zone-short",
            r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"U","reason":"Recurring inspection"}"#.into(),
            bad("too_small", &["timeZone"]),
        ),
        (
            "title-short",
            r#"{"assetId":"asset","assetVersion":1,"title":" x ","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"UTC","reason":"Recurring inspection"}"#.into(),
            bad("too_small", &["title"]),
        ),
        (
            "asset-empty",
            r#"{"assetId":"","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"UTC","reason":"Recurring inspection"}"#.into(),
            bad("too_small", &["assetId"]),
        ),
        (
            "multiple",
            r#"{"assetId":"asset","assetVersion":0,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":4000,"firstDueAt":1772868659000,"timeZone":"UTC","leadDays":80,"reason":"x"}"#.into(),
            json!({
                "ok": false,
                "validation": true,
                "issues": [
                    {"code": "too_small", "path": ["assetVersion"]},
                    {"code": "too_big", "path": ["intervalDays"]},
                    {"code": "too_big", "path": ["leadDays"]},
                    {"code": "too_small", "path": ["reason"]},
                ],
            }),
        ),
    ];
    for (id, body, expected) in &cases {
        assert_eq!(&observed(body), expected, "{id}");
    }
    assert_eq!(observed("{"), json!({"ok": false, "json": false}));
    assert_eq!(observed("[]"), bad("invalid_type", &[]));
}

#[test]
fn typed_plan_save_uses_defaults_and_the_canonical_receipt_bytes() {
    let body = r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"Asia/Bangkok","reason":"Recurring inspection"}"#;
    let save = normalize_preventive_plan_save(body.as_bytes()).unwrap();
    assert_eq!(save.input().lead_days, 7);
    assert_eq!(save.input().interval_days, 1);
    assert_eq!(save.input().expected_version, None);
    assert_eq!(
        save.receipt_input().as_str(),
        r#"{"assetId":"asset","assetVersion":1,"title":"Daily safety inspection","priority":"normal","ownerId":"user","intervalDays":1,"firstDueAt":1772868659000,"timeZone":"Asia/Bangkok","leadDays":7,"reason":"Recurring inspection"}"#
    );
}
