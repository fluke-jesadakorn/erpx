//! Commerce-owned order version effect for a supplier promise. These facts do
//! not grant a caller permission or prove a Procurement promise exists.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPLIER_PROMISE_ORDER_CHANGED",
        "The original purchase order changed.",
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
        return Err(PersistenceError::new("SUPPLIER_PROMISE_ORDER_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Call in the enclosing owner lock order, before its first snapshot.
pub async fn lock_supplier_promise_order(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SUPPLIER_PROMISE_ORDER_UNIT_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.commerce_orders IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn held(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation='public.commerce_orders'::regclass)").fetch_one(c).await?)
}
async fn image(
    c: &mut PgConnection,
    company: &str,
    order: &str,
) -> Result<Option<(i64, String)>, Error> {
    Ok(sqlx::query_as("SELECT version,state FROM public.commerce_orders o WHERE id=$1 AND company_id=$2 AND kind='purchase' AND state IN ('confirmed','partially_fulfilled') FOR UPDATE").bind(order).bind(company).fetch_optional(c).await?)
}
/// Opaque source witness. Current IAM, purchase and Procurement proof are the
/// composition's responsibility and cannot be inferred from this witness.
pub struct SupplierPromiseOrder {
    unit: Unit,
    company: String,
    order: String,
    version: i64,
    image: String,
}
pub struct RetainedSupplierPromiseOrder {
    unit: Unit,
    company: String,
    order: String,
    version: i64,
    image: String,
}
pub async fn prepare_supplier_promise_order(
    c: &mut PgConnection,
    company: &CompanyId,
    order: &str,
    version: i64,
) -> Result<SupplierPromiseOrder, Error> {
    if order.is_empty() || version < 1 || !held(c).await? {
        return Err(changed());
    }
    let unit = unit(c).await?;
    let (actual, image) = image(c, company.as_str(), order)
        .await?
        .ok_or_else(changed)?;
    if actual != version {
        return Err(changed());
    }
    Ok(SupplierPromiseOrder {
        unit,
        company: company.as_str().into(),
        order: order.into(),
        version,
        image,
    })
}
impl SupplierPromiseOrder {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || !held(c).await?
            || image(c, &self.company, &self.order).await?
                != Some((self.version, self.image.clone()))
        {
            return Err(changed());
        }
        Ok(())
    }
    /// Advance the captured order version on the original transaction.
    pub async fn advance(
        self,
        c: &mut PgConnection,
        now: i64,
    ) -> Result<RetainedSupplierPromiseOrder, Error> {
        self.require_current(c).await?;
        let next = self.version.checked_add(1).ok_or_else(changed)?;
        let rows=sqlx::query("UPDATE public.commerce_orders SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4 AND kind='purchase' AND state IN ('confirmed','partially_fulfilled')").bind(now).bind(&self.order).bind(&self.company).bind(self.version).execute(&mut *c).await?.rows_affected();
        if rows != 1 {
            return Err(changed());
        }
        let result = RetainedSupplierPromiseOrder {
            unit: self.unit,
            company: self.company,
            order: self.order,
            version: next,
            image: self.image,
        };
        result.require_current(c).await?;
        Ok(result)
    }
}
impl RetainedSupplierPromiseOrder {
    pub fn company(&self) -> &str {
        &self.company
    }
    pub fn order_id(&self) -> &str {
        &self.order
    }
    pub fn version(&self) -> i64 {
        self.version
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || !held(c).await?
            || image(c, &self.company, &self.order).await?
                != Some((self.version, self.image.clone()))
        {
            return Err(changed());
        }
        Ok(())
    }
}
