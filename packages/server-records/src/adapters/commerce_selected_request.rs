//! Selected Records request source for the Commerce linkage boundary. This is
//! not request visibility, access, purchasing authority or a replay grant.
//! Lock before the first protected snapshot and use a canonical search_path and
//! non-BYPASSRLS role; this raw witness does not enforce either.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, String, i64);

/// Acquire before any protected snapshot; the composition owns lock order.
pub async fn lock_commerce_selected_request(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("RECORDS_SELECTED_REQUEST_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.employee_requests IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("RECORDS_SELECTED_REQUEST_SERIALIZABLE_REQUIRED").into());
    }
    let valid:bool=sqlx::query_scalar("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass('employee_requests')=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid) FROM pg_class c WHERE c.oid=to_regclass('public.employee_requests')").fetch_optional(&mut *c).await?.unwrap_or(false);
    if !valid {
        return Err(PersistenceError::new("RECORDS_SELECTED_REQUEST_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),'public.employee_requests'::regclass::oid::bigint").fetch_one(c).await?)
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<String>, Error> {
    let value:Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN to_jsonb(r)::text END FROM public.employee_requests r WHERE id=$1 AND company_id=$2").bind(id).bind(company.as_str()).fetch_optional(c).await?;
    match value {
        Some(None) => Err(limited()),
        Some(Some(value)) => Ok(Some(value)),
        None => Ok(None),
    }
}
/// Opaque same-unit selection retaining the full actual row or exact absence.
/// `approval_json` stays unparsed; callers must match the original parser and
/// all Support/People/Organization/Workflow/Identity authority decisions.
pub struct CurrentCommerceSelectedRequest {
    unit: Unit,
    company: CompanyId,
    id: String,
    image: Option<String>,
}
impl CurrentCommerceSelectedRequest {
    pub fn company(&self) -> &CompanyId {
        &self.company
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn image(&self) -> Option<&str> {
        self.image.as_deref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || image(&mut *c, &self.company, &self.id).await? != self.image
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// The selected id must come from the trusted joined source, not a client
/// source label. Row/output budget is 2 MiB; SQL scan/work is not bounded.
pub async fn observe_commerce_selected_request(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<CurrentCommerceSelectedRequest, Error> {
    if id.len() > 8192 {
        return Err(limited());
    }
    let identity = unit(&mut *c).await?;
    let image = image(&mut *c, company, id).await?;
    if unit(c).await? != identity {
        return Err(changed());
    }
    Ok(CurrentCommerceSelectedRequest {
        unit: identity,
        company: company.clone(),
        id: id.to_owned(),
        image,
    })
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "RECORDS_SELECTED_REQUEST_CHANGED",
        "The selected employee request changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "RECORDS_SELECTED_REQUEST_LIMIT",
        "The selected employee request exceeds the bounded read.",
    )
    .into()
}
