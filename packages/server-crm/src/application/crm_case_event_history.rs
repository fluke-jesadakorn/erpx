//! Native CRM case-event history read use case.
//!
//! SQL stays in the owner Postgres adapter; the caller's retained serializable
//! Holding transaction is used by composition, which also supplies the current
//! actor, selected scope and `sales.crm` purchase admission. This application
//! enforces the actor read policy, the bounded lead identity and every owner
//! row before presentation. It never writes or amends an immutable event.
use crate::domain::crm_case_event_history::{CrmCaseEventRow, CrmHistoryActor, validate_history};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

pub type CrmCaseEventHistoryFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Owner read port. The only data crossing it is immutable `crm_case_events`
/// rows for one lead; no cross-owner table is read.
pub trait CrmCaseEventHistoryRepository: Send {
    type Error: From<BusinessError> + Send;
    fn lead_events<'a>(
        &'a mut self,
        lead_id: &'a str,
    ) -> CrmCaseEventHistoryFuture<'a, Vec<CrmCaseEventRow>, Self::Error>;
}

/// Execute one bounded lead-history read and return the exact retired
/// TypeScript `SELECT * ... ORDER BY created_at DESC, id DESC` payload.
pub async fn crm_case_event_history<R>(
    actor: &CrmHistoryActor,
    lead_id: &str,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Vec<CrmCaseEventRow>, R::Error>
where
    R: CrmCaseEventHistoryRepository,
{
    actor.require_history(now)?;
    let source = repository.lead_events(lead_id).await?;
    validate_history(lead_id, source).map_err(R::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::crm_case_event_history::CrmCaseEventRow as Row;
    use erpx_server_shared::public::CompanyId;

    struct Fake {
        rows: Vec<Row>,
        calls: usize,
    }

    impl CrmCaseEventHistoryRepository for Fake {
        type Error = BusinessError;
        fn lead_events<'a>(
            &'a mut self,
            _lead_id: &'a str,
        ) -> CrmCaseEventHistoryFuture<'a, Vec<Row>, Self::Error> {
            self.calls += 1;
            Box::pin(async move { Ok(self.rows.clone()) })
        }
    }

    fn actor() -> CrmHistoryActor {
        CrmHistoryActor {
            user_id: "user-1".to_owned(),
            company_id: CompanyId::new("company").expect("company"),
            permissions: vec!["commerce.read".to_owned()],
            modules: Vec::new(),
            expires_at: 200,
        }
    }

    fn row(id: &str, created_at: i64) -> Row {
        Row {
            id: id.to_owned(),
            lead_id: "lead-1".to_owned(),
            opportunity_id: None,
            version: 1,
            action: "create".to_owned(),
            actor_id: "user-1".to_owned(),
            reason: String::new(),
            snapshot_json: serde_json::json!({}),
            created_at,
        }
    }

    #[test]
    fn use_case_reads_once_and_keeps_the_owner_order() {
        let mut repo = Fake {
            rows: vec![row("event-2", 20), row("event-1", 10)],
            calls: 0,
        };
        let current = actor();
        let future =
            crm_case_event_history(&current, "lead-1", &mut repo, TimestampMillis::new(100));
        let rows = futures_executor(future).expect("use case");
        assert_eq!(repo.calls, 1);
        assert_eq!(rows[0].id, "event-2");
        assert_eq!(rows[1].id, "event-1");
    }

    #[test]
    fn use_case_fails_closed_before_touching_the_repository() {
        let mut denied = actor();
        denied.permissions.clear();
        let mut repo = Fake {
            rows: vec![row("event-1", 10)],
            calls: 0,
        };
        let future =
            crm_case_event_history(&denied, "lead-1", &mut repo, TimestampMillis::new(100));
        assert!(futures_executor(future).is_err());
        assert_eq!(repo.calls, 0);
    }

    /// Tiny same-thread executor keeps the unit test independent of a Tokio
    /// runtime; the port future is ready immediately in these tests.
    fn futures_executor<T, E>(future: impl Future<Output = Result<T, E>>) -> Result<T, E> {
        use std::task::{Context, Poll, Waker};
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("test future must be ready"),
        }
    }
}
