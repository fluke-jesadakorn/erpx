//! Exact reserve-part stock phases. This path neither changes physical quantity
//! nor borrows the active-only consumption warehouse/product policy.
use super::*;
use crate::domain::valuation::safe;
use erpx_server_commerce::public::field_service_reservation::{
    CurrentFieldServiceStockScope, FieldServiceStockScopeRequest,
    read_current_field_service_stock_scope,
};
use erpx_server_quality::public::field_service_reservation::{
    CurrentFieldServiceReservationHolds, read_field_service_reservation_holds,
};
use erpx_server_shared::public::CommandKey;

#[derive(Clone, Eq, PartialEq)]
pub struct FieldServiceNewReservationBinding {
    pub owner_key: CommandKey,
    pub order_id: String,
    pub reservation_id: String,
    pub stock: FieldServiceStockScopeRequest,
    pub quantity_milli: i64,
}
/// Service implements with the actual prepared/new reservation owner objects.
/// No default implementation or scalar successful flag can satisfy this port.
pub trait PgFieldServiceNewReservationSource: Send + Sync {
    type Applied: Send + Sync;
    fn binding(&self) -> &FieldServiceNewReservationBinding;
    fn require_prepared(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_inserted(
        &self,
        c: &mut PgConnection,
        applied: &Self::Applied,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
type StockImage = (i64, i64, i64);
pub struct PreparedFieldServiceStockReservation {
    unit: Transaction,
    binding: FieldServiceNewReservationBinding,
}
/// Actual stock-lock effect evidence. Its data comes from the original image,
/// never from the result returned by the reservation trigger.
pub struct LockedFieldServiceStockReservation {
    unit: Transaction,
    binding: FieldServiceNewReservationBinding,
    before: StockImage,
    intermediate: StockImage,
    after: StockImage,
    scope: CurrentFieldServiceStockScope,
    holds: CurrentFieldServiceReservationHolds,
}
/// The caller must acquire reservation-view, stock, Commerce and Quality utility
/// fences before its first Holding SELECT, and retain the actual owner source.
/// Missing stock remains data here; its CONFLICT stays at the original apply.
pub async fn prepare_field_service_stock_reservation<R: PgFieldServiceNewReservationSource>(
    c: &mut PgConnection,
    source: &R,
) -> Result<PreparedFieldServiceStockReservation, Error> {
    require_unit(c).await?;
    let unit = transaction(c).await?;
    source.require_prepared(c).await?;
    let binding = source.binding().clone();
    if binding.quantity_milli <= 0 || binding.quantity_milli > 9_007_199_254_740_991 {
        return Err(changed());
    }
    Ok(PreparedFieldServiceStockReservation { unit, binding })
}
impl PreparedFieldServiceStockReservation {
    /// Original SQL first locks by incrementing version, then checks exact
    /// scoped quantity minus active reservations and ANY active Quality hold.
    pub async fn lock_and_require_available<R: PgFieldServiceNewReservationSource>(
        &self,
        c: &mut PgConnection,
        source: &R,
    ) -> Result<LockedFieldServiceStockReservation, Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.unit || source.binding() != &self.binding {
            return Err(changed());
        }
        source.require_prepared(c).await?;
        let scope = read_current_field_service_stock_scope(c, self.binding.stock.clone()).await?;
        let holds = read_field_service_reservation_holds(
            c,
            &self.binding.stock.warehouse_id,
            &self.binding.stock.product_id,
        )
        .await?;
        let before = stock(c, &self.binding.stock).await?;
        let r = &self.binding.stock;
        sqlx::query(
            "UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=$1 AND product_id=$2",
        )
        .bind(r.warehouse_id.as_str())
        .bind(r.product_id.as_str())
        .execute(&mut *c)
        .await?;
        let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM commerce_stock s WHERE s.warehouse_id=$1 AND s.product_id=$2 AND s.quantity_milli::numeric-COALESCE((SELECT SUM(r.quantity_milli)FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)>=$3::numeric)")
            .bind(r.warehouse_id.as_str()).bind(r.product_id.as_str()).bind(self.binding.quantity_milli).fetch_one(&mut*c).await?;
        if scope.is_none() || holds.is_blocked() || !available {
            return Err(guard_conflict());
        }
        let before = before.ok_or_else(guard_conflict)?;
        let intermediate = (before.0, before.1, safe(before.2 + 1)?);
        let after = (before.0, before.1, safe(before.2 + 2)?);
        if stock(c, r).await? != Some(intermediate) {
            return Err(changed());
        }
        Ok(LockedFieldServiceStockReservation {
            unit: transaction(c).await?,
            binding: self.binding.clone(),
            before,
            intermediate,
            after,
            scope: scope.ok_or_else(guard_conflict)?,
            holds,
        })
    }
    /// After the actual owner insertion, its exact trigger increment must be
    /// verified again immediately before commit along with the owner's receipt.
    pub async fn require_after<R: PgFieldServiceNewReservationSource>(
        &self,
        c: &mut PgConnection,
        source: &R,
        locked: &LockedFieldServiceStockReservation,
        inserted: &R::Applied,
    ) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.unit
            || locked.unit != self.unit
            || locked.binding != self.binding
            || source.binding() != &self.binding
            || locked.intermediate != (locked.before.0, locked.before.1, locked.before.2 + 1)
            || locked.after != (locked.before.0, locked.before.1, locked.before.2 + 2)
        {
            return Err(changed());
        }
        source.require_inserted(c, inserted).await?;
        locked.scope.require_current(c).await?;
        locked.holds.require_current(c).await?;
        if stock(c, &self.binding.stock).await? != Some(locked.after) {
            return Err(changed());
        }
        Ok(())
    }
}
async fn stock(
    c: &mut PgConnection,
    r: &FieldServiceStockScopeRequest,
) -> Result<Option<StockImage>, Error> {
    Ok(sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2 FOR SHARE")
        .bind(r.warehouse_id.as_str()).bind(r.product_id.as_str()).fetch_optional(c).await?)
}
