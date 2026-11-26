//! Records reference-search use case.
//!
//! The composition root supplies the owner SQL reader, the cross-owner
//! admission port and the current clock. This layer validates scope, filters
//! owner rows through admission, and assembles the exact legacy reply. It never
//! builds SQL and never reads HTTP.

use crate::domain::record_search::{
    MAX_HITS, PrincipalHit, RecordHit, SearchActor, SearchQuery, SearchResponse,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type SearchFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner-local SQL access to Records-owned `workspace_records` rows. The
/// caller must already be inside the serialised Holding read unit; no commit is
/// performed here.
pub trait RecordSearchSource: Send + Sync {
    fn records<'a>(
        &'a self,
        actor: &'a SearchActor,
        query: &'a SearchQuery,
    ) -> SearchFuture<'a, Vec<RecordHit>>;
}

/// Cross-owner admission supplied by composition. `visible` reproduces the
/// retained per-owner source read scopes with owning public ports; a `false`
/// filters one row while an error fails the whole read closed.
/// `principal_hits` performs the control-side people and active-assignment
/// reads using identity-access public ports. Neither method executes SQL from
/// this crate.
pub trait SearchAdmission: Send + Sync {
    fn visible<'a>(
        &'a self,
        actor: &'a SearchActor,
        hit: &'a RecordHit,
    ) -> SearchFuture<'a, bool>;

    fn principal_hits<'a>(
        &'a self,
        needle: &'a str,
        now: i64,
    ) -> SearchFuture<'a, Vec<PrincipalHit>>;
}

pub struct RecordSearchHandler<S: RecordSearchSource, A: SearchAdmission> {
    source: S,
    admission: A,
}

impl<S: RecordSearchSource, A: SearchAdmission> RecordSearchHandler<S, A> {
    pub fn new(source: S, admission: A) -> Self {
        Self { source, admission }
    }

    pub async fn execute(
        &self,
        actor: &SearchActor,
        query: &SearchQuery,
        now: i64,
    ) -> Result<SearchResponse, BusinessError> {
        actor.validate()?;

        let mut hits = Vec::with_capacity(MAX_HITS);
        for row in self.source.records(actor, query).await? {
            if self.admission.visible(actor, &row).await? {
                hits.push(row.to_wire());
                if hits.len() >= MAX_HITS {
                    break;
                }
            }
        }

        if hits.len() < MAX_HITS {
            for principal in self.admission.principal_hits(&query.needle(), now).await? {
                hits.push(principal.to_wire());
                if hits.len() >= MAX_HITS {
                    break;
                }
            }
        }

        Ok(SearchResponse { hits })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::record_search::{request_invalid, unavailable};

    fn actor() -> SearchActor {
        SearchActor {
            user_id: "u1".into(),
            assignment_id: "a1".into(),
            company_id: "c1".into(),
            branch_id: None,
            department_id: None,
            modules: vec!["operations".into()],
        }
    }

    struct FakeSource {
        rows: Vec<RecordHit>,
    }

    impl RecordSearchSource for FakeSource {
        fn records<'a>(
            &'a self,
            _actor: &'a SearchActor,
            _query: &'a SearchQuery,
        ) -> SearchFuture<'a, Vec<RecordHit>> {
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    struct FakeAdmission {
        deny: bool,
        people: Vec<PrincipalHit>,
    }

    impl SearchAdmission for FakeAdmission {
        fn visible<'a>(
            &'a self,
            _actor: &'a SearchActor,
            _hit: &'a RecordHit,
        ) -> SearchFuture<'a, bool> {
            let deny = self.deny;
            Box::pin(async move { Ok(!deny) })
        }

        fn principal_hits<'a>(
            &'a self,
            _needle: &'a str,
            _now: i64,
        ) -> SearchFuture<'a, Vec<PrincipalHit>> {
            let people = self.people.clone();
            Box::pin(async move { Ok(people) })
        }
    }

    #[tokio::test]
    async fn assembles_records_then_people_in_the_exact_order() {
        let handler = RecordSearchHandler::new(
            FakeSource {
                rows: vec![RecordHit {
                    module_id: "operations".into(),
                    id: "rec-1".into(),
                    title: "Desk".into(),
                    status: "draft".into(),
                    version: 1,
                }],
            },
            FakeAdmission {
                deny: false,
                people: vec![PrincipalHit {
                    target_id: "u2".into(),
                    label: "Alice".into(),
                }],
            },
        );
        let value = handler
            .execute(&actor(), &SearchQuery::new("@Desk").unwrap(), 9)
            .await
            .unwrap()
            .value();
        assert_eq!(value["data"][0]["kind"], "record");
        assert_eq!(value["data"][1]["kind"], "person");
        assert_eq!(value.as_object().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn denied_rows_are_replaced_by_principal_hits() {
        let handler = RecordSearchHandler::new(
            FakeSource {
                rows: (0..20)
                    .map(|i| RecordHit {
                        module_id: "operations".into(),
                        id: format!("rec-{i}"),
                        title: "t".into(),
                        status: "draft".into(),
                        version: 1,
                    })
                    .collect(),
            },
            FakeAdmission {
                deny: true,
                people: vec![PrincipalHit {
                    target_id: "u2".into(),
                    label: "Alice".into(),
                }],
            },
        );
        let value = handler
            .execute(&actor(), &SearchQuery::new("needle").unwrap(), 9)
            .await
            .unwrap()
            .value();
        assert_eq!(value["data"].as_array().unwrap().len(), 1);
        assert_eq!(value["data"][0]["kind"], "person");
    }

    #[tokio::test]
    async fn an_invalid_actor_fails_closed_before_any_read() {
        struct Failing;
        impl RecordSearchSource for Failing {
            fn records<'a>(
                &'a self,
                _actor: &'a SearchActor,
                _query: &'a SearchQuery,
            ) -> SearchFuture<'a, Vec<RecordHit>> {
                Box::pin(async { Err(unavailable()) })
            }
        }
        let handler = RecordSearchHandler::new(
            Failing,
            FakeAdmission {
                deny: false,
                people: vec![],
            },
        );
        let mut bad = actor();
        bad.company_id.clear();
        assert_eq!(
            handler
                .execute(&bad, &SearchQuery::new("needle").unwrap(), 9)
                .await
                .unwrap_err()
                .code,
            request_invalid().code
        );
    }
}
