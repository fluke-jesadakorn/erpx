//! Focused owner test for the native public Marketing form source read.
//!
//! Pins the exact retired `PublicMarketingForm` payload, the source SQL shape
//! and the no-fallback application gate. The lead registers this test target in
//! `packages/server-marketing/Cargo.toml` and mounts the router in composition.

use erpx_server_marketing::public::public_form_source::{
    FormQuery, FormRow, PublicFormFuture, PublicFormSource, SOURCE_KEYS, SOURCE_SQL, read,
};
use erpx_server_runtime::public::http_support::serde_json::Value;
use std::task::{Context, Poll, Waker};

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn row() -> FormRow {
    FormRow {
        id: "form-1".into(),
        company_id: "co-1".into(),
        name: "Contact".into(),
        version: 3,
        preview_json: "{\"headline\":\"Hi\"}".into(),
        configuration_json: "{\"consentRequired\":true}".into(),
    }
}

struct Probe {
    row: Option<FormRow>,
}

impl PublicFormSource for Probe {
    fn form<'a>(&'a self, _: &'a str) -> PublicFormFuture<'a, Option<FormRow>> {
        let row = self.row.clone();
        Box::pin(async move { Ok(row) })
    }
}

#[test]
fn native_source_payload_matches_the_retired_row_field_set() {
    let source = Probe { row: Some(row()) };
    let value = block_on(read(
        &source,
        &FormQuery {
            form_id: "form-1".into(),
        },
    ))
    .unwrap();
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), SOURCE_KEYS.len());
    for key in SOURCE_KEYS {
        assert!(object.contains_key(key), "missing field: {key}");
    }
    assert_eq!(value["id"], "form-1");
    assert_eq!(value["company_id"], "co-1");
    assert_eq!(value["name"], Value::from("Contact"));
    assert_eq!(value["version"], 3);
    assert_eq!(value["preview_json"], "{\"headline\":\"Hi\"}");
    assert_eq!(value["configuration_json"], "{\"consentRequired\":true}");
}

#[test]
fn source_statement_is_owner_local_read_only_and_single_bound() {
    assert_eq!(
        SOURCE_SQL,
        "SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'"
    );
    assert_eq!(SOURCE_SQL.matches('?').count(), 1);
    for banned in [
        "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE", "LOCK TABLE",
    ] {
        assert!(!SOURCE_SQL.contains(banned), "banned fragment: {banned}");
    }
}
