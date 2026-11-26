//! Application port and use case for the native sales review counts read.
//!
//! The caller supplies a source port; this layer owns validation and the
//! complete-payload invariant. It opens no connection and executes no SQL.
use crate::domain::sales_review_counts::{
    ReviewCount, SalesReviewCountsQuery, validate_counts,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type ReviewCountsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port for the single bounded counts read. The concrete PostgreSQL
/// source lives in `adapters`; every call observes the caller-owned unit.
pub trait SalesReviewCountsSource {
    fn review_counts<'a>(
        &'a mut self,
        query: &'a SalesReviewCountsQuery,
    ) -> ReviewCountsFuture<'a, Vec<ReviewCount>>;
}

/// Read the pending identity/duplicate/access review counts for one holding.
pub async fn sales_customer_review_counts<S: SalesReviewCountsSource>(
    source: &mut S,
    query: SalesReviewCountsQuery,
) -> Result<Vec<ReviewCount>, BusinessError> {
    query.validate()?;
    validate_counts(source.review_counts(&query).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sales_review_counts::{REVIEW_KINDS, unavailable};

    #[derive(Default)]
    struct Fake {
        rows: Vec<ReviewCount>,
        calls: usize,
    }

    impl SalesReviewCountsSource for Fake {
        fn review_counts<'a>(
            &'a mut self,
            _: &'a SalesReviewCountsQuery,
        ) -> ReviewCountsFuture<'a, Vec<ReviewCount>> {
            self.calls += 1;
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn query() -> SalesReviewCountsQuery {
        SalesReviewCountsQuery {
            company_id: "company-1".into(),
            branch_id: None,
            department_id: None,
        }
    }

    fn counts() -> Vec<ReviewCount> {
        REVIEW_KINDS
            .iter()
            .enumerate()
            .map(|(n, kind)| ReviewCount {
                kind: (*kind).into(),
                count: n as i64,
            })
            .collect()
    }

    #[tokio::test]
    async fn returns_the_complete_counts_payload() {
        let mut fake = Fake {
            rows: counts(),
            ..Fake::default()
        };
        let rows = sales_customer_review_counts(&mut fake, query()).await.unwrap();
        assert_eq!(fake.calls, 1);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].to_json()["kind"], "identity");
        assert_eq!(rows[2].to_json()["count"], 2);
    }

    #[tokio::test]
    async fn rejects_invalid_scope_before_touching_the_source() {
        let mut fake = Fake::default();
        let bad = SalesReviewCountsQuery {
            company_id: String::new(),
            ..query()
        };
        assert_eq!(
            sales_customer_review_counts(&mut fake, bad)
                .await
                .unwrap_err()
                .code,
            "CUSTOMER_SALES_REVIEW_COUNTS_INVALID"
        );
        assert_eq!(fake.calls, 0);
    }

    #[tokio::test]
    async fn rejects_an_incomplete_payload() {
        let mut fake = Fake {
            rows: counts()[..2].to_vec(),
            ..Fake::default()
        };
        assert_eq!(
            sales_customer_review_counts(&mut fake, query())
                .await
                .unwrap_err()
                .code,
            unavailable().code
        );
    }
}
