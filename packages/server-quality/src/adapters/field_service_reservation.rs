//! Exact original reserve-part ANY active Quality hold predicate. Issue-part's
//! quantity/lot/quarantine projection remains separate and unchanged.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
pub struct CurrentFieldServiceReservationHolds {
    warehouse: String,
    product: String,
    blocked: bool,
    unit: Unit,
}
impl CurrentFieldServiceReservationHolds {
    pub fn warehouse_id(&self) -> &str {
        &self.warehouse
    }
    pub fn product_id(&self) -> &str {
        &self.product
    }
    pub fn is_blocked(&self) -> bool {
        self.blocked
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.warehouse, &self.product).await? != self.blocked
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_field_service_reservation_holds(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE inventory_quality_holds IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_field_service_reservation_holds(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
) -> Result<CurrentFieldServiceReservationHolds, Error> {
    let unit = unit(c).await?;
    let blocked = read(c, warehouse, product).await?;
    Ok(CurrentFieldServiceReservationHolds {
        warehouse: warehouse.to_owned(),
        product: product.to_owned(),
        blocked,
        unit,
    })
}
async fn read(c: &mut PgConnection, warehouse: &str, product: &str) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=$1 AND h.product_id=$2 AND h.status NOT IN ('released','cancelled','disposed'))").bind(warehouse).bind(product).fetch_one(c).await?)
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
    PersistenceError::new("QUALITY_FIELD_SERVICE_RESERVATION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_FIELD_SERVICE_RESERVATION_CHANGED",
        "The actual Service reservation Quality hold predicate changed.",
    )
    .into()
}
