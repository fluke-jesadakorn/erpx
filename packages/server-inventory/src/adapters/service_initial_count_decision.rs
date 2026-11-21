//! Ordered Inventory-owned detail for the retained Count review digest.
//! Commerce supplies the already-witnessed SKU and tracking mode.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("INITIAL_COUNT_DECISION_UNIT_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Count review detail changed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountDecisionDetail {
    pub lines: String,
    pub lot_lines: String,
}
pub struct CurrentInitialCountDecisionDetail {
    unit: Unit,
    count: String,
    warehouse: String,
    sku: String,
    tracking: String,
    image: InitialCountDecisionDetail,
}
impl CurrentInitialCountDecisionDetail {
    pub fn detail(&self) -> &InitialCountDecisionDetail {
        &self.image
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.count, &self.warehouse, &self.sku, &self.tracking).await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Locks must be acquired with `lock_initial_count_inventory_sources` before
/// the first snapshot. The caller retains its Commerce master witness as well.
pub async fn read_initial_count_decision_detail(
    c: &mut PgConnection,
    count: &str,
    warehouse: &str,
    sku: &str,
    tracking: &str,
) -> Result<CurrentInitialCountDecisionDetail, Error> {
    if count.is_empty()
        || warehouse.is_empty()
        || sku.is_empty()
        || !["none", "lot", "serial"].contains(&tracking)
    {
        return Err(invalid());
    }
    let unit = unit(c).await?;
    let image = capture(c, count, warehouse, sku, tracking).await?;
    Ok(CurrentInitialCountDecisionDetail {
        unit,
        count: count.into(),
        warehouse: warehouse.into(),
        sku: sku.into(),
        tracking: tracking.into(),
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    count: &str,
    warehouse: &str,
    sku: &str,
    tracking: &str,
) -> Result<InitialCountDecisionDetail, Error> {
    let lines:Option<String>=sqlx::query_scalar(r#"WITH items AS MATERIALIZED (SELECT l.ordinal,row_to_json(q)::text image FROM inventory_count_lines l JOIN LATERAL (SELECT l.*,$3::text sku,$4::text tracking_mode,s.quantity_milli current_quantity_milli,s.value_minor current_value_minor,COALESCE(s.version,0) current_version FROM (SELECT 1) x LEFT JOIN commerce_stock s ON s.warehouse_id=$2 AND s.product_id=l.product_id) q ON TRUE WHERE l.count_id=$1 ORDER BY l.ordinal LIMIT 501), stats AS (SELECT count(*) n,COALESCE(sum(octet_length(image)),0) bytes FROM items) SELECT CASE WHEN n=1 AND bytes+n+2<=2097152 THEN '['||(SELECT string_agg(image,',' ORDER BY ordinal) FROM items)||']' END FROM stats"#).bind(count).bind(warehouse).bind(sku).bind(tracking).fetch_one(&mut *c).await?;
    let lot_lines:Option<String>=sqlx::query_scalar(r#"WITH items AS MATERIALIZED (SELECT l.ordinal,cl.id,row_to_json(q)::text image FROM inventory_count_lot_lines cl JOIN inventory_count_lines l ON l.id=cl.count_line_id JOIN LATERAL (SELECT cl.*,b.quantity_milli current_quantity_milli,b.value_minor current_value_minor,COALESCE(b.version,0) current_version FROM (SELECT 1) x LEFT JOIN inventory_lot_balances b ON b.lot_id=cl.result_lot_id AND b.warehouse_id=$2) q ON TRUE WHERE l.count_id=$1 ORDER BY l.ordinal,cl.id LIMIT 501), stats AS (SELECT count(*) n,COALESCE(sum(octet_length(image)),0) bytes FROM items) SELECT CASE WHEN n<=500 AND bytes+n+2<=2097152 THEN '['||COALESCE((SELECT string_agg(image,',' ORDER BY ordinal,id) FROM items),'')||']' END FROM stats"#).bind(count).bind(warehouse).fetch_one(c).await?;
    Ok(InitialCountDecisionDetail {
        lines: lines.ok_or_else(invalid)?,
        lot_lines: lot_lines.ok_or_else(invalid)?,
    })
}
