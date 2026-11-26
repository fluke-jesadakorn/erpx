//! Native CRM contact guest-history read use case.
//!
//! SQL stays in the owner Postgres adapter; the caller's retained serializable
//! Holding transaction is used by composition, which also supplies the current
//! actor, selected scope, `sales.crm` purchase admission, retained Hotel
//! reservation history and both query statements. This application enforces
//! the actor read policy and every owner row before presentation. It never
//! writes a guest link, a stay or a case event.
use crate::domain::guest_history::{
    GuestHistoryActor, GuestHistorySource, GuestHistoryView, validate_guest_history,
};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

pub type GuestHistoryFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Owner read port. The only data crossing it is one scoped contact row and
/// its granted guest-link reservation/stay rows from one caller unit.
pub trait GuestHistoryRepository: Send {
    type Error: From<BusinessError> + Send;

    fn guest_history<'a>(
        &'a mut self,
        actor: &'a GuestHistoryActor,
        contact_id: &'a str,
    ) -> GuestHistoryFuture<'a, GuestHistorySource, Self::Error>;
}

/// Execute one bounded contact guest-history read and return the exact retired
/// TypeScript `{contact, stays, hasMore}` payload.
pub async fn crm_guest_history<R>(
    actor: &GuestHistoryActor,
    contact_id: &str,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<GuestHistoryView, R::Error>
where
    R: GuestHistoryRepository,
{
    actor.require_guest_history(now)?;
    let source = repository.guest_history(actor, contact_id).await?;
    validate_guest_history(contact_id, source).map_err(R::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::guest_history::{GuestContactRow, GuestStayRow};
    use erpx_server_shared::public::CompanyId;

    struct Fake {
        source: GuestHistorySource,
        calls: usize,
    }

    impl GuestHistoryRepository for Fake {
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

    fn source() -> GuestHistorySource {
        GuestHistorySource {
            contact: Some(GuestContactRow {
                id: "contact-1".to_owned(),
                name: "Guest Name".to_owned(),
                account_id: "account-1".to_owned(),
            }),
            stays: vec![GuestStayRow {
                guest_id: "guest-1".to_owned(),
                reservation_id: "reservation-1".to_owned(),
                reservation_state: Some("confirmed".to_owned()),
                stay_id: None,
                stay_state: None,
                arrival: None,
                departure: None,
                property_id: "property-1".to_owned(),
            }],
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
    fn use_case_reads_once_and_returns_the_complete_owner_payload() {
        let mut repo = Fake {
            source: source(),
            calls: 0,
        };
        let view = run(crm_guest_history(
            &actor(),
            "contact-1",
            &mut repo,
            TimestampMillis::new(100),
        ))
        .expect("use case");
        assert_eq!(repo.calls, 1);
        assert_eq!(view.contact.id, "contact-1");
        assert_eq!(view.stays.len(), 1);
        assert!(!view.has_more);
    }

    #[test]
    fn use_case_fails_closed_before_touching_the_repository() {
        let mut denied = actor();
        denied.permissions.clear();
        let mut repo = Fake {
            source: source(),
            calls: 0,
        };
        assert!(run(crm_guest_history(
            &denied,
            "contact-1",
            &mut repo,
            TimestampMillis::new(100),
        ))
        .is_err());
        assert_eq!(repo.calls, 0);
    }
}
