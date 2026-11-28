//! Focused application-level test for the native Support ticket/incident event
//! history reads. Register with a `[[test]] name = "support_event_history"`
//! entry in `modules/support/server/Cargo.toml` (lead-owned path). The same
//! behaviour is covered by the inline `#[cfg(test)]` modules, which run with
//! `--lib`.
use erpx_support_server::public::support_event_history::{
    read_incident_history, read_ticket_history, IncidentEventRow, SupportEventHistoryFuture,
    SupportEventHistoryPorts, SupportEventHistoryScope, TicketEventRow,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

fn block_on<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

fn scope() -> SupportEventHistoryScope {
    SupportEventHistoryScope {
        company_id: "company".into(),
        user_id: "reader".into(),
        support_read: true,
    }
}

fn ticket() -> TicketEventRow {
    TicketEventRow {
        id: "support-ticket-1:1".into(),
        ticket_id: "support-ticket-1".into(),
        version: 1,
        action: "create".into(),
        actor_id: "creator".into(),
        reason: "Reported".into(),
        owner_id: Some("owner".into()),
        due_date: Some("2026-11-30".into()),
        status: "open".into(),
        created_at: 1_793_757_000_000,
        evidence_json: json!({"kind": "report"}),
    }
}

fn incident() -> IncidentEventRow {
    IncidentEventRow {
        id: "support-incident-1:1".into(),
        incident_id: "support-incident-1".into(),
        version: 1,
        action: "create".into(),
        actor_id: "creator".into(),
        reason: "Reported".into(),
        created_at: 1_793_757_000_000,
        evidence_json: json!({"kind": "outage"}),
    }
}

#[derive(Default)]
struct Fake {
    tickets: Vec<TicketEventRow>,
    incidents: Vec<IncidentEventRow>,
    calls: BTreeMap<&'static str, usize>,
}

impl SupportEventHistoryPorts for Fake {
    fn ticket_events<'a>(
        &'a mut self,
        _id: &'a str,
    ) -> SupportEventHistoryFuture<'a, Vec<TicketEventRow>> {
        *self.calls.entry("ticket").or_insert(0) += 1;
        let rows = self.tickets.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn incident_events<'a>(
        &'a mut self,
        _id: &'a str,
    ) -> SupportEventHistoryFuture<'a, Vec<IncidentEventRow>> {
        *self.calls.entry("incident").or_insert(0) += 1;
        let rows = self.incidents.clone();
        Box::pin(async move { Ok(rows) })
    }
}

#[test]
fn native_event_history_has_full_payload_parity() {
    let mut ports = Fake {
        tickets: vec![ticket()],
        incidents: vec![incident()],
        ..Default::default()
    };
    let tickets: Value = serde_json::to_value(
        block_on(read_ticket_history(&mut ports, &scope(), "support-ticket-1")).unwrap(),
    )
    .unwrap();
    assert_eq!(tickets[0]["id"], "support-ticket-1:1");
    assert_eq!(tickets[0]["ticket_id"], "support-ticket-1");
    assert_eq!(tickets[0]["version"], 1);
    assert_eq!(tickets[0]["action"], "create");
    assert_eq!(tickets[0]["actor_id"], "creator");
    assert_eq!(tickets[0]["reason"], "Reported");
    assert_eq!(tickets[0]["owner_id"], "owner");
    assert_eq!(tickets[0]["due_date"], "2026-11-30");
    assert_eq!(tickets[0]["status"], "open");
    assert_eq!(tickets[0]["created_at"], 1_793_757_000_000_i64);
    assert_eq!(tickets[0]["evidence_json"]["kind"], "report");
    assert_eq!(tickets[0].as_object().unwrap().len(), 11);

    let incidents: Value = serde_json::to_value(
        block_on(read_incident_history(&mut ports, &scope(), "support-incident-1")).unwrap(),
    )
    .unwrap();
    assert_eq!(incidents[0]["incident_id"], "support-incident-1");
    assert_eq!(incidents[0]["evidence_json"]["kind"], "outage");
    assert_eq!(incidents[0].as_object().unwrap().len(), 8);
    assert_eq!(ports.calls.get("ticket"), Some(&1));
    assert_eq!(ports.calls.get("incident"), Some(&1));
}

#[test]
fn native_event_history_denies_a_reader_without_support_read() {
    let mut ports = Fake::default();
    let mut denied = scope();
    denied.support_read = false;
    let error = block_on(read_ticket_history(&mut ports, &denied, "support-ticket-1")).unwrap_err();
    assert_eq!(error.code, "SUPPORT_READ_REQUIRED");
    assert!(ports.calls.is_empty());
}
