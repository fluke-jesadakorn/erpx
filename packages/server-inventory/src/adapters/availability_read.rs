//! Current, bounded stock-availability read for one authorized Company and
//! optional branch. Every fact comes from Inventory-owned relations; the caller
//! supplies the already-authorized scope and the owning transaction.
use crate::domain::availability::{AvailabilityRow, MAX_ROWS, validate_row};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn invalid() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_AVAILABILITY_INVALID",
        "The Inventory availability source is unavailable.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Every source relation the projection reads, fenced for the duration of the
/// caller's serializable unit.
pub async fn lock_availability_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE commerce_stock,commerce_products,commerce_warehouses,pos_reservations,\
         inventory_order_reservations,inventory_transit,inventory_transit_lines,\
         inventory_quality_holds,inventory_lot_balances,inventory_lots IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

type Row = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
);

const AVAILABILITY: &str = "SELECT s.warehouse_id,s.product_id,p.sku,p.name,p.unit,p.tracking_mode,\
    w.name warehouse_name,s.quantity_milli,s.value_minor,s.version,\
    COALESCE((SELECT SUM(r.quantity_milli)::bigint FROM inventory_active_reservations r \
      WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,\
    inventory_quarantined_quantity(s.warehouse_id,s.product_id) quarantined_milli,\
    COALESCE((SELECT SUM(tl.quantity_milli-tl.received_milli-tl.lost_milli)::bigint \
      FROM inventory_transit t JOIN inventory_transit_lines tl ON tl.transit_id=t.id \
      WHERE t.status='in_transit' AND t.target_warehouse_id=s.warehouse_id \
        AND tl.product_id=s.product_id),0) in_transit_milli \
    FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id \
    JOIN commerce_warehouses w ON w.id=s.warehouse_id \
    WHERE w.company_id=$1 AND ($2::text IS NULL OR w.branch_id=$2) \
    ORDER BY p.sku,w.name,s.product_id,s.warehouse_id LIMIT 1001";

/// Read the current availability projection. `branch` is the already-resolved
/// assignment scope, never a caller label.
pub async fn read_availability(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
) -> Result<Vec<AvailabilityRow>, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 {
        return Err(invalid());
    }
    let rows: Vec<Row> = sqlx::query_as(AVAILABILITY)
        .bind(company)
        .bind(branch)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > MAX_ROWS {
        return Err(invalid());
    }
    rows.into_iter()
        .map(
            |(
                warehouse_id,
                product_id,
                sku,
                name,
                unit,
                tracking_mode,
                warehouse_name,
                quantity_milli,
                value_minor,
                version,
                reserved_milli,
                quarantined_milli,
                in_transit_milli,
            )| {
                validate_row(AvailabilityRow {
                    warehouse_id,
                    product_id,
                    sku,
                    name,
                    unit,
                    tracking_mode,
                    warehouse_name,
                    quantity_milli,
                    reserved_milli,
                    quarantined_milli,
                    in_transit_milli,
                    value_minor,
                    version,
                    available_milli: 0,
                })
                .map_err(Into::into)
            },
        )
        .collect()
}
