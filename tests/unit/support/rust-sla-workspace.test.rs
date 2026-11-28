//! Focused application-level test for the native Support SLA workspace read.
//!
//! Register with a `[[test]] name = "sla_workspace"` entry in
//! `modules/support/server/Cargo.toml` (lead-owned path). The same behaviour is
//! covered by the inline `#[cfg(test)]` modules, which run with `--lib`.
use erpx_support_server::public::sla_workspace::{
    read_sla_workspace, SlaPolicyRow, SlaTicketRow, SlaWorkspaceFuture, SlaWorkspacePorts,
    SlaWorkspaceScope,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

#[derive(Default)]
struct Fake {
    calls: BTreeMap<&'static str, usize>,
}

impl SlaWorkspacePorts for Fake {
    fn now(&self) -> i64 {
        1_793_757_600_000
    }
    fn policies<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<SlaPolicyRow>> {
        *self.calls.entry("policies").or_insert(0) += 1;
        Box::pin(async {
            Ok(vec![SlaPolicyRow {
                id: "support-policy-1".into(),
                name: "Standard support".into(),
                version: 1,
                status: "approved".into(),
                created_by: "creator".into(),
                policy_json: r#"{"calendar":{"timezone":"Asia/Bangkok"},"targetMinutes":60,"pauseWhileWaiting":true}"#.into(),
                actor_eligible: 0,
            }])
        })
    }
    fn tickets<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<SlaTicketRow>> {
        *self.calls.entry("tickets").or_insert(0) += 1;
        Box::pin(async {
            Ok(vec![SlaTicketRow {
                id: "ticket-1".into(),
                title: "Printer offline".into(),
                status: "open".into(),
                owner_id: Some("manager".into()),
                policy_id: Some("support-policy-1".into()),
                policy_version: Some(1),
                clock_json: Some(r#"{"state":"running","remainingMs":3600000,"dueAt":1793757600000,"startedAt":1793754000000,"breachedAt":null,"snapshot":{"targetMinutes":60}}"#.into()),
            }])
        })
    }
    fn alerts<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<Value>> {
        *self.calls.entry("alerts").or_insert(0) += 1;
        Box::pin(async { Ok(vec![json!({"id":"alert-1","can_ack":1,"recipients":2})]) })
    }
}

fn scope() -> SlaWorkspaceScope {
    SlaWorkspaceScope {
        company_id: "company".into(),
        user_id: "reviewer".into(),
        branch_id: None,
        department_id: None,
        support_manage: true,
        support_read: true,
        new_work: true,
        settle_existing: true,
    }
}

#[test]
fn native_sla_workspace_has_full_payload_parity() {
    let mut ports = Fake::default();
    let view = block_on(read_sla_workspace(&mut ports, &scope())).unwrap();
    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(value["alerts"][0]["id"], "alert-1");
    assert_eq!(value["policies"][0]["id"], "support-policy-1");
    assert_eq!(value["policies"][0]["name"], "Standard support");
    assert_eq!(value["policies"][0]["version"], 1);
    assert_eq!(value["policies"][0]["status"], "approved");
    assert_eq!(value["policies"][0]["createdBy"], "creator");
    assert_eq!(value["policies"][0]["policy"]["targetMinutes"], 60);
    assert_eq!(value["policies"][0]["canApprove"], false);
    assert_eq!(value["items"][0]["id"], "ticket-1");
    assert_eq!(value["items"][0]["ownerId"], "manager");
    assert_eq!(value["items"][0]["policyId"], "support-policy-1");
    assert_eq!(value["items"][0]["policyVersion"], 1);
    assert_eq!(value["items"][0]["clock"]["state"], "running");
    assert_eq!(value["canManage"], true);
    assert_eq!(value["canCreate"], true);
    assert_eq!(value["now"], 1_793_757_600_000_i64);
    assert_eq!(ports.calls.get("policies"), Some(&1));
    assert_eq!(ports.calls.get("tickets"), Some(&1));
    assert_eq!(ports.calls.get("alerts"), Some(&1));
}

#[test]
fn native_sla_workspace_denies_a_reader_without_support_read() {
    let mut ports = Fake::default();
    let mut denied = scope();
    denied.support_read = false;
    let error = block_on(read_sla_workspace(&mut ports, &denied)).unwrap_err();
    assert_eq!(error.code, "SUPPORT_READ_REQUIRED");
    assert!(ports.calls.is_empty());
}
