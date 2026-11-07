//! Native Procurement supplier promise history use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application enforces the current actor policy and validates
//! the bounded owner rows before presentation.
use crate::domain::supplier_promise_history::{
    OpenPromiseRow, SupplierPromiseHistory, SupplierPromiseLineRow, SupplierPromiseReadActor,
    SupplierPromiseRow, not_found, validate_history,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use std::future::Future;

pub type SupplierPromiseHistoryFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait SupplierPromiseHistoryRepository: Send {
    type Error: From<BusinessError> + Send;
    fn history<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseRow>, Self::Error>;
    fn lines<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseLineRow>, Self::Error>;
    fn open<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<OpenPromiseRow>, Self::Error>;
}

pub async fn supplier_promise_history<R: SupplierPromiseHistoryRepository>(
    actor: &SupplierPromiseReadActor,
    repository: &mut R,
    order_id: &str,
    now: TimestampMillis,
) -> Result<SupplierPromiseHistory, R::Error> {
    actor.require_read(now)?;
    let order_id = RecordId::new(order_id).map_err(|_| not_found())?;
    let history = repository
        .history(&actor.company_id, order_id.as_str())
        .await?;
    let lines = repository
        .lines(&actor.company_id, order_id.as_str())
        .await?;
    let open = repository
        .open(&actor.company_id, order_id.as_str())
        .await?;
    Ok(validate_history(
        &actor.company_id,
        order_id.as_str(),
        history,
        lines,
        open,
    )?)
}
