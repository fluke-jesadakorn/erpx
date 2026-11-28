//! Query port and handler for the lane-32 Workflow failed-metrics read.
//! No SQL is assembled here; the owner postgres adapter owns the statement.

use super::domain::FailedMetrics;
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type FailedMetricsReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner query port. A shared reader can itself be injected, so a router may
/// hold one independently backed reader per Holding.
pub trait FailedMetricsRead: Send + Sync {
    fn failed<'a>(&'a self) -> FailedMetricsReadFuture<'a, i64>;
}

impl FailedMetricsRead for Arc<dyn FailedMetricsRead> {
    fn failed<'a>(&'a self) -> FailedMetricsReadFuture<'a, i64> {
        (**self).failed()
    }
}

/// Bounded query handler. It owns the shape invariant; no caller can supply a
/// statement or widen the scope.
pub struct ReadFailedMetrics<R: FailedMetricsRead>(R);

impl<R: FailedMetricsRead> ReadFailedMetrics<R> {
    pub fn new(reader: R) -> Self {
        Self(reader)
    }

    pub fn reader(&self) -> &R {
        &self.0
    }

    pub async fn execute(&self) -> Result<FailedMetrics, BusinessError> {
        let count = self.0.failed().await?;
        FailedMetrics::new(count)
    }
}
