//! Behavioral parity test for the native CRM collaboration-lead read.
//!
//! The crate manifest has `autotests = false`; the lead must register this file
//! as a `[[test]]` target named `crm_collaboration_lead` before `cargo test`
//! picks it up. No build was run by the worker (BUILD FREEZE).
use erpx_server_crm::public::collaboration_lead::{
    CollaborationLeadActor, CollaborationLeadFuture, CollaborationLeadRepository,
    CollaborationLeadRow, LEAD_SQL, crm_collaboration_notification_lead,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

struct Fixture {
    row: Option<CollaborationLeadRow>,
    calls: usize,
}

impl CollaborationLeadRepository for Fixture {
    type Error = BusinessError;

    fn lead<'a>(
        &'a mut self,
        _actor: &'a CollaborationLeadActor,
        _record_id: &'a str,
    ) -> CollaborationLeadFuture<'a, Option<CollaborationLeadRow>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.row.clone()) })
    }
}

fn actor() -> CollaborationLeadActor {
    CollaborationLeadActor {
        user_id: "user-1".to_owned(),
        company_id: CompanyId::new("company").expect("company"),
        permissions: vec!["commerce.read".to_owned()],
        modules: vec!["crm".to_owned()],
        expires_at: 500,
    }
}

fn run<T, E>(future: impl Future<Output = Result<T, E>>) -> Result<T, E> {
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(result) => result,
        std::task::Poll::Pending => panic!("fixture future must be ready"),
    }
}

#[test]
fn native_read_returns_the_exact_typescript_lead_id_payload() {
    let mut fixture = Fixture {
        row: Some(CollaborationLeadRow {
            lead_id: "lead-1".to_owned(),
        }),
        calls: 0,
    };
    let view = run(crm_collaboration_notification_lead(
        &actor(),
        "activity-1",
        &mut fixture,
        TimestampMillis::new(100),
    ))
    .expect("use case")
    .expect("row");
    assert_eq!(fixture.calls, 1);
    assert_eq!(view.lead_id, "lead-1");
    let value = serde_json::to_value(&view).expect("serialize");
    let object = value.as_object().expect("object");
    let mut fields: Vec<&str> = object.keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(fields, vec!["lead_id"]);
    assert_eq!(value["lead_id"], "lead-1");
}

#[test]
fn native_read_returns_null_when_the_activity_has_no_lead() {
    let mut fixture = Fixture {
        row: None,
        calls: 0,
    };
    let view = run(crm_collaboration_notification_lead(
        &actor(),
        "activity-1",
        &mut fixture,
        TimestampMillis::new(100),
    ))
    .expect("use case");
    assert_eq!(fixture.calls, 1);
    assert!(view.is_none());
}

#[test]
fn native_read_fails_closed_before_touching_the_repository() {
    let mut denied = actor();
    denied.permissions.clear();
    let mut fixture = Fixture {
        row: Some(CollaborationLeadRow {
            lead_id: "lead-1".to_owned(),
        }),
        calls: 0,
    };
    assert!(run(crm_collaboration_notification_lead(
        &denied,
        "activity-1",
        &mut fixture,
        TimestampMillis::new(100),
    ))
    .is_err());
    assert_eq!(fixture.calls, 0);
}

#[test]
fn owner_statement_executes_the_exact_retired_owner_read() {
    assert_eq!(
        LEAD_SQL,
        "SELECT lead_id FROM crm_lead_activities WHERE id=$1 AND company_id=$2"
    );
    let upper = LEAD_SQL.to_ascii_uppercase();
    assert!(upper.starts_with("SELECT "));
    for forbidden in ["INSERT", "UPDATE ", "DELETE", "FOR UPDATE", "LOCK TABLE"] {
        assert!(!upper.contains(forbidden), "forbidden {forbidden}");
    }
    assert_eq!(LEAD_SQL.matches('$').count(), 2);
}
