//! Native CRM collaboration-lead read use case.
//!
//! SQL stays in the owner Postgres adapter; the caller's retained serializable
//! Holding transaction is used by composition, which also supplies the current
//! actor and live work context. This application enforces the actor read policy
//! and the bounded row shape before presentation. It carries no ledger and
//! never writes an activity.
use crate::domain::collaboration_lead::{
    CollaborationLeadActor, CollaborationLeadRow, validate_lead,
};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

pub type CollaborationLeadFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Owner read port. The only data crossing it is the single `lead_id` row for
/// one activity identity on the caller's unit; no SQL text crosses the port.
pub trait CollaborationLeadRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lead<'a>(
        &'a mut self,
        actor: &'a CollaborationLeadActor,
        record_id: &'a str,
    ) -> CollaborationLeadFuture<'a, Option<CollaborationLeadRow>, Self::Error>;
}

/// Execute one bounded collaboration-lead read and return the exact retired
/// TypeScript `{lead_id}` row, or `None` for the retired `null`.
pub async fn crm_collaboration_notification_lead<R>(
    actor: &CollaborationLeadActor,
    record_id: &str,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Option<CollaborationLeadRow>, R::Error>
where
    R: CollaborationLeadRepository,
{
    actor.require_read(now)?;
    let source = repository.lead(actor, record_id).await?;
    validate_lead(record_id, source).map_err(R::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::CompanyId;

    struct Fake {
        row: Option<CollaborationLeadRow>,
        calls: usize,
    }

    impl CollaborationLeadRepository for Fake {
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
    fn use_case_reads_once_and_returns_the_complete_owner_row() {
        let mut repo = Fake {
            row: Some(CollaborationLeadRow {
                lead_id: "lead-1".to_owned(),
            }),
            calls: 0,
        };
        let view = run(crm_collaboration_notification_lead(
            &actor(),
            "activity-1",
            &mut repo,
            TimestampMillis::new(100),
        ))
        .expect("use case");
        assert_eq!(repo.calls, 1);
        assert_eq!(view.expect("row").lead_id, "lead-1");
    }

    #[test]
    fn use_case_fails_closed_before_touching_the_repository() {
        let mut denied = actor();
        denied.permissions.clear();
        let mut repo = Fake {
            row: Some(CollaborationLeadRow {
                lead_id: "lead-1".to_owned(),
            }),
            calls: 0,
        };
        assert!(run(crm_collaboration_notification_lead(
            &denied,
            "activity-1",
            &mut repo,
            TimestampMillis::new(100),
        ))
        .is_err());
        assert_eq!(repo.calls, 0);
    }
}
