//! Behavioral parity test for the native CRM contact guest-history read.
//!
//! The crate manifest has `autotests = false`; the lead must register this file
//! as a `[[test]]` target named `crm_guest_history` before `cargo test` picks it
//! up. No build was run by the worker (BUILD FREEZE).
use erpx_server_crm::public::guest_history::{
    CONTACT_SQL, STAYS_SQL, GuestHistoryActor, GuestHistoryFuture, GuestHistoryRepository,
    GuestHistorySource, crm_guest_history,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

struct Fixture {
    source: GuestHistorySource,
    calls: usize,
}

impl GuestHistoryRepository for Fixture {
    type Error = BusinessError;

    fn guest_history<'a>(
        &'a mut self,
        _actor: &'a GuestHistoryActor,
        _contact_id: &'a str,
    ) -> GuestHistoryFuture<'a, GuestHistorySource, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.source.clone()) })
    }
}

fn actor() -> GuestHistoryActor {
    GuestHistoryActor {
        user_id: "user-1".to_owned(),
        company_id: CompanyId::new("company").expect("company"),
        branch_id: Some("branch-1".to_owned()),
        department_id: Some("dept-1".to_owned()),
        permissions: vec!["commerce.read".to_owned(), "pms.read".to_owned()],
        modules: vec!["crm".to_owned()],
        crm_purchased: true,
        hotel_history: true,
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
fn native_read_returns_the_complete_typescript_payload() {
    let mut fixture = Fixture {
        source: GuestHistorySource {
            contact: Some(erpx_server_crm::public::guest_history::GuestContactRow {
                id: "contact-1".to_owned(),
                name: "Guest Name".to_owned(),
                account_id: "account-1".to_owned(),
            }),
            stays: vec![erpx_server_crm::public::guest_history::GuestStayRow {
                guest_id: "guest-1".to_owned(),
                reservation_id: "reservation-1".to_owned(),
                reservation_state: Some("confirmed".to_owned()),
                stay_id: Some("stay-1".to_owned()),
                stay_state: Some("checked_out".to_owned()),
                arrival: Some("2026-11-01".to_owned()),
                departure: Some("2026-11-03".to_owned()),
                property_id: "property-1".to_owned(),
            }],
        },
        calls: 0,
    };
    let view = run(crm_guest_history(
        &actor(),
        "contact-1",
        &mut fixture,
        TimestampMillis::new(100),
    ))
    .expect("use case");
    assert_eq!(fixture.calls, 1);
    assert_eq!(view.contact.id, "contact-1");
    assert_eq!(view.stays.len(), 1);
    assert_eq!(view.stays[0].guest_id, "guest-1");
    assert!(!view.has_more);
    let value = serde_json::to_value(&view).expect("serialize");
    assert_eq!(value["contact"]["account_id"], "account-1");
    assert_eq!(value["stays"][0]["reservation_state"], "confirmed");
    assert_eq!(value["stays"][0]["property_id"], "property-1");
    assert_eq!(value["hasMore"], false);
}

#[test]
fn native_read_fails_closed_before_touching_the_repository() {
    let mut denied = actor();
    denied.permissions.clear();
    let mut fixture = Fixture {
        source: GuestHistorySource::default(),
        calls: 0,
    };
    assert!(run(crm_guest_history(
        &denied,
        "contact-1",
        &mut fixture,
        TimestampMillis::new(100),
    ))
    .is_err());
    assert_eq!(fixture.calls, 0);
}

#[test]
fn owner_statements_are_real_bounded_reads_of_the_crm_owner_tables() {
    for sql in [CONTACT_SQL, STAYS_SQL] {
        let upper = sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "FOR UPDATE", "LOCK TABLE"] {
            assert!(!upper.contains(forbidden), "forbidden {forbidden}");
        }
    }
    assert!(CONTACT_SQL.contains("FROM crm_contacts c"));
    assert!(CONTACT_SQL.contains("JOIN crm_accounts acc"));
    assert_eq!(CONTACT_SQL.matches('$').count(), 7);
    assert!(STAYS_SQL.contains("FROM crm_guest_links g"));
    assert!(STAYS_SQL.contains("g.sharing_state='granted'"));
    assert!(STAYS_SQL.ends_with("ORDER BY r.updated_at DESC,s.id LIMIT 201"));
    assert_eq!(STAYS_SQL.matches('$').count(), 4);
}
