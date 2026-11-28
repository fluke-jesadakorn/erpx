//! Owner application port and validation for the lane-7 legacy people clock.
//!
//! No SQL is present here: composition injects the owner reader behind the
//! port, and the handler only validates a bounded Company request.

use std::{future::Future, pin::Pin};

use erpx_platform_shared_server::public::BusinessError;

use super::domain::{
    self, AuthorityKind, AuthorityPerson, AuthorityPersonQuery, LegacyPeopleTransition,
    WorkspacePaymentRow,
};

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

/// Owner port for the Company-scoped People payment projection.
pub trait WorkspacePeoplePayments: Send + Sync {
    fn outstanding<'a>(
        &'a self,
        company: &'a str,
    ) -> TransitionFuture<'a, Vec<WorkspacePaymentRow>>;
}

pub async fn read_workspace_people_payments(
    reader: &(dyn WorkspacePeoplePayments + '_),
    company: &str,
) -> Result<Vec<WorkspacePaymentRow>, BusinessError> {
    domain::validate_company(company)?;
    reader.outstanding(company).await
}

/// Owned, validated form of the wire query; no SQL is constructed from it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityQuery {
    pub kind: AuthorityKind,
    pub user_id: String,
    pub person_id: String,
    pub position_id: Option<String>,
    pub appointment_kind: String,
    pub scope_id: String,
    pub self_ceo: bool,
}

impl TryFrom<&AuthorityPersonQuery> for AuthorityQuery {
    type Error = BusinessError;

    fn try_from(query: &AuthorityPersonQuery) -> Result<Self, Self::Error> {
        domain::validate_authority_query(query)?;
        Ok(Self {
            kind: query.kind,
            user_id: query.user_id.clone(),
            person_id: query.person_id.clone(),
            position_id: (!query.position_id.is_empty()).then(|| query.position_id.clone()),
            appointment_kind: query.appointment_kind.clone(),
            scope_id: query.scope_id.clone(),
            self_ceo: query.self_ceo,
        })
    }
}

pub trait AuthorityPersonLookup: Send + Sync {
    fn lookup<'a>(
        &'a self,
        company: &'a str,
        query: AuthorityQuery,
    ) -> TransitionFuture<'a, AuthorityPerson>;
}

pub async fn read_authority_person(
    lookup: &(dyn AuthorityPersonLookup + '_),
    company: &str,
    query: AuthorityQuery,
) -> Result<AuthorityPerson, BusinessError> {
    domain::validate_company(company)?;
    domain::validate_authority_fields(
        query.kind,
        &query.user_id,
        &query.person_id,
        query.position_id.as_deref().unwrap_or(""),
        &query.appointment_kind,
        &query.scope_id,
    )?;
    lookup.lookup(company, query).await
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

    struct StubPayments;
    impl WorkspacePeoplePayments for StubPayments {
        fn outstanding<'a>(
            &'a self,
            company: &'a str,
        ) -> TransitionFuture<'a, Vec<WorkspacePaymentRow>> {
            let company = company.to_owned();
            Box::pin(async move {
                Ok(vec![WorkspacePaymentRow {
                    id: format!("batch-{company}"),
                    source_type: "payroll".to_owned(),
                    source_id: "run-1".to_owned(),
                    currency: "THB".to_owned(),
                    status: "approved".to_owned(),
                    due_date: Some("2026-11-25".to_owned()),
                    amount_minor: 1_500,
                }])
            })
        }
    }

    #[tokio::test]
    async fn payments_handler_validates_and_projects() {
        let rows = read_workspace_people_payments(&StubPayments, "acme")
            .await
            .expect("read");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "batch-acme");
        assert_eq!(rows[0].amount_minor, 1_500);
        assert!(
            read_workspace_people_payments(&StubPayments, "../other")
                .await
                .is_err()
        );
    }

    struct StubAuthority;
    impl AuthorityPersonLookup for StubAuthority {
        fn lookup<'a>(
            &'a self,
            company: &'a str,
            query: AuthorityQuery,
        ) -> TransitionFuture<'a, AuthorityPerson> {
            let company = company.to_owned();
            Box::pin(async move {
                Ok(AuthorityPerson {
                    id: (query.kind == AuthorityKind::Actor).then(|| format!("{company}-person")),
                })
            })
        }
    }

    #[tokio::test]
    async fn authority_handler_validates_and_delegates() {
        let lookup = StubAuthority;
        let query = AuthorityQuery {
            kind: AuthorityKind::Actor,
            user_id: "usr_1".to_owned(),
            person_id: String::new(),
            position_id: None,
            appointment_kind: String::new(),
            scope_id: String::new(),
            self_ceo: false,
        };
        let result = read_authority_person(&lookup, "acme", query.clone())
            .await
            .expect("read");
        assert_eq!(result.id.as_deref(), Some("acme-person"));
        let bad = AuthorityQuery {
            kind: AuthorityKind::Appointee,
            person_id: String::new(),
            ..query
        };
        assert!(read_authority_person(&lookup, "acme", bad).await.is_err());
    }
}
