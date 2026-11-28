//! Inventory reservation effects for a caller-owned serializable unit.
//! Commerce must independently authorize and lock the selected order/line,
//! retain its source witness and commit its own version and command receipt.
use crate::domain::sales_reservation::{
    SalesReservationRow, plan_sales_reservation_release, validate_sales_reservation_create,
};
use erpx_commerce_server::public::sales_reservation_source::SalesReservationSource;
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

async fn transaction(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(&mut *c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "RESERVATION_SOURCE_CHANGED",
        "The reservation changed. Refresh and retry.",
    )
    .into()
}

/// Locked Inventory rows for one Commerce order line. This is not an order,
/// purchase or actor authorization witness. Only the transaction which obtained
/// it may apply the selected release, after Commerce checks its own authority.
pub struct LockedSalesReservations {
    unit: Unit,
    line_id: String,
    order_id: String,
    company_id: String,
    observed_at: i64,
    rows: Vec<SalesReservationRow>,
}

/// Acquire the common stock/fulfillment fences before the first Holding source
/// read, matching the lock order used by the native sales delivery path.
pub async fn lock_sales_reservation_sources(c: &mut PgConnection) -> Result<(), Error> {
    super::stock_issue::lock_stock_issue_sources(c).await
}

/// Read current rows in the same order as the legacy release query. The DB
/// statement clock, not the application clock, decides active expiry.
pub async fn lock_sales_reservations(
    c: &mut PgConnection,
    source: &SalesReservationSource,
) -> Result<LockedSalesReservations, Error> {
    let unit = transaction(c).await?;
    let fenced: i64 = sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode='ShareRowExclusiveLock' AND relation IN ('commerce_orders'::regclass,'commerce_order_lines'::regclass,'commerce_fulfillments'::regclass,'commerce_fulfillment_lines'::regclass,'commerce_stock'::regclass,'commerce_stock_moves'::regclass,'inventory_lots'::regclass,'inventory_lot_balances'::regclass,'inventory_lot_moves'::regclass,'inventory_fulfillment_lot_allocations'::regclass,'inventory_quality_holds'::regclass,'pos_reservations'::regclass,'inventory_order_reservations'::regclass,'field_service_part_reservations'::regclass,'manufacturing_material_reservations'::regclass)")
        .fetch_one(&mut *c).await?;
    if fenced != 15 {
        return Err(PersistenceError::new("INVENTORY_RESERVATION_FENCE_REQUIRED").into());
    }
    let masters_fenced: i64 = sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('commerce_products'::regclass,'commerce_warehouses'::regclass)")
        .fetch_one(&mut *c).await?;
    if masters_fenced != 2 {
        return Err(PersistenceError::new("INVENTORY_RESERVATION_FENCE_REQUIRED").into());
    }
    source.require_current(c).await?;
    let line_id = source.line_id();
    let result = sqlx::query("SELECT id,quantity_milli,consumed_milli,released_milli,expires_at,created_at,floor(extract(epoch FROM statement_timestamp())*1000)::bigint FROM inventory_order_reservations WHERE order_line_id=$1 AND quantity_milli>consumed_milli+released_milli AND expires_at>floor(extract(epoch FROM statement_timestamp())*1000)::bigint ORDER BY created_at,id FOR UPDATE")
        .bind(line_id).fetch_all(&mut *c).await?;
    // With no rows, the release always fails; the sentinel is never used to
    // decide eligibility. Otherwise this is the exact SELECT statement clock.
    let observed_at = result
        .first()
        .map(|row| row.try_get::<i64, _>(6))
        .transpose()?
        .unwrap_or(0);
    let rows = result
        .into_iter()
        .map(|r| {
            Ok(SalesReservationRow {
                id: r.try_get(0)?,
                quantity_milli: r.try_get(1)?,
                consumed_milli: r.try_get(2)?,
                released_milli: r.try_get(3)?,
                expires_at: r.try_get(4)?,
                created_at: r.try_get(5)?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(LockedSalesReservations {
        unit,
        line_id: line_id.into(),
        order_id: source.order_id().into(),
        company_id: source.company_id().into(),
        observed_at,
        rows,
    })
}

impl LockedSalesReservations {
    /// Insert one reservation after rechecking the current Commerce line and
    /// active warehouse/product source. Actor, purchase, receipt and order CAS
    /// are still mandatory responsibilities of the caller's command unit.
    pub async fn reserve(
        self,
        c: &mut PgConnection,
        source: &SalesReservationSource,
        reservation_id: &str,
        actor_id: &str,
        quantity_milli: i64,
        expires_at: i64,
        reason: &str,
        now: i64,
    ) -> Result<(), Error> {
        if transaction(c).await? != self.unit
            || source.line_id() != self.line_id
            || source.order_id() != self.order_id
            || source.company_id() != self.company_id
            || reservation_id.is_empty()
            || actor_id.is_empty()
            || crate::domain::stock_receipt::js_trim(reason) != reason
            || !(3..=500).contains(&reason.encode_utf16().count())
        {
            return Err(changed());
        }
        source.require_active_stock_scope(c).await?;
        let reserved: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(quantity_milli),0)::bigint FROM inventory_active_reservations WHERE order_line_id=$1")
            .bind(source.line_id()).fetch_one(&mut *c).await?;
        validate_sales_reservation_create(
            quantity_milli,
            source.quantity_milli(),
            source.fulfilled_milli(),
            reserved,
            expires_at,
            now,
        )?;
        let warehouse = source.warehouse_id().ok_or_else(changed)?;
        sqlx::query("INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(reservation_id).bind(source.line_id()).bind(warehouse).bind(source.product_id())
            .bind(quantity_milli).bind(expires_at).bind(actor_id).bind(reason).bind(now)
            .execute(&mut *c).await?;
        source.require_active_stock_scope(c).await?;
        Ok(())
    }

    /// Apply a complete FIFO release or fail without any writes. Commerce must
    /// recheck its source/authority and commit receipt and order version in this
    /// same unit. The caller owns rollback, commit and serialization retries.
    pub async fn release(
        self,
        c: &mut PgConnection,
        source: &SalesReservationSource,
        quantity_milli: i64,
        reason: &str,
    ) -> Result<(), Error> {
        if transaction(c).await? != self.unit
            || source.line_id() != self.line_id
            || source.order_id() != self.order_id
            || source.company_id() != self.company_id
            || crate::domain::stock_receipt::js_trim(reason) != reason
            || !(3..=500).contains(&reason.encode_utf16().count())
        {
            return Err(changed());
        }
        source.require_current(c).await?;
        let plan = plan_sales_reservation_release(&self.rows, quantity_milli, self.observed_at)?;
        for effect in plan {
            let original = self
                .rows
                .iter()
                .find(|row| row.id == effect.id)
                .ok_or_else(changed)?;
            let changed_rows = sqlx::query("UPDATE inventory_order_reservations SET released_milli=released_milli+$1,release_reason=$2 WHERE id=$3 AND order_line_id=$4 AND quantity_milli=$5 AND consumed_milli=$6 AND released_milli=$7 AND expires_at=$8")
                .bind(effect.quantity_milli).bind(reason).bind(&effect.id).bind(&self.line_id)
                .bind(original.quantity_milli).bind(original.consumed_milli).bind(original.released_milli).bind(original.expires_at)
                .execute(&mut *c).await?.rows_affected();
            if changed_rows != 1 {
                return Err(changed());
            }
        }
        source.require_current(c).await?;
        Ok(())
    }
}
