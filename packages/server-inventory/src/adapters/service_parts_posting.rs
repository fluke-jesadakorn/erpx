//! Inventory-owned historical image for original Service posting setup.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePartsInventoryHistory {
    pub stock_json: String,
    pub ledger_json: String,
    pub counts_json: String,
    pub consumptions_json: String,
    pub has_history: bool,
}
pub struct CurrentServicePartsInventoryHistory {
    company: CompanyId,
    warehouses: Vec<String>,
    unit: Unit,
    data: ServicePartsInventoryHistory,
}
impl CurrentServicePartsInventoryHistory {
    pub fn data(&self) -> &ServicePartsInventoryHistory {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, &self.warehouses).await? != self.data
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Warehouse IDs must be obtained from and revalidated by the Commerce owner
/// in this same unit. They are data, not a grant of authority.
pub async fn lock_service_parts_inventory_history(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_stock,commerce_stock_moves,inventory_ledger,inventory_counts,inventory_count_lines,operational_inventory_consumptions IN SHARE MODE").execute(c).await?;
    Ok(())
}
pub async fn read_service_parts_inventory_history(
    c: &mut PgConnection,
    company: &CompanyId,
    warehouses: &[String],
) -> Result<CurrentServicePartsInventoryHistory, Error> {
    let bytes = warehouses
        .iter()
        .try_fold(0usize, |n, id| n.checked_add(id.len()))
        .ok_or_else(invalid)?;
    if bytes > 2 * 1024 * 1024 || warehouses.len() > 10000 {
        return Err(invalid());
    }
    let unit = unit(c).await?;
    let data = capture(c, company, warehouses).await?;
    Ok(CurrentServicePartsInventoryHistory {
        company: company.clone(),
        warehouses: warehouses.to_vec(),
        unit,
        data,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    warehouses: &[String],
) -> Result<ServicePartsInventoryHistory, Error> {
    let row=sqlx::query("WITH image AS (SELECT (SELECT jsonb_agg(to_jsonb(s) ORDER BY s.warehouse_id,s.product_id) FROM commerce_stock s WHERE s.warehouse_id=ANY($2::text[])) stock,(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM inventory_ledger l WHERE l.warehouse_id=ANY($2::text[])) ledger,(SELECT jsonb_agg(to_jsonb(n) ORDER BY n.id) FROM inventory_counts n WHERE n.company_id=$1) counts,(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM operational_inventory_consumptions i WHERE i.company_id=$1) consumptions) SELECT CASE WHEN octet_length(coalesce(stock::text,'null'))+octet_length(coalesce(ledger::text,'null'))+octet_length(coalesce(counts::text,'null'))+octet_length(coalesce(consumptions::text,'null'))<=2097152 THEN coalesce(stock::text,'null') END stock,CASE WHEN octet_length(coalesce(stock::text,'null'))+octet_length(coalesce(ledger::text,'null'))+octet_length(coalesce(counts::text,'null'))+octet_length(coalesce(consumptions::text,'null'))<=2097152 THEN coalesce(ledger::text,'null') END ledger,CASE WHEN octet_length(coalesce(stock::text,'null'))+octet_length(coalesce(ledger::text,'null'))+octet_length(coalesce(counts::text,'null'))+octet_length(coalesce(consumptions::text,'null'))<=2097152 THEN coalesce(counts::text,'null') END counts,CASE WHEN octet_length(coalesce(stock::text,'null'))+octet_length(coalesce(ledger::text,'null'))+octet_length(coalesce(counts::text,'null'))+octet_length(coalesce(consumptions::text,'null'))<=2097152 THEN coalesce(consumptions::text,'null') END consumptions,(stock IS NOT NULL OR ledger IS NOT NULL OR EXISTS(SELECT 1 FROM inventory_counts WHERE company_id=$1 AND status='posted') OR consumptions IS NOT NULL) history FROM image").bind(company.as_str()).bind(warehouses).fetch_one(c).await?;
    Ok(ServicePartsInventoryHistory {
        stock_json: row
            .try_get::<Option<String>, _>("stock")?
            .ok_or_else(invalid)?,
        ledger_json: row
            .try_get::<Option<String>, _>("ledger")?
            .ok_or_else(invalid)?,
        counts_json: row
            .try_get::<Option<String>, _>("counts")?
            .ok_or_else(invalid)?,
        consumptions_json: row
            .try_get::<Option<String>, _>("consumptions")?
            .ok_or_else(invalid)?,
        has_history: row.try_get("history")?,
    })
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
fn invalid() -> Error {
    PersistenceError::new("INVENTORY_SERVICE_POSTING_HISTORY_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PARTS_POSTING_SOURCE_CHANGED",
        "The original Inventory history changed.",
    )
    .into()
}

/// Inventory owns the Service initial-count source relation. Its availability
/// is a schema fact only and does not confer Stock authority.
pub struct CurrentServiceInitialStockSchema {
    unit: Unit,
    available: bool,
}
impl CurrentServiceInitialStockSchema {
    pub fn available(&self) -> bool {
        self.available
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || service_initial_stock_schema(c).await? != self.available {
            return Err(changed());
        }
        Ok(())
    }
}
/// Take this utility lock before any transaction snapshot, including when the
/// reviewed relation may not yet be installed.
pub async fn lock_service_initial_stock_schema(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("SAVEPOINT service_initial_stock_schema_lock")
        .execute(&mut *c)
        .await?;
    let locked = sqlx::query("LOCK TABLE inventory_service_initial_physical_sources IN SHARE MODE")
        .execute(&mut *c)
        .await;
    if let Err(error) = locked {
        sqlx::query("ROLLBACK TO SAVEPOINT service_initial_stock_schema_lock")
            .execute(&mut *c)
            .await?;
        sqlx::query("RELEASE SAVEPOINT service_initial_stock_schema_lock")
            .execute(&mut *c)
            .await?;
        if error.as_database_error().and_then(|e| e.code()).as_deref() != Some("42P01") {
            return Err(error.into());
        }
        return Ok(());
    }
    sqlx::query("RELEASE SAVEPOINT service_initial_stock_schema_lock")
        .execute(c)
        .await?;
    Ok(())
}
async fn service_initial_stock_schema(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT to_regclass('inventory_service_initial_physical_sources') IS NOT NULL",
    )
    .fetch_one(c)
    .await?)
}
pub async fn read_service_initial_stock_schema(
    c: &mut PgConnection,
) -> Result<CurrentServiceInitialStockSchema, Error> {
    let unit = unit(c).await?;
    let available = service_initial_stock_schema(c).await?;
    Ok(CurrentServiceInitialStockSchema { unit, available })
}
