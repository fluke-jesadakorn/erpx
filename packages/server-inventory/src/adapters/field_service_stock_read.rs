//! Bounded stock fact for an owner-authorized Service reservation read.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_STOCK_CHANGED",
        "The Service stock source changed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_field_service_stock_read_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE commerce_stock IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
#[derive(Clone, PartialEq, Eq)]
pub struct FieldServiceStockRead {
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub version: i64,
}
pub struct CurrentFieldServiceStockRead {
    unit: (String, i32, String, String),
    data: FieldServiceStockRead,
}
impl CurrentFieldServiceStockRead {
    pub fn data(&self) -> &FieldServiceStockRead {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || stock(c, &self.data.warehouse_id, &self.data.product_id)
                .await?
                .as_ref()
                != Some(&self.data)
        {
            return Err(changed());
        }
        Ok(())
    }
}
async fn stock(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
) -> Result<Option<FieldServiceStockRead>, Error> {
    let row:Option<(i64,i64)>=sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2 FOR SHARE").bind(warehouse).bind(product).fetch_optional(c).await?;
    row.map(|(quantity_milli, version)| {
        if !(0..=9_007_199_254_740_991).contains(&quantity_milli)
            || !(1..=9_007_199_254_740_991).contains(&version)
        {
            return Err(changed());
        }
        Ok(FieldServiceStockRead {
            warehouse_id: warehouse.into(),
            product_id: product.into(),
            quantity_milli,
            version,
        })
    })
    .transpose()
}
pub async fn read_field_service_stock(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
) -> Result<Option<CurrentFieldServiceStockRead>, Error> {
    let unit = unit(c).await?;
    Ok(stock(c, warehouse, product)
        .await?
        .map(|data| CurrentFieldServiceStockRead { unit, data }))
}
