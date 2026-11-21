//! Commerce source for supplier promise planning; no IAM or purchase grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
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
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SUPPLIER_PROMISE_SOURCE_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Enabled purchase composition calls this in the global owner order, before
/// the first snapshot. Disabled-mode readers must not call this automatically.
pub async fn lock_supplier_promise_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SUPPLIER_PROMISE_SOURCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE public.commerce_orders,public.commerce_order_lines IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    sqlx::query("LOCK TABLE public.commerce_products IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupplierPromiseSourceMode {
    Disabled,
    Enabled,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierPromiseOrderFacts {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub version: i64,
    pub order_date: String,
    pub due_date: String,
    pub currency: String,
    pub party_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierPromiseLineFacts {
    pub id: String,
    pub quantity_milli: i64,
    pub fulfilled_milli: i64,
}
pub struct CurrentSupplierPromiseSource {
    unit: Unit,
    mode: SupplierPromiseSourceMode,
    branch: Option<String>,
    department: Option<String>,
    order: SupplierPromiseOrderFacts,
    lines: Vec<SupplierPromiseLineFacts>,
}
async fn enabled_fence(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT (SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('public.commerce_orders'::regclass,'public.commerce_order_lines'::regclass))=2 AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation='public.commerce_products'::regclass)").fetch_one(c).await?)
}
/// Reads the same scoped order and product-joined lines used by the retained
/// command. The selected values are facts, not authority or purchase proof.
pub async fn inspect_supplier_promise_source(
    c: &mut PgConnection,
    mode: SupplierPromiseSourceMode,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    order_id: &str,
) -> Result<Option<CurrentSupplierPromiseSource>, Error> {
    let unit = unit(c).await?;
    if mode == SupplierPromiseSourceMode::Enabled && !enabled_fence(c).await? {
        return Err(PersistenceError::new("SUPPLIER_PROMISE_SOURCE_LOCKS_REQUIRED").into());
    }
    let row=sqlx::query("SELECT id,company_id,branch_id,department_id,kind,state,version,order_date,due_date,currency,party_id FROM public.commerce_orders WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4)").bind(order_id).bind(company.as_str()).bind(branch).bind(department).fetch_optional(&mut *c).await?;
    let Some(row) = row else { return Ok(None) };
    let order = SupplierPromiseOrderFacts {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        kind: row.try_get("kind")?,
        state: row.try_get("state")?,
        version: row.try_get("version")?,
        order_date: row.try_get("order_date")?,
        due_date: row.try_get("due_date")?,
        currency: row.try_get("currency")?,
        party_id: row.try_get("party_id")?,
    };
    let rows=sqlx::query("SELECT l.id,l.quantity_milli,l.fulfilled_milli FROM public.commerce_order_lines l JOIN public.commerce_products p ON p.id=l.product_id WHERE l.order_id=$1 ORDER BY l.ordinal").bind(order_id).fetch_all(&mut *c).await?;
    let lines = rows
        .into_iter()
        .map(|r| {
            Ok(SupplierPromiseLineFacts {
                id: r.try_get("id")?,
                quantity_milli: r.try_get("quantity_milli")?,
                fulfilled_milli: r.try_get("fulfilled_milli")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(Some(CurrentSupplierPromiseSource {
        unit,
        mode,
        branch: branch.map(str::to_owned),
        department: department.map(str::to_owned),
        order,
        lines,
    }))
}
impl CurrentSupplierPromiseSource {
    pub fn mode(&self) -> SupplierPromiseSourceMode {
        self.mode
    }
    /// Validate only unit identity and enabled fence, not business state.
    pub async fn require_same_unit(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || (self.mode == SupplierPromiseSourceMode::Enabled && !enabled_fence(c).await?)
        {
            return Err(changed());
        }
        Ok(())
    }
    pub fn order(&self) -> &SupplierPromiseOrderFacts {
        &self.order
    }
    pub fn lines(&self) -> &[SupplierPromiseLineFacts] {
        &self.lines
    }
    /// Mirrors the TS pre-write source-order assertion. It deliberately does not
    /// demand equality of unrelated order columns or line values after reading.
    pub async fn require_source_guard(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || (self.mode == SupplierPromiseSourceMode::Enabled && !enabled_fence(c).await?)
        {
            return Err(changed());
        }
        let o = &self.order;
        let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.commerce_orders WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4) AND version=$5 AND kind='purchase' AND state IN ('confirmed','partially_fulfilled'))").bind(&o.id).bind(&o.company_id).bind(self.branch.as_deref()).bind(self.department.as_deref()).bind(o.version).fetch_one(c).await?;
        if !ok {
            return Err(changed());
        }
        Ok(())
    }
}
