//! Focused application-level test for the native Support SLA policy point/work
//! reads. Register with a `[[test]] name = "sla_policy_read"` entry in
//! `packages/server-support/Cargo.toml` (lead-owned path). The same behaviour is
//! covered by the inline `#[cfg(test)]` modules, which run with `--lib`.
use erpx_server_support::public::sla_policy_read::{
    read_policy_point, read_policy_work, SlaPolicyReadFuture, SlaPolicyReadPorts, SlaPolicyReadScope,
    SlaPolicyRow,
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

fn scope() -> SlaPolicyReadScope {
    SlaPolicyReadScope {
        company_id: "company".into(),
        user_id: "reviewer".into(),
        support_manage: true,
        support_read: true,
        new_work: true,
    }
}

fn row(status: &str) -> SlaPolicyRow {
    SlaPolicyRow {
        id: "support-policy-1".into(),
        company_id: "company".into(),
        name: "Standard support".into(),
        policy_json: json!({"targetMinutes":60,"pauseWhileWaiting":true}),
        status: status.into(),
        version: 2,
        created_by: "creator".into(),
        approved_by: None,
        approved_at: None,
        created_at: 1_793_757_000_000,
        updated_at: 1_793_757_600_000,
        actor_eligible: 1,
        reviewers: 2,
    }
}

#[derive(Default)]
struct Fake {
    point: Option<SlaPolicyRow>,
    work: Vec<SlaPolicyRow>,
    calls: BTreeMap<&'static str, usize>,
}

impl SlaPolicyReadPorts for Fake {
    fn policy<'a>(&'a mut self, id: &'a str) -> SlaPolicyReadFuture<'a, Option<SlaPolicyRow>> {
        *self.calls.entry("point").or_insert(0) += 1;
        let row = self.point.clone();
        Box::pin(async move { Ok(row) })
    }
    fn work_policies<'a>(&'a mut self) -> SlaPolicyReadFuture<'a, Vec<SlaPolicyRow>> {
        *self.calls.entry("work").or_insert(0) += 1;
        let rows = self.work.clone();
        Box::pin(async move { Ok(rows) })
    }
}

#[test]
fn point_payload_preserves_the_retained_row() {
    let mut ports = Fake {
        point: Some(row("draft")),
        ..Default::default()
    };
    let view = block_on(read_policy_point(&mut ports, &scope(), "support-policy-1")).unwrap();
    let value: Value = serde_json::to_value(&view).unwrap();
    assert_eq!(value["id"], "support-policy-1");
    assert_eq!(value["company_id"], "company");
    assert_eq!(value["name"], "Standard support");
    assert_eq!(value["policy_json"]["targetMinutes"], 60);
    assert_eq!(value["status"], "draft");
    assert_eq!(value["version"], 2);
    assert_eq!(value["created_by"], "creator");
    assert_eq!(value["created_at"], 1_793_757_000_000_i64);
    assert_eq!(value["updated_at"], 1_793_757_600_000_i64);
    assert!(value["approved_by"].is_null());
    assert!(value["approved_at"].is_null());
    assert_eq!(ports.calls["point"], 1);
}

#[test]
fn work_payload_preserves_the_retired_typescript_shape() {
    let mut ports = Fake {
        work: vec![row("draft"), row("approved")],
        ..Default::default()
    };
    let items = block_on(read_policy_work(&mut ports, &scope())).unwrap();
    let value: Value = serde_json::to_value(&items).unwrap();
    assert_eq!(value.as_array().unwrap().len(), 2);
    assert_eq!(value[0]["type"], "support_policy");
    assert_eq!(value[0]["title"], "Resolution-time policy · Standard support");
    assert_eq!(value[0]["detail"], "Review the proposed working calendar and target before use.");
    assert_eq!(value[0]["href"], "/support/sla");
    assert_eq!(value[0]["status"], "draft");
    assert_eq!(value[0]["updatedAt"], 1_793_757_600_000_i64);
    assert_eq!(value[0]["queue"], "todo");
    assert_eq!(value[0]["canAct"], true);
    assert_eq!(value[0]["responsibleQueue"], "Independent Company Support manager");
    assert_eq!(value[0]["nextAction"], "Review or retire the proposed policy");
    assert!(value[0]["blocker"].is_null());
    assert_eq!(value[0]["relationships"][0], "approval");
    assert_eq!(value[1]["queue"], "completed");
    assert_eq!(value[1]["canAct"], false);
    assert_eq!(value[1]["nextAction"], "Read the retained policy snapshot");
    assert_eq!(ports.calls["work"], 1);
}

#[test]
fn gates_and_selectors_fail_closed() {
    let mut ports = Fake::default();
    assert_eq!(
        block_on(read_policy_point(&mut ports, &scope(), ""))
            .unwrap_err()
            .code,
        "SUPPORT_POLICY_ID_INVALID"
    );
    assert_eq!(
        block_on(read_policy_point(&mut ports, &scope(), "x"))
            .unwrap_err()
            .code,
        "SUPPORT_POLICY_NOT_FOUND"
    );
    assert_eq!(ports.calls["point"], 1);

    let mut gated = scope();
    gated.new_work = false;
    let mut ports = Fake {
        work: vec![row("draft")],
        ..Default::default()
    };
    let value: Value = serde_json::to_value(
        block_on(read_policy_work(&mut ports, &gated)).unwrap(),
    )
    .unwrap();
    assert_eq!(value[0]["canAct"], false);
    assert_eq!(value[0]["queue"], "waiting");

    let mut denied = scope();
    denied.support_read = false;
    let mut ports = Fake::default();
    assert!(block_on(read_policy_work(&mut ports, &denied))
        .unwrap()
        .is_empty());
    assert_eq!(ports.calls.get("work"), None);
    assert_eq!(
        block_on(read_policy_point(&mut ports, &denied, "support-policy-1"))
            .unwrap_err()
            .code,
        "SUPPORT_READ_REQUIRED"
    );
}
