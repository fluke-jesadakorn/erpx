//! Verified-customer portal support-thread read use case.
//!
//! The caller supplies a source port; this layer owns validation, the identity
//! guard and the source-stability check. It opens no connection and executes no
//! SQL. The concrete PostgreSQL source lives in `adapters`.

use crate::domain::portal_support_threads::{
    identity_required, not_found, source_changed, Thread, ThreadsQuery,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type ThreadsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port for the three reads the use case needs. Every method executes on
/// the same caller-owned connection so the identity guard and both request-id
/// reads observe one snapshot.
pub trait PortalThreadSource {
    fn identity_exists<'a>(
        &'a mut self,
        query: &'a ThreadsQuery,
    ) -> ThreadsFuture<'a, bool>;

    fn request_id<'a>(
        &'a mut self,
        query: &'a ThreadsQuery,
    ) -> ThreadsFuture<'a, Option<String>>;

    fn threads<'a>(&'a mut self, query: &'a ThreadsQuery) -> ThreadsFuture<'a, Vec<Thread>>;
}

/// Read the support threads a verified customer may see for one exact binding.
pub async fn customer_support_threads<S: PortalThreadSource>(
    source: &mut S,
    query: ThreadsQuery,
) -> Result<Vec<Thread>, BusinessError> {
    let query = query.validate()?;
    if !source.identity_exists(&query).await? {
        return Err(identity_required());
    }
    let before = source
        .request_id(&query)
        .await?
        .ok_or_else(not_found)?;
    let threads = source.threads(&query).await?;
    let after = source
        .request_id(&query)
        .await?
        .ok_or_else(not_found)?;
    if before != after {
        return Err(source_changed());
    }
    Ok(threads)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::portal_support_threads::thread_from_columns;

    #[derive(Default)]
    struct Fake {
        exists: bool,
        ids: std::collections::VecDeque<String>,
        threads: Vec<Thread>,
    }
    impl PortalThreadSource for Fake {
        fn identity_exists<'a>(
            &'a mut self,
            _: &'a ThreadsQuery,
        ) -> ThreadsFuture<'a, bool> {
            let value = self.exists;
            Box::pin(async move { Ok(value) })
        }
        fn request_id<'a>(
            &'a mut self,
            _: &'a ThreadsQuery,
        ) -> ThreadsFuture<'a, Option<String>> {
            let value = self.ids.pop_front();
            Box::pin(async move { Ok(value) })
        }
        fn threads<'a>(&'a mut self, _: &'a ThreadsQuery) -> ThreadsFuture<'a, Vec<Thread>> {
            let threads = self.threads.clone();
            Box::pin(async move { Ok(threads) })
        }
    }
    fn query() -> ThreadsQuery {
        ThreadsQuery {
            binding_id: "binding-1".into(),
            user_id: "customer".into(),
            email: "customer@example.invalid".into(),
        }
    }
    fn thread() -> Thread {
        thread_from_columns(
            "request-1".into(),
            1000,
            "note".into(),
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    #[tokio::test]
    async fn returns_threads_when_identity_and_source_are_stable() {
        let mut fake = Fake {
            exists: true,
            ids: vec!["request-1".into(), "request-1".into()].into(),
            threads: vec![thread()],
        };
        let result = customer_support_threads(&mut fake, query()).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].request_id, "request-1");
    }

    #[tokio::test]
    async fn rejects_identity_that_no_longer_matches() {
        let mut fake = Fake::default();
        assert_eq!(
            customer_support_threads(&mut fake, query())
                .await
                .unwrap_err()
                .code,
            "CUSTOMER_PORTAL_IDENTITY_REQUIRED"
        );
    }

    #[tokio::test]
    async fn rejects_a_request_that_disappeared_before_read() {
        let mut fake = Fake {
            exists: true,
            ..Fake::default()
        };
        assert_eq!(
            customer_support_threads(&mut fake, query())
                .await
                .unwrap_err()
                .code,
            "PORTAL_SUPPORT_REQUEST_NOT_FOUND"
        );
    }

    #[tokio::test]
    async fn rejects_a_request_that_changed_between_reads() {
        let mut fake = Fake {
            exists: true,
            ids: vec!["request-1".into(), "request-2".into()].into(),
            threads: vec![thread()],
        };
        assert_eq!(
            customer_support_threads(&mut fake, query())
                .await
                .unwrap_err()
                .code,
            "PORTAL_SUPPORT_SOURCE_CHANGED"
        );
    }

    #[tokio::test]
    async fn rejects_invalid_scope_before_touching_the_source() {
        let mut fake = Fake {
            exists: true,
            ..Fake::default()
        };
        let bad = ThreadsQuery {
            email: "bad".into(),
            ..query()
        };
        assert_eq!(
            customer_support_threads(&mut fake, bad)
                .await
                .unwrap_err()
                .code,
            "PORTAL_SUPPORT_THREADS_INVALID"
        );
    }
}
