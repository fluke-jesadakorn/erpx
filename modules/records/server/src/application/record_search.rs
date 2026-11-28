//! Records reference-search use case.
//!
//! The composition root supplies the owner SQL reader, the cross-owner
//! filters and the current clock. This layer validates scope and assembles the
//! legacy reply from guarded owner rows and eligible principals. It never
//! builds SQL and never reads HTTP.

use crate::domain::record_search::{
    MAX_HITS, PrincipalHit, RecordHit, SearchActor, SearchQuery, SearchResponse,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type SearchFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Reads borrowed on the selected Holding transaction. Owner predicates belong
/// in the candidate query before any result limit is applied.
pub trait RecordSearchRead: Send {
    fn records<'a>(
        &'a mut self,
        actor: &'a SearchActor,
        query: &'a SearchQuery,
    ) -> SearchFuture<'a, Vec<RecordHit>>;

    fn principal_hits<'a>(
        &'a mut self,
        needle: &'a str,
        now: i64,
    ) -> SearchFuture<'a, Vec<PrincipalHit>>;
}

pub async fn execute_search(
    read: &mut impl RecordSearchRead,
    actor: &SearchActor,
    query: &SearchQuery,
    now: i64,
) -> Result<SearchResponse, BusinessError> {
    actor.validate()?;
    if actor.modules.is_empty() {
        return Ok(SearchResponse { hits: Vec::new() });
    }

    let mut hits: Vec<_> = read
        .records(actor, query)
        .await?
        .into_iter()
        .take(MAX_HITS)
        .map(|row| row.to_wire())
        .collect();
    if hits.len() < MAX_HITS {
        let remaining = MAX_HITS - hits.len();
        hits.extend(
            read.principal_hits(&query.needle(), now)
                .await?
                .into_iter()
                .take(remaining)
                .map(|principal| principal.to_wire()),
        );
    }
    Ok(SearchResponse { hits })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::record_search::request_invalid;

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

    #[derive(Default)]
    struct FixtureRead {
        rows: Vec<RecordHit>,
        people: Vec<PrincipalHit>,
        record_reads: usize,
        principal_reads: usize,
    }
    impl RecordSearchRead for FixtureRead {
        fn records<'a>(
            &'a mut self,
            _actor: &'a SearchActor,
            _query: &'a SearchQuery,
        ) -> SearchFuture<'a, Vec<RecordHit>> {
            self.record_reads += 1;
            let rows = std::mem::take(&mut self.rows);
            Box::pin(async move { Ok(rows) })
        }

        fn principal_hits<'a>(
            &'a mut self,
            _needle: &'a str,
            _now: i64,
        ) -> SearchFuture<'a, Vec<PrincipalHit>> {
            self.principal_reads += 1;
            let people = std::mem::take(&mut self.people);
            Box::pin(async move { Ok(people) })
        }
    }

    fn record(id: String) -> RecordHit {
        RecordHit {
            module_id: "operations".into(),
            id,
            title: "Desk".into(),
            status: "draft".into(),
            version: 1,
        }
    }

    #[tokio::test]
    async fn assembles_records_then_people_in_the_exact_order() {
        let mut read = FixtureRead {
            rows: vec![record("rec-1".into())],
            people: vec![PrincipalHit {
                target_id: "u2".into(),
                label: "Alice".into(),
            }],
            ..Default::default()
        };
        let value = execute_search(&mut read, &actor(), &SearchQuery::new("@Desk").unwrap(), 9)
            .await
            .unwrap()
            .value();
        assert_eq!(value["data"][0]["kind"], "record");
        assert_eq!(value["data"][1]["kind"], "person");
        assert_eq!(value.as_object().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn full_record_results_skip_the_control_lookup() {
        let mut read = FixtureRead {
            rows: (0..20).map(|i| record(format!("rec-{i}"))).collect(),
            ..Default::default()
        };
        let value = execute_search(&mut read, &actor(), &SearchQuery::new("Desk").unwrap(), 9)
            .await
            .unwrap()
            .value();
        assert_eq!(value["data"].as_array().unwrap().len(), MAX_HITS);
        assert_eq!(read.record_reads, 1);
        assert_eq!(read.principal_reads, 0);
    }

    #[tokio::test]
    async fn empty_modules_return_empty_data_without_any_read() {
        let mut read = FixtureRead::default();
        let mut empty = actor();
        empty.modules.clear();
        let value = execute_search(&mut read, &empty, &SearchQuery::new("needle").unwrap(), 9)
            .await
            .unwrap()
            .value();
        assert_eq!(value, serde_json::json!({"data": []}));
        assert_eq!((read.record_reads, read.principal_reads), (0, 0));
    }

    #[tokio::test]
    async fn an_invalid_actor_fails_closed_before_any_read() {
        let mut read = FixtureRead::default();
        let mut bad = actor();
        bad.company_id.clear();
        assert_eq!(
            execute_search(&mut read, &bad, &SearchQuery::new("needle").unwrap(), 9)
                .await
                .unwrap_err()
                .code,
            request_invalid().code
        );
        assert_eq!((read.record_reads, read.principal_reads), (0, 0));
    }
}
