//! Owner query port for the lane 33 holding-binding reads. No SQL, no axum.
//!
//! The reader owns the query and the transaction; the router only transports
//! the validated scope and the resulting facts. Keeping this port injected is
//! what makes the HTTP contract testable without a database.

use super::domain::HoldingBinding;
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type HoldingBindingReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait HoldingBindingRead: Send + Sync {
    /// `scope` is the validated resource-slot allow-list from the caller.
    fn by_slug<'a>(
        &'a self,
        scope: &'a [String],
        slug: &'a str,
    ) -> HoldingBindingReadFuture<'a, Option<HoldingBinding>>;

    fn default_binding<'a>(
        &'a self,
        scope: &'a [String],
    ) -> HoldingBindingReadFuture<'a, Option<HoldingBinding>>;

    fn active<'a>(
        &'a self,
        scope: &'a [String],
    ) -> HoldingBindingReadFuture<'a, Vec<HoldingBinding>>;
}

pub type SharedHoldingBindingRead = Arc<dyn HoldingBindingRead>;

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Unavailable,
        "RUNTIME_HOLDING_BINDING_READ_UNAVAILABLE",
        "The native Runtime holding-binding service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_33::domain::BindingQuery;

    struct Fake;

    impl HoldingBindingRead for Fake {
        fn by_slug<'a>(
            &'a self,
            _scope: &'a [String],
            _slug: &'a str,
        ) -> HoldingBindingReadFuture<'a, Option<HoldingBinding>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn default_binding<'a>(
            &'a self,
            _scope: &'a [String],
        ) -> HoldingBindingReadFuture<'a, Option<HoldingBinding>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn active<'a>(
            &'a self,
            _scope: &'a [String],
        ) -> HoldingBindingReadFuture<'a, Vec<HoldingBinding>> {
            Box::pin(async { Err(unavailable()) })
        }
    }

    #[tokio::test]
    async fn reader_port_fails_closed_when_unavailable() {
        let reader = Fake;
        let scope = ["h001".to_owned()];
        let error = reader.by_slug(&scope, "acme").await.unwrap_err();
        assert_eq!(error.code, "RUNTIME_HOLDING_BINDING_READ_UNAVAILABLE");
        assert_eq!(error.kind, erpx_platform_shared_server::public::FailureKind::Unavailable);
        assert!(BindingQuery::parse("default").is_some());
    }
}
