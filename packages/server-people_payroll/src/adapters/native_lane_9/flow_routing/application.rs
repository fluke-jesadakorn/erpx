//! Query ports for the payment-batch family. No SQL lives here.

use super::domain::{PaymentBatch, PaymentBatchDetail};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type BatchFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition injects the Pg-backed implementation; the
/// router transports bounded identities only.
pub trait PaymentBatchRead: Send + Sync {
    fn batches<'a>(&'a self, company: &'a str) -> BatchFuture<'a, Vec<PaymentBatch>>;

    fn by_source<'a>(
        &'a self,
        company: &'a str,
        source: &'a str,
    ) -> BatchFuture<'a, Option<PaymentBatchDetail>>;
}
