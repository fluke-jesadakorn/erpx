//! Current, bounded stock-register read for one authorized Company and
//! optional branch. Aggregate stock, per-lot traceability and the Count
//! lifecycle are read from Inventory-owned relations in the caller's
//! serializable transaction; the caller supplies the already-authorized scope.
use crate::domain::stock_register::{
    MAX_COUNT_ROWS, MAX_LOT_ROWS, MAX_STOCK_ROWS, StockRegister, StockRegisterCount,
    StockRegisterLot, StockRegisterRow, validate_count, validate_lot, validate_register,
    validate_row,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn invalid() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_STOCK_REGISTER_INVALID",
        "The Inventory stock register source is unavailable.",
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

/// Every relation the register reads, fenced for the caller's serializable
/// unit before the first Inventory or Finance data statement.
pub async fn lock_stock_register_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        // inventory_active_reservations unions all four reservation sources, so
        // the fence must include Field Service and Manufacturing as well as POS
        // and Sales. Missing either would let a concurrent reservation commit
        // between the register's stock and count reads.
        "LOCK TABLE commerce_stock,commerce_products,commerce_warehouses,pos_reservations,\
         inventory_order_reservations,field_service_part_reservations,\
         manufacturing_material_reservations,inventory_transit,inventory_transit_lines,\
         inventory_quality_holds,inventory_lot_balances,inventory_lots,inventory_counts,\
         inventory_initial_physical_stock_sources,finance_charts,company_money_settings \
         IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

type StockSqlRow = (
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

const STOCK: &str = "SELECT s.warehouse_id,s.product_id,p.sku,p.name,p.unit,p.tracking_mode,\
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

type LotSqlRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    i64,
    i64,
    i64,
    i64,
    Option<String>,
);

const LOTS: &str = "SELECT l.id,b.warehouse_id,l.product_id,p.sku,l.tracking_mode,\
    w.name warehouse_name,l.lot_code,l.serial_number,l.manufactured_date,l.expiry_date,\
    l.status,b.quantity_milli,b.value_minor,b.version,\
    LEAST(b.quantity_milli,COALESCE((SELECT SUM(COALESCE(h.held_quantity_milli,b.quantity_milli)) \
      FROM inventory_quality_holds h WHERE h.lot_id=l.id AND h.warehouse_id=b.warehouse_id \
        AND h.status NOT IN ('released','cancelled','disposed')),0))::bigint quality_hold_milli,\
    (SELECT h.status FROM inventory_quality_holds h WHERE h.lot_id=l.id \
      AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed') \
      ORDER BY h.created_at DESC,h.id DESC LIMIT 1) quality_status \
    FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id \
    JOIN commerce_products p ON p.id=l.product_id \
    JOIN commerce_warehouses w ON w.id=b.warehouse_id \
    WHERE l.company_id=$1 AND ($2::text IS NULL OR w.branch_id=$2) \
    ORDER BY p.sku,l.lot_code NULLS LAST,l.serial_number NULLS LAST,b.warehouse_id,l.id \
    LIMIT 2001";

type CountSqlRow = (
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    String,
    i64,
    Option<i64>,
    Option<String>,
    bool,
    i64,
    i64,
);

const COUNTS: &str = "SELECT c.id,c.number,c.status,c.version,c.count_date,c.reason,\
    c.warehouse_id,w.name warehouse_name,c.net_value_minor,c.posted_at,c.journal_id,\
    EXISTS(SELECT 1 FROM inventory_initial_physical_stock_sources s WHERE s.count_id=c.id) \
      initial_physical_source,c.created_at,c.updated_at \
    FROM inventory_counts c JOIN commerce_warehouses w ON w.id=c.warehouse_id \
      AND w.company_id=c.company_id \
    WHERE c.company_id=$1 AND ($2::text IS NULL OR c.branch_id=$2) \
    ORDER BY c.updated_at DESC,c.id DESC LIMIT 201";

async fn currency(c: &mut PgConnection, company: &str) -> Result<String, Error> {
    let chart: Option<String> = sqlx::query_scalar(
        "SELECT base_currency FROM finance_charts WHERE company_id=$1 AND status='active' ORDER BY id LIMIT 1",
    )
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    if let Some(value) = chart {
        return Ok(value);
    }
    sqlx::query_scalar("SELECT default_currency FROM company_money_settings WHERE company_id=$1")
        .bind(company)
        .fetch_optional(&mut *c)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unavailable,
                "INVENTORY_STOCK_REGISTER_CURRENCY_REQUIRED",
                "The Inventory stock-register currency is unavailable.",
            )
            .into()
        })
}

/// Read the current stock register. `branch` is the already-resolved
/// assignment scope, never a caller label.
pub async fn read_stock_register(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
) -> Result<StockRegister, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 {
        return Err(invalid());
    }
    let stock_rows: Vec<StockSqlRow> = sqlx::query_as(STOCK)
        .bind(company)
        .bind(branch)
        .fetch_all(&mut *c)
        .await?;
    let lot_rows: Vec<LotSqlRow> = sqlx::query_as(LOTS)
        .bind(company)
        .bind(branch)
        .fetch_all(&mut *c)
        .await?;
    let count_rows: Vec<CountSqlRow> = sqlx::query_as(COUNTS)
        .bind(company)
        .bind(branch)
        .fetch_all(&mut *c)
        .await?;
    if stock_rows.len() > MAX_STOCK_ROWS
        || lot_rows.len() > MAX_LOT_ROWS
        || count_rows.len() > MAX_COUNT_ROWS
    {
        return Err(invalid());
    }
    let currency = currency(c, company).await?;
    let stock = stock_rows
        .into_iter()
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
                validate_row(StockRegisterRow {
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
        .collect::<Result<Vec<_>, Error>>()?;
    let lots = lot_rows
        .into_iter()
        .map(
            |(
                id,
                warehouse_id,
                product_id,
                sku,
                tracking_mode,
                warehouse_name,
                lot_code,
                serial_number,
                manufactured_date,
                expiry_date,
                status,
                quantity_milli,
                value_minor,
                version,
                quality_hold_milli,
                quality_status,
            )| {
                validate_lot(StockRegisterLot {
                    id,
                    warehouse_id,
                    product_id,
                    sku,
                    tracking_mode,
                    warehouse_name,
                    lot_code,
                    serial_number,
                    manufactured_date,
                    expiry_date,
                    status,
                    quantity_milli,
                    value_minor,
                    version,
                    quality_hold_milli,
                    quality_status,
                })
                .map_err(Into::into)
            },
        )
        .collect::<Result<Vec<_>, Error>>()?;
    let counts = count_rows
        .into_iter()
        .map(
            |(
                id,
                number,
                status,
                version,
                count_date,
                reason,
                warehouse_id,
                warehouse_name,
                net_value_minor,
                posted_at,
                journal_id,
                initial_physical_source,
                created_at,
                updated_at,
            )| {
                validate_count(StockRegisterCount {
                    id,
                    number,
                    status,
                    version,
                    count_date,
                    reason,
                    warehouse_id,
                    warehouse_name,
                    net_value_minor,
                    posted_at,
                    journal_id,
                    initial_physical_source,
                    created_at,
                    updated_at,
                })
                .map_err(Into::into)
            },
        )
        .collect::<Result<Vec<_>, Error>>()?;
    validate_register(stock, lots, counts, currency).map_err(Into::into)
}
