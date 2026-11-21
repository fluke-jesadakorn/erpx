//! Existing Field Service reservation stock scope, without consumption-only
//! active warehouse/product or posting-settings prerequisites.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Eq, PartialEq)]
pub struct FieldServiceStockScopeRequest {
    pub company_id: CompanyId,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
}
pub struct CurrentFieldServiceStockScope {
    request: FieldServiceStockScopeRequest,
    unit: Unit,
}
impl CurrentFieldServiceStockScope {
    pub fn request(&self) -> &FieldServiceStockScopeRequest {
        &self.request
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || !exists(c, &self.request).await? {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_field_service_stock_scope_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_warehouses,commerce_products IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_current_field_service_stock_scope(
    c: &mut PgConnection,
    request: FieldServiceStockScopeRequest,
) -> Result<Option<CurrentFieldServiceStockScope>, Error> {
    let unit = unit(c).await?;
    Ok(exists(c, &request)
        .await?
        .then_some(CurrentFieldServiceStockScope { request, unit }))
}
async fn exists(c: &mut PgConnection, r: &FieldServiceStockScopeRequest) -> Result<bool, Error> {
    // B0 checks the actual warehouse scope and stock, not product company.
    Ok(sqlx::query_scalar::<_,bool>("SELECT true FROM commerce_warehouses w JOIN commerce_stock s ON s.warehouse_id=w.id WHERE w.id=$3 AND w.company_id=$1 AND w.branch_id=$2 AND s.product_id=$4 FOR SHARE OF w,s")
        .bind(r.company_id.as_str()).bind(r.branch_id.as_str()).bind(&r.warehouse_id).bind(&r.product_id).fetch_optional(c).await?.unwrap_or(false))
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
    PersistenceError::new("COMMERCE_FIELD_SERVICE_STOCK_SCOPE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_FIELD_SERVICE_STOCK_SCOPE_CHANGED",
        "The actual Service warehouse/product scope changed.",
    )
    .into()
}
