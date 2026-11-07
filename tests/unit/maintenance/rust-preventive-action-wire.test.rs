//! Preventive-plan action wire parity against the pinned B0 Zod 4.6.7 schema.
//! Expectations are the observed Node 26.11.1 Fetch/Zod outcomes recorded for
//! this bounded slice; the preventive plan save (IANA timezone, recurrence and
//! asset authority) is not claimed by this wire module.
use erpx_server_maintenance::public::work_wire::*;
use serde_json::{Value, json};

fn failure(e: MaintenanceWireFailure) -> Value {
    if e.invalid_json {
        return json!({"ok": false, "json": false});
    }
    json!({
        "ok": false,
        "validation": true,
        "issues": e.issues.iter().map(|i| json!({
            "code": i.code,
            "path": i.path.iter().map(|p| match p {
                MaintenanceIssuePath::Field(s) => json!(s),
                MaintenanceIssuePath::Index(n) => json!(n),
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn observed(body: &str) -> Value {
    match normalize_maintenance_preventive_action(body.as_bytes()) {
        Ok(v) => json!({"ok": true, "normalized": v.receipt_input().as_str()}),
        Err(e) => failure(e),
    }
}

fn ok(s: &str) -> Value {
    json!({"ok": true, "normalized": s})
}

fn bad(code: &str, path: &[&str]) -> Value {
    json!({
        "ok": false,
        "validation": true,
        "issues": [{"code": code, "path": path}],
    })
}

#[test]
fn preventive_action_wire_matches_pinned_zod_receipt_bytes_and_issue_order() {
    let nel = "\u{85}x\u{85}";
    let long = "x".repeat(2001);
    let cases: Vec<(&str, String, Value)> = vec![
        ("valid", r#"{"action":"activate","expectedVersion":7,"reason":" Synthetic reason "}"#.into(), ok(r#"{"action":"activate","expectedVersion":7,"reason":"Synthetic reason"}"#)),
        ("valid-pause", r#"{"action":"pause","expectedVersion":7,"reason":" Synthetic reason ","unknown":"strip"}"#.into(), ok(r#"{"action":"pause","expectedVersion":7,"reason":"Synthetic reason"}"#)),
        ("valid-generate", r#"{"action":"generate","expectedVersion":3,"reason":" Due generation "}"#.into(), ok(r#"{"action":"generate","expectedVersion":3,"reason":"Due generation"}"#)),
        ("valid-cancel", r#"{"action":"cancel","expectedVersion":1,"reason":" Cancel plan "}"#.into(), ok(r#"{"action":"cancel","expectedVersion":1,"reason":"Cancel plan"}"#)),
        ("unknown", r#"{"action":"invented","expectedVersion":7,"reason":" Synthetic reason "}"#.into(), bad("invalid_value", &["action"])),
        ("missing-action", r#"{"expectedVersion":7,"reason":" Synthetic reason "}"#.into(), bad("invalid_value", &["action"])),
        ("numeric-action", r#"{"action":0,"expectedVersion":7,"reason":" Synthetic reason "}"#.into(), bad("invalid_value", &["action"])),
        ("null-action", r#"{"action":null,"expectedVersion":7,"reason":" Synthetic reason "}"#.into(), bad("invalid_value", &["action"])),
        ("bad-reason", r#"{"action":"activate","expectedVersion":7,"reason":0}"#.into(), bad("invalid_type", &["reason"])),
        ("short", r#"{"action":"activate","expectedVersion":7,"reason":"  x  "}"#.into(), bad("too_small", &["reason"])),
        ("long", format!(r#"{{"action":"activate","expectedVersion":7,"reason":"{long}"}}"#), bad("too_big", &["reason"])),
        ("nel", format!(r#"{{"action":"activate","expectedVersion":7,"reason":"{nel}"}}"#), ok(&format!(r#"{{"action":"activate","expectedVersion":7,"reason":"{nel}"}}"#))),
        ("astral-length", r#"{"action":"activate","expectedVersion":7,"reason":"🙂x"}"#.into(), bad("too_small", &["reason"])),
        ("astral-three", r#"{"action":"activate","expectedVersion":7,"reason":"🙂xy"}"#.into(), ok(r#"{"action":"activate","expectedVersion":7,"reason":"🙂xy"}"#)),
        ("lone-high", r#"{"action":"activate","expectedVersion":7,"reason":"\ud800ab"}"#.into(), ok(r#"{"action":"activate","expectedVersion":7,"reason":"\ud800ab"}"#)),
        ("lone-low", r#"{"action":"activate","expectedVersion":7,"reason":"\udc00ab"}"#.into(), ok(r#"{"action":"activate","expectedVersion":7,"reason":"\udc00ab"}"#)),
        ("js-trim", r#"{"action":"activate","expectedVersion":7,"reason":"﻿ trim "}"#.into(), ok(r#"{"action":"activate","expectedVersion":7,"reason":"trim"}"#)),
        ("float-version", r#"{"action":"activate","expectedVersion":1.5,"reason":" Synthetic reason "}"#.into(), bad("invalid_type", &["expectedVersion"])),
        ("zero-version", r#"{"action":"activate","expectedVersion":0,"reason":" Synthetic reason "}"#.into(), bad("too_small", &["expectedVersion"])),
        ("unsafe-version", r#"{"action":"activate","expectedVersion":9007199254740992,"reason":" Synthetic reason "}"#.into(), bad("too_big", &["expectedVersion"])),
        ("string-version", r#"{"action":"activate","expectedVersion":"7","reason":" Synthetic reason "}"#.into(), bad("invalid_type", &["expectedVersion"])),
        ("missing-expected", r#"{"action":"pause","reason":" Synthetic reason "}"#.into(), bad("invalid_type", &["expectedVersion"])),
        ("missing-reason", r#"{"action":"pause","expectedVersion":7}"#.into(), bad("invalid_type", &["reason"])),
        ("null-body", "null".into(), bad("invalid_type", &[])),
        ("array-body", "[]".into(), bad("invalid_type", &[])),
        ("duplicate-last-valid", r#"{"action":"invented","action":"activate","expectedVersion":7,"reason":"First","reason":" Last "}"#.into(), ok(r#"{"action":"activate","expectedVersion":7,"reason":"Last"}"#)),
        ("bom", "\u{feff}{\"action\":\"pause\",\"expectedVersion\":7,\"reason\":\" Synthetic reason \"}".into(), ok(r#"{"action":"pause","expectedVersion":7,"reason":"Synthetic reason"}"#)),
        ("multi-invalid", r#"{"action":"invented","expectedVersion":0,"reason":"x"}"#.into(), json!({
            "ok": false,
            "validation": true,
            "issues": [
                {"code": "invalid_value", "path": ["action"]},
                {"code": "too_small", "path": ["expectedVersion"]},
                {"code": "too_small", "path": ["reason"]},
            ],
        })),
        ("neg-unsafe-duplicate", r#"{"action":"pause","expectedVersion":-9007199254740992,"reason":" Synthetic reason "}"#.into(), json!({
            "ok": false,
            "validation": true,
            "issues": [
                {"code": "too_small", "path": ["expectedVersion"]},
                {"code": "too_small", "path": ["expectedVersion"]},
            ],
        })),
    ];
    for (id, body, expected) in &cases {
        assert_eq!(&observed(body), expected, "{}", id);
    }
    assert!(observed(r#"{"action":"activate","expectedVersion":7,"reason":"x"}"#).get("validation") == Some(&json!(true)));
    // Malformed JSON is a codec failure, not a validation issue.
    assert_eq!(observed("{\"action\":"), json!({"ok": false, "json": false}));
}

#[test]
fn preventive_lone_units_keep_distinct_receipt_bytes_with_lossy_typed_reason() {
    let high =
        normalize_maintenance_preventive_action(br#"{"action":"pause","expectedVersion":7,"reason":"\ud800ab"}"#)
            .unwrap();
    let low =
        normalize_maintenance_preventive_action(br#"{"action":"pause","expectedVersion":7,"reason":"\udc00ab"}"#)
            .unwrap();
    assert_eq!(high.input().reason, "�ab");
    assert_eq!(high.input().reason, low.input().reason);
    assert_ne!(high.receipt_input(), low.receipt_input());
    assert_eq!(high.input().action.action(), "pause");
    assert_eq!(high.input().expected_version, 7);
}
