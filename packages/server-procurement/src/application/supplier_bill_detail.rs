//! Native supplier-bill detail query. Read-only: one owner query owns no
//! transaction boundary and no idempotency key. Composition supplies the actor,
//! the current flow's invoicing user and the purchased-package overlay.

use crate::domain::supplier_bill_detail::{
    Row, SupplierBillDetail, SupplierBillDetailActor, can_review, not_found, validate_bill,
};
use erpx_server_shared::public::{BusinessError, RecordId, TimestampMillis};
use std::{future::Future, pin::Pin};

pub type SupplierBillDetailFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait SupplierBillDetailRepository: Send {
    type Error: From<BusinessError> + Send;
    fn bill<'a>(
        &'a mut self,
        actor: &'a SupplierBillDetailActor,
        bill_id: &'a str,
    ) -> SupplierBillDetailFuture<'a, Option<Row>, Self::Error>;
    fn lines<'a>(
        &'a mut self,
        company: &'a str,
        bill_id: &'a str,
    ) -> SupplierBillDetailFuture<'a, Vec<Row>, Self::Error>;
    fn receipts<'a>(
        &'a mut self,
        bill_id: &'a str,
    ) -> SupplierBillDetailFuture<'a, Vec<Row>, Self::Error>;
    fn events<'a>(
        &'a mut self,
        bill_id: &'a str,
    ) -> SupplierBillDetailFuture<'a, Vec<Row>, Self::Error>;
    fn accounts<'a>(
        &'a mut self,
        company: &'a str,
        chart_id: Option<&'a str>,
    ) -> SupplierBillDetailFuture<'a, Vec<Row>, Self::Error>;
}

/// The complete retained `supplierBillDetail` payload. `purchase` is never
/// derived here; the composition-owned purchased-package fact is passed in.
pub async fn supplier_bill_detail<R: SupplierBillDetailRepository>(
    actor: &SupplierBillDetailActor,
    repository: &mut R,
    bill_id: &str,
    invoicing_user_id: Option<&str>,
    purchase: Option<Row>,
    now: TimestampMillis,
) -> Result<SupplierBillDetail, R::Error> {
    actor.require_read(now)?;
    let id = RecordId::new(bill_id).map_err(|_| not_found())?;
    let id = id.as_str();
    let bill = repository.bill(actor, id).await?.ok_or_else(not_found)?;
    validate_bill(actor, &bill)?;
    let lines = repository.lines(actor.company_id.as_str(), id).await?;
    let receipts = repository.receipts(id).await?;
    let events = repository.events(id).await?;
    let accounts = repository
        .accounts(
            actor.company_id.as_str(),
            bill.get("chart_id").and_then(serde_json::Value::as_str),
        )
        .await?;
    let can_review = can_review(actor, &bill);
    let can_prepare = actor.can_write()
        && invoicing_user_id.is_some_and(|user| user == actor.user_id.as_str());
    Ok(SupplierBillDetail {
        bill,
        lines,
        receipts,
        events,
        accounts,
        can_review,
        can_prepare,
        purchase,
    })
}
