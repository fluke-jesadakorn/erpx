//! Inventory-owned stock source for a Quality hold. Commerce owns current
//! product/warehouse status and scope; this port never queries those owners.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityStockFacts {
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
}
pub struct CurrentQualityStock {
    facts: QualityStockFacts,
    unit: Unit,
}
impl CurrentQualityStock {
    pub fn facts(&self) -> &QualityStockFacts {
        &self.facts
    }
    /// Match the pre-effect Quality stock-version assertion. Hold insertion and
    /// cancellation transitions advance this version. Commerce scope,
    /// product/warehouse state and actor authority must be checked separately.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let current = read(c, &self.facts.warehouse_id, &self.facts.product_id).await?;
        if current.as_ref().map(|row| row.version) != Some(self.facts.version) {
            return Err(changed());
        }
        Ok(())
    }
}
/// Utility-only: acquire with all other owner locks before any snapshot.
pub async fn lock_quality_stock_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_stock IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
/// None is absence, never permission to create a new stock row.
pub async fn inspect_quality_stock(
    c: &mut PgConnection,
    warehouse_id: &str,
    product_id: &str,
) -> Result<Option<CurrentQualityStock>, Error> {
    let unit = unit(c).await?;
    Ok(read(c, warehouse_id, product_id)
        .await?
        .map(|facts| CurrentQualityStock { facts, unit }))
}
async fn read(
    c: &mut PgConnection,
    warehouse_id: &str,
    product_id: &str,
) -> Result<Option<QualityStockFacts>, Error> {
    let row=sqlx::query("SELECT warehouse_id,product_id,quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2 FOR SHARE")
        .bind(warehouse_id).bind(product_id).fetch_optional(c).await?;
    row.map(|r| {
        Ok(QualityStockFacts {
            warehouse_id: r.try_get("warehouse_id")?,
            product_id: r.try_get("product_id")?,
            quantity_milli: r.try_get("quantity_milli")?,
            value_minor: r.try_get("value_minor")?,
            version: r.try_get("version")?,
        })
    })
    .transpose()
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
    PersistenceError::new("INVENTORY_QUALITY_STOCK_TRANSACTION_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_QUALITY_STOCK_CHANGED",
        "The original Quality stock version changed.",
    )
    .into()
}
