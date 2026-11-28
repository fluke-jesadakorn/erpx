//! Composition-supplied Manufacturing release port.
//!
//! An approved Planning recommendation creates a Manufacturing-owned order in
//! the same Holding transaction. Planning never writes `manufacturing_orders`
//! itself; composition provides an implementation of [`PlanningReleaseFactory`]
//! that borrows the active connection for the duration of one commit.
use crate::domain::scenario::ManufacturingOrderPlan;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::BusinessError;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type ReleaseFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, PgTransactionError<BusinessError>>> + Send + 'a>>;

pub trait PlanningReleasePort: Send {
    fn release<'a>(&'a mut self, plan: &'a ManufacturingOrderPlan) -> ReleaseFuture<'a, ()>;
}

pub trait PlanningReleaseFactory: Send + Sync {
    fn create<'a>(&'a self, connection: &'a mut PgConnection) -> Box<dyn PlanningReleasePort + 'a>;
}
