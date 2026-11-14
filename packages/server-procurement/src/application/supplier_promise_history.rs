//! Native Procurement supplier promise history use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application enforces the current actor policy and validates
//! the bounded owner rows before presentation.
use crate::domain::supplier_promise_history::{
    OpenPromiseRow, SupplierPromiseDetail, SupplierPromiseHistory, SupplierPromiseLineRow,
    SupplierPromiseOrderRow, SupplierPromisePurchaseAccess, SupplierPromiseReadActor,
    SupplierPromiseRow, SupplierPromiseSourceLineRow, can_record, detail_lines, not_found,
    validate_history, validate_order,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use std::future::Future;

pub type SupplierPromiseHistoryFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait SupplierPromiseHistoryRepository: Send {
    type Error: From<BusinessError> + Send;
    /// Scoped purchase-order header. `None` means the order is absent from the
    /// current Company/branch/department workspace, which is a not-found.
    fn order<'a>(
        &'a mut self,
        actor: &'a SupplierPromiseReadActor,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Option<SupplierPromiseOrderRow>, Self::Error>;
    /// Commerce order lines joined to the product SKU, before the
    /// outstanding/unconfirmed projection.
    fn source_lines<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseSourceLineRow>, Self::Error>;
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

/// Retained supplier-promise detail read for the Supply route.
///
/// The order header is read first so a missing/foreign/out-of-scope purchase
/// order is the same not-found the retained `order(id)` guard produced, before
/// any promise rows are read. `purchase` is *not* derived here: composition
/// reads the commercial package decision in the same Holding transaction and
/// supplies it. The owner intersects only `settleExisting`, its own IAM
/// `commerce.purchase` fact and the order state.
pub async fn supplier_promise_detail<R: SupplierPromiseHistoryRepository>(
    actor: &SupplierPromiseReadActor,
    repository: &mut R,
    order_id: &str,
    purchase: Option<SupplierPromisePurchaseAccess>,
    now: TimestampMillis,
) -> Result<SupplierPromiseDetail, R::Error> {
    actor.require_read(now)?;
    let order_id = RecordId::new(order_id).map_err(|_| not_found())?;
    let order = repository
        .order(actor, order_id.as_str())
        .await?
        .ok_or_else(not_found)?;
    validate_order(&order, &actor.company_id, order_id.as_str())?;
    let history = repository
        .history(&actor.company_id, order_id.as_str())
        .await?;
    let schedule = repository
        .lines(&actor.company_id, order_id.as_str())
        .await?;
    let open = repository
        .open(&actor.company_id, order_id.as_str())
        .await?;
    let base = validate_history(
        &actor.company_id,
        order_id.as_str(),
        history,
        schedule,
        open,
    )?;
    let source = repository
        .source_lines(&actor.company_id, order_id.as_str())
        .await?;
    let lines = detail_lines(order_id.as_str(), source, &base.open)?;
    let can_record = can_record(actor, &order, purchase.as_ref());
    Ok(SupplierPromiseDetail {
        order,
        lines,
        history: base.history,
        schedule: base.lines,
        open: base.open,
        can_record,
        purchase,
    })
}
