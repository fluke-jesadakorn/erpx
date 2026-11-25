//! Owner application port and validation for the lane-7 legacy people clock.
//!
//! No SQL is present here: composition injects the owner reader behind the
//! port, and the handler only validates a bounded Company request.

use std::{future::Future, pin::Pin};

use erpx_server_shared::public::BusinessError;

use super::domain::{self, LegacyPeopleTransition};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionQuery {
    pub company: String,
    pub observed_at_ms: i64,
}

pub type TransitionFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader; the router never sees a connection.
pub trait LegacyPeopleClock: Send + Sync {
    fn next_transition<'a>(
        &'a self,
        query: TransitionQuery,
    ) -> TransitionFuture<'a, LegacyPeopleTransition>;
}

pub async fn read_next_transition(
    clock: &(dyn LegacyPeopleClock + '_),
    company: &str,
    observed_at_ms: i64,
) -> Result<LegacyPeopleTransition, BusinessError> {
    domain::validate_company(company)?;
    if observed_at_ms < 0 {
        return Err(domain::invalid_observed_at());
    }
    clock
        .next_transition(TransitionQuery {
            company: company.to_owned(),
            observed_at_ms,
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct StubClock {
        seen: Mutex<Vec<TransitionQuery>>,
    }

    impl LegacyPeopleClock for StubClock {
        fn next_transition<'a>(
            &'a self,
            query: TransitionQuery,
        ) -> TransitionFuture<'a, LegacyPeopleTransition> {
            let seen = &self.seen;
            Box::pin(async move {
                seen.lock().expect("lock").push(query.clone());
                Ok(LegacyPeopleTransition {
                    deadline: (query.observed_at_ms > 0).then_some(query.observed_at_ms + 1),
                })
            })
        }
    }

    #[tokio::test]
    async fn handler_validates_and_delegates() {
        let clock = StubClock::default();
        let facts = read_next_transition(&clock, "acme", 100)
            .await
            .expect("read");
        assert_eq!(
            facts,
            LegacyPeopleTransition {
                deadline: Some(101)
            }
        );
        assert_eq!(
            clock.seen.lock().expect("lock").as_slice(),
            &[TransitionQuery {
                company: "acme".to_owned(),
                observed_at_ms: 100
            }]
        );
    }

    #[tokio::test]
    async fn handler_rejects_bad_scope_before_reader() {
        let clock = StubClock::default();
        assert!(read_next_transition(&clock, "../other", 100).await.is_err());
        assert!(read_next_transition(&clock, "acme", -1).await.is_err());
        assert!(clock.seen.lock().expect("lock").is_empty());
    }
}
