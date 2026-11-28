//! Application port and use case for the native customer-access sources read.
//!
//! The caller supplies a source port; this layer owns validation and the
//! complete-payload invariant. It opens no connection and executes no SQL.
use crate::domain::access::AccessSourceRow;
use crate::domain::access_sources::{AccessSourcesQuery, validate_rows};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type SourcesFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port for the single bounded source read. The concrete PostgreSQL
/// source lives in `adapters`; every call observes the caller-owned unit.
pub trait AccessSourcesSource {
    fn sources<'a>(
        &'a mut self,
        query: &'a AccessSourcesQuery,
    ) -> SourcesFuture<'a, Vec<AccessSourceRow>>;
}

/// Read the approved customer documents for one exact holding scope.
pub async fn customer_access_sources<S: AccessSourcesSource>(
    source: &mut S,
    query: AccessSourcesQuery,
) -> Result<Vec<AccessSourceRow>, BusinessError> {
    query.validate()?;
    validate_rows(source.sources(&query).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::access_sources::unavailable;

    #[derive(Default)]
    struct Fake {
        rows: Vec<AccessSourceRow>,
        calls: usize,
    }

    impl AccessSourcesSource for Fake {
        fn sources<'a>(
            &'a mut self,
            _: &'a AccessSourcesQuery,
        ) -> SourcesFuture<'a, Vec<AccessSourceRow>> {
            self.calls += 1;
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn query() -> AccessSourcesQuery {
        AccessSourcesQuery {
            company_id: "company-1".into(),
            branch_id: None,
            department_id: None,
        }
    }

    fn row(id: &str) -> AccessSourceRow {
        AccessSourceRow {
            id: id.into(),
            number: "SO-1".into(),
            name: "Acme".into(),
            version: 1,
        }
    }

    #[tokio::test]
    async fn returns_the_complete_source_payload() {
        let mut fake = Fake {
            rows: vec![row("a"), row("b")],
            ..Fake::default()
        };
        let rows = customer_access_sources(&mut fake, query()).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].to_json()["id"], "a");
        assert_eq!(fake.calls, 1);
    }

    #[tokio::test]
    async fn rejects_invalid_scope_before_touching_the_source() {
        let mut fake = Fake::default();
        let bad = AccessSourcesQuery {
            company_id: String::new(),
            ..query()
        };
        assert_eq!(
            customer_access_sources(&mut fake, bad).await.unwrap_err().code,
            "CUSTOMER_ACCESS_SOURCES_INVALID"
        );
        assert_eq!(fake.calls, 0);
    }

    #[tokio::test]
    async fn rejects_an_incomplete_payload() {
        let mut fake = Fake {
            rows: vec![AccessSourceRow {
                number: String::new(),
                ..row("a")
            }],
            ..Fake::default()
        };
        assert_eq!(
            customer_access_sources(&mut fake, query()).await.unwrap_err().code,
            unavailable().code
        );
    }
}
