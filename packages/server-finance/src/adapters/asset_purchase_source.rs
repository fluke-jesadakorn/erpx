//! Finance's linked-asset contribution to the Assets purchase source image.
//! This is source evidence only; it does not grant access or purchase rights.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, i64);

/// Acquire before the enclosing serializable unit takes its first snapshot.
/// An unavailable lock requires retrying the whole unit. The composition root
/// owns global lock ordering and retains the Assets and Commercial witnesses.
pub async fn lock_asset_purchase_finance_source(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSET_FINANCE_SOURCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.finance_assets IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable"
    {
        return Err(PersistenceError::new("ASSET_FINANCE_SOURCE_SERIALIZABLE_REQUIRED").into());
    }
    let (valid, rls, oid): (bool, bool, i64) = sqlx::query_as(
        "SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),row_security_active(c.oid),c.oid::bigint FROM pg_class c WHERE c.oid='public.finance_assets'::regclass"
    ).fetch_one(&mut *c).await?;
    if !valid { return Err(PersistenceError::new("ASSET_FINANCE_SOURCE_LOCK_REQUIRED").into()); }
    if rls { return Err(PersistenceError::new("ASSET_FINANCE_SOURCE_RLS_UNSUPPORTED").into()); }
    let (database, pid, xid, started, role, path): (String, i32, String, String, String, String) = sqlx::query_as(
        "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,current_setting('search_path')"
    ).fetch_one(c).await?;
    Ok((database, pid, xid, started, role, path, oid))
}

async fn image(c: &mut PgConnection, company: &CompanyId, id: Option<&str>) -> Result<Option<String>, Error> {
    let Some(id) = id else { return Ok(None); };
    Ok(sqlx::query_scalar("SELECT to_jsonb(f)::text FROM public.finance_assets f WHERE f.id=$1 AND f.company_id=$2")
        .bind(id).bind(company.as_str()).fetch_optional(c).await?)
}

/// The full JSONB row or exact absence returned by the legacy scoped LEFT
/// JOIN. A foreign Company is absent, not a source of authority.
pub struct CurrentAssetPurchaseFinanceSource {
    company: CompanyId,
    id: Option<String>,
    unit: Unit,
    image: Option<String>,
}

pub async fn observe_asset_purchase_finance_source(
    c: &mut PgConnection, company: &CompanyId, id: Option<&str>,
) -> Result<CurrentAssetPurchaseFinanceSource, Error> {
    let observed_unit = unit(&mut *c).await?;
    let image = image(&mut *c, company, id).await?;
    if unit(&mut *c).await? != observed_unit {
        return Err(BusinessError::new(FailureKind::Conflict, "ASSET_FINANCE_SOURCE_CHANGED", "The linked Finance asset changed.").into());
    }
    Ok(CurrentAssetPurchaseFinanceSource { company: company.clone(), id: id.map(str::to_owned), unit: observed_unit, image })
}

impl CurrentAssetPurchaseFinanceSource {
    pub fn company(&self) -> &CompanyId { &self.company }
    pub fn id(&self) -> Option<&str> { self.id.as_deref() }
    pub fn image(&self) -> Option<&str> { self.image.as_deref() }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || image(&mut *c, &self.company, self.id.as_deref()).await? != self.image
            || unit(c).await? != self.unit
        {
            return Err(BusinessError::new(FailureKind::Conflict, "ASSET_FINANCE_SOURCE_CHANGED", "The linked Finance asset changed.").into());
        }
        Ok(())
    }
}

/// Finance-owned fixed table source for an Asset purchase SQL join. This
/// retains only relation/unit identity; it does not retain rows, prove absence,
/// attest collation or grant Finance/Asset access.
pub struct CurrentAssetPurchaseFinanceJoin{unit:Unit,session_user:String}
impl CurrentAssetPurchaseFinanceJoin{
    pub fn cte_sql(&self)->&'static str{"finance_asset_purchase_rows AS (SELECT * FROM public.finance_assets)"}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(&mut *c).await?!=self.unit||sqlx::query_scalar::<_,String>("SELECT session_user::text").fetch_one(c).await?!=self.session_user{return Err(BusinessError::new(FailureKind::Conflict,"ASSET_FINANCE_JOIN_SOURCE_CHANGED","The Finance Asset join source changed.").into());}Ok(())
    }
}
/// Call after the Finance utility lock and all other owner locks. The caller
/// must rerun and compare its full joined source image after late effects.
pub async fn inspect_asset_purchase_finance_join(c:&mut PgConnection)->Result<CurrentAssetPurchaseFinanceJoin,Error>{
    let identity=unit(&mut *c).await?;let session_user=sqlx::query_scalar("SELECT session_user::text").fetch_one(&mut *c).await?;if unit(c).await?!=identity{return Err(PersistenceError::new("ASSET_FINANCE_JOIN_UNIT_CHANGED").into());}Ok(CurrentAssetPurchaseFinanceJoin{unit:identity,session_user})
}
