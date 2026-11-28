//! Commerce order scope for Portal notifications. This witness does not grant
//! customer session, Portal action or purchase authority.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "PORTAL_ORDER_SCOPE_CHANGED",
        "The original order scope changed.",
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
        return Err(PersistenceError::new("PORTAL_ORDER_SCOPE_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Take this fence in the enclosing owner's global order before first snapshot.
pub async fn lock_portal_order_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PORTAL_ORDER_SCOPE_UNIT_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.commerce_orders IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn locked(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation='public.commerce_orders'::regclass AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(c).await?)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalOrderScope {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}
#[derive(Debug)]
pub struct PortalOrderScopeWitness {
    unit: Unit,
    scope: PortalOrderScope,
}
impl PortalOrderScopeWitness {
    pub fn scope(&self) -> &PortalOrderScope {
        &self.scope
    }
    /// Check only captured notification scope, not unrelated order fields. Calling
    /// this after later effects is a composition policy, not a session grant.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || !locked(c).await?
            || capture(c, &self.scope.company_id, &self.scope.id)
                .await?
                .as_ref()
                != Some(&self.scope)
        {
            return Err(changed());
        }
        Ok(())
    }
}
async fn capture(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Option<PortalOrderScope>, Error> {
    let Some(row)=sqlx::query("SELECT id,company_id,branch_id,department_id,version FROM public.commerce_orders WHERE id=$1 AND company_id=$2 FOR SHARE").bind(id).bind(company).fetch_optional(c).await? else{return Ok(None)};
    Ok(Some(PortalOrderScope {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        version: row.try_get("version")?,
    }))
}
/// Capture the actual order scope in the already fenced serializable unit.
pub async fn read_portal_order_scope(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<PortalOrderScopeWitness>, Error> {
    let unit = unit(c).await?;
    if !locked(c).await? {
        return Err(changed());
    }
    Ok(capture(c, company.as_str(), id)
        .await?
        .map(|scope| PortalOrderScopeWitness { unit, scope }))
}
