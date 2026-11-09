//! Current employment reference for operational owners. This is not a login,
//! selected assignment, permission, purchase or stock-mutation capability.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis, UserId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Transaction = (String, i32, String, String);

/// Fixed People-owned relation projections for the composed Sales recipient
/// predicate. This witnesses source identity, not content or authorization.
pub struct CurrentSalesWorkforceJoin {
    transaction: Transaction,
    boundary: (String, String, String, i64, i64),
}
impl CurrentSalesWorkforceJoin {
    pub fn cte_sql(&self) -> &'static str {
        "sales_recipient_people AS (SELECT id,user_id,company_id,status,merged_into_id FROM public.hr_people), sales_recipient_engagements AS (SELECT id,person_id,company_id,status,start_date,end_date FROM public.hr_engagements)"
    }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if transaction(c).await? != self.transaction || sales_workforce_boundary(c).await? != self.boundary { return Err(PersistenceError::new("PEOPLE_SALES_JOIN_CHANGED").into()); }
        Ok(())
    }
}
/// Prepare People and all other owner locks before the first snapshot. This
/// validates current lock possession and same unit, not lock timing or rows.
pub async fn inspect_sales_workforce_join(c:&mut PgConnection)->Result<CurrentSalesWorkforceJoin,Error> {
    let transaction=transaction(c).await?;
    let boundary=sales_workforce_boundary(c).await?;
    Ok(CurrentSalesWorkforceJoin{transaction,boundary})
}
async fn sales_workforce_boundary(c:&mut PgConnection)->Result<(String,String,String,i64,i64),Error> {
    let (held,visible,role,session,path,people,engagements):(bool,bool,String,String,String,i64,i64)=sqlx::query_as("SELECT (SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.hr_people'::regclass,'public.hr_engagements'::regclass)), NOT (row_security_active('public.hr_people'::regclass) OR row_security_active('public.hr_engagements'::regclass)),current_user::text,session_user::text,current_setting('search_path'),'public.hr_people'::regclass::oid::bigint,'public.hr_engagements'::regclass::oid::bigint").fetch_one(c).await?;
    if !held || !visible {return Err(PersistenceError::new("PEOPLE_SALES_JOIN_REQUIRED").into());}
    Ok((role,session,path,people,engagements))
}

/// Opaque current People source in one actual transaction. Only the required
/// eligibility predicate is transferred; no contact, salary or profile data.
pub struct CurrentWorkforceReference {
    company: CompanyId,
    user: String,
    eligible: bool,
    observed_at: i64,
    transaction: Transaction,
}
impl CurrentWorkforceReference {
    pub fn is_eligible(&self) -> bool {
        self.eligible
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if now.get() < self.observed_at || transaction(c).await? != self.transaction {
            return Err(changed().into());
        }
        let current = inspect_workforce_reference_raw(c, &self.company, &self.user, now).await?;
        if current.eligible != self.eligible {
            return Err(changed().into());
        }
        Ok(())
    }
}

/// Call the existing People source-lock preparation before the owning unit's
/// first snapshot. A positive reference additionally retains actual person and
/// engagement tuples, preserving old-snapshot 40001 rather than stale approval.
pub async fn inspect_workforce_reference(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &UserId,
    now: TimestampMillis,
) -> Result<CurrentWorkforceReference, Error> {
    inspect_workforce_reference_raw(c, company, user.as_str(), now).await
}

pub async fn inspect_workforce_reference_raw(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &str,
    now: TimestampMillis,
) -> Result<CurrentWorkforceReference, Error> {
    let transaction = transaction(c).await?;
    // This exactly preserves Maintenance's current reference rule: an active,
    // unmerged linked person and any active engagement effective on the UTC day.
    // Engagement placement does not replace the separate assignment scope rule.
    // LIMIT and SELECT true keep client retention constant even with many rows.
    let eligible = sqlx::query_scalar::<_,bool>("SELECT true FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.user_id=$2 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(to_timestamp($3::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(to_timestamp($3::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY p.id,e.id LIMIT 1 FOR SHARE OF p,e")
        .bind(company.as_str()).bind(user).bind(now.get()).fetch_optional(c).await?.unwrap_or(false);
    Ok(CurrentWorkforceReference {
        company: company.clone(),
        user: user.to_owned(),
        eligible,
        observed_at: now.get(),
        transaction,
    })
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_WORKFORCE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PEOPLE_WORKFORCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PEOPLE_WORKFORCE_REFERENCE_CHANGED",
        "The current workforce reference changed.",
    )
}
