//! Behavioral parity test for the native CRM lead case-event history read.
//!
//! The crate manifest has `autotests = false`; the lead must register this file
//! as a `[[test]]` target named `crm_case_event_history` before `cargo test`
//! picks it up. No build was run by the worker (BUILD FREEZE).
use erpx_server_crm::public::crm_case_event_history::{
    CrmCaseEventHistoryFuture, CrmCaseEventHistoryRepository, CrmCaseEventRow, CrmHistoryActor,
    crm_case_event_history,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

struct Fixture {
    rows: Vec<CrmCaseEventRow>,
}

impl CrmCaseEventHistoryRepository for Fixture {
    type Error = BusinessError;
    fn lead_events<'a>(
        &'a mut self,
        _lead_id: &'a str,
    ) -> CrmCaseEventHistoryFuture<'a, Vec<CrmCaseEventRow>, Self::Error> {
        Box::pin(async move { Ok(self.rows.clone()) })
    }
}

fn actor() -> CrmHistoryActor {
    CrmHistoryActor {
        user_id: "user-1".to_owned(),
        company_id: CompanyId::new("company").expect("company"),
        permissions: vec!["commerce.read".to_owned()],
        modules: vec!["crm".to_owned()],
        expires_at: 500,
    }
}

fn event(id: &str, created_at: i64) -> CrmCaseEventRow {
    CrmCaseEventRow {
        id: id.to_owned(),
        lead_id: "lead-1".to_owned(),
        opportunity_id: None,
        version: 2,
        action: "reassign".to_owned(),
        actor_id: "user-1".to_owned(),
        reason: "owner changed".to_owned(),
        snapshot_json: serde_json::json!({"ownerId":"user-2","reason":"owner changed"}),
        created_at,
    }
}

fn run<T, E>(future: impl Future<Output = Result<T, E>>) -> Result<T, E> {
    use std::task::{Context, Poll, Waker};
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("fixture future must be ready"),
    }
}

#[test]
fn owner_history_payload_matches_retired_select_star_shape() {
    let mut fixture = Fixture {
        rows: vec![event("event-2", 20), event("event-1", 10)],
    };
    let rows = run(crm_case_event_history(
        &actor(),
        "lead-1",
        &mut fixture,
        TimestampMillis::new(100),
    ))
    .expect("history");

    let payload = serde_json::to_value(rows).expect("serialize");
    let items = payload.as_array().expect("array");
    assert_eq!(items.len(), 2);
    let mut fields: Vec<&str> = items[0]
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        vec![
            "action",
            "actor_id",
            "created_at",
            "id",
            "lead_id",
            "opportunity_id",
            "reason",
            "snapshot_json",
            "version",
        ]
    );
    assert_eq!(items[0]["id"], "event-2");
    assert_eq!(items[1]["id"], "event-1");
    assert_eq!(items[0]["lead_id"], "lead-1");
    assert_eq!(items[0]["version"], 2);
    assert_eq!(items[0]["action"], "reassign");
    assert_eq!(items[0]["snapshot_json"]["ownerId"], "user-2");
}

#[test]
fn denied_actor_never_reaches_the_owner_source() {
    let mut denied = actor();
    denied.permissions.clear();
    let mut fixture = Fixture {
        rows: vec![event("event-1", 10)],
    };
    assert!(
        run(crm_case_event_history(
            &denied,
            "lead-1",
            &mut fixture,
            TimestampMillis::new(100),
        ))
        .is_err()
    );
}
