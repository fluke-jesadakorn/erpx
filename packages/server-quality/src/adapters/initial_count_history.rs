//! Initial physical stock requires absence of all Quality history, including
//! released/cancelled/disposed holds. Reservation's active-hold port is distinct.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

pub struct CurrentInitialCountQualityHistory {
    warehouse: String,
    product: String,
    has_history: bool,
    unit: Unit,
}
impl CurrentInitialCountQualityHistory {
    pub fn warehouse_id(&self) -> &str {
        &self.warehouse
    }
    pub fn product_id(&self) -> &str {
        &self.product
    }
    pub fn has_history(&self) -> bool {
        self.has_history
    }
    /// A source witness, never authorization to initialize or post stock.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.warehouse, &self.product).await? != self.has_history
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Composition must acquire this utility fence before taking its first source
/// snapshot, alongside the other required owner fences in a consistent order.
pub async fn lock_initial_count_quality_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE inventory_quality_holds IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_initial_count_quality_history(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
) -> Result<CurrentInitialCountQualityHistory, Error> {
    let unit = unit(c).await?;
    let has_history = read(c, warehouse, product).await?;
    Ok(CurrentInitialCountQualityHistory {
        warehouse: warehouse.into(),
        product: product.into(),
        has_history,
        unit,
    })
}
async fn read(c: &mut PgConnection, warehouse: &str, product: &str) -> Result<bool, Error> {
    // Preserve the original pair predicate, including foreign-company rows for
    // this pair. Never mask anomalous history by adding an actor scope filter.
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_quality_holds WHERE warehouse_id=$1 AND product_id=$2)")
        .bind(warehouse).bind(product).fetch_one(c).await?)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("QUALITY_INITIAL_COUNT_TRANSACTION_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_INITIAL_COUNT_HISTORY_CHANGED",
        "The original initial-stock Quality history predicate changed.",
    )
    .into()
}
