//! Inventory-owned planned stock version after a Quality cancellation trigger.
//! Quality attests the actual hold and event; this port never reads Quality
//! tables or authorizes the command, recipient, purchase, or Workflow effects.
use super::quality_stock_source::QualityStockFacts;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualityCancellationStockAction {
    Request,
    Approve,
    Reject,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityCancellationStockFacts {
    pub warehouse_id: String,
    pub product_id: String,
    pub action: QualityCancellationStockAction,
}
/// This must be backed by actual Quality owner before/after witnesses in the
/// same unit. Input facts alone are not proof that a transition occurred.
pub trait PgQualityCancellationStockSource: Sync {
    fn facts(&self) -> &QualityCancellationStockFacts;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(&self, c: &mut PgConnection)
    -> impl Future<Output = Result<(), Error>> + Send;
}
/// Acquire write intent alongside all owner locks before the first snapshot.
/// A positive stock row is then locked FOR UPDATE during preparation.
pub async fn lock_quality_cancellation_stock_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_stock IN ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub struct PreparedQualityCancellationStock<'a, S: PgQualityCancellationStockSource> {
    source: &'a S,
    unit: Unit,
    before: QualityStockFacts,
    after: QualityStockFacts,
}
pub async fn prepare_quality_cancellation_stock<'a, S: PgQualityCancellationStockSource>(
    c: &mut PgConnection,
    source: &'a S,
) -> Result<PreparedQualityCancellationStock<'a, S>, Error> {
    source.require_before(c).await?;
    let unit = unit(c).await?;
    let f = source.facts();
    let before = read(c, &f.warehouse_id, &f.product_id)
        .await?
        .ok_or_else(changed)?;
    let mut after = before.clone();
    after.version = after.version.checked_add(1).ok_or_else(changed)?;
    Ok(PreparedQualityCancellationStock {
        source,
        unit,
        before,
        after,
    })
}
impl<'a, S: PgQualityCancellationStockSource> PreparedQualityCancellationStock<'a, S> {
    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.before.warehouse_id, &self.before.product_id)
                .await?
                .as_ref()
                != Some(&self.before)
        {
            return Err(changed());
        }
        self.source.require_before(c).await
    }
    /// Call only after the matching actual Quality transition and its trigger.
    pub async fn retain(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedQualityCancellationStock<'a, S>, Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        self.source.require_after(c).await?;
        if read(c, &self.after.warehouse_id, &self.after.product_id)
            .await?
            .as_ref()
            != Some(&self.after)
        {
            return Err(changed());
        }
        Ok(RetainedQualityCancellationStock {
            source: self.source,
            unit: self.unit,
            after: self.after,
        })
    }
}
pub struct RetainedQualityCancellationStock<'a, S: PgQualityCancellationStockSource> {
    source: &'a S,
    unit: Unit,
    after: QualityStockFacts,
}
impl<S: PgQualityCancellationStockSource> RetainedQualityCancellationStock<'_, S> {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.after.warehouse_id, &self.after.product_id)
                .await?
                .as_ref()
                != Some(&self.after)
        {
            return Err(changed());
        }
        self.source.require_after(c).await
    }
}
async fn read(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
) -> Result<Option<QualityStockFacts>, Error> {
    let row=sqlx::query("SELECT warehouse_id,product_id,quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2 FOR UPDATE").bind(warehouse).bind(product).fetch_optional(c).await?;
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
    PersistenceError::new("INVENTORY_QUALITY_CANCEL_STOCK_UNIT_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_QUALITY_CANCEL_STOCK_CHANGED",
        "The Quality cancellation stock effect changed.",
    )
    .into()
}
