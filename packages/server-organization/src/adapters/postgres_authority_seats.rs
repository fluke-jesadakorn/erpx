use crate::domain::authority_seats::HrAuthoritySeat;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

/// Locks owning Organization sources before IAM/People eligibility snapshots.
/// It borrows the caller's transaction and never starts, retries or commits one.
pub async fn lock_organization_authority_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    sqlx::query(
        "LOCK TABLE organization_nodes,authority_seats,organization_positions IN SHARE MODE",
    )
    .execute(connection)
    .await?;
    Ok(())
}

/// Require all Organization fences on the current connection without taking
/// or upgrading locks. Snapshot timing and actor authority belong to callers.
pub async fn require_organization_authority_sources_held(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    let held:bool=sqlx::query_scalar("SELECT count(DISTINCT relation)=3 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation IN ('organization_nodes'::regclass,'authority_seats'::regclass,'organization_positions'::regclass) AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(connection).await?;
    if !held{return Err(PersistenceError::new("ORGANIZATION_AUTHORITY_LOCKS_REQUIRED").into());}
    Ok(())
}
pub struct PgOrganizationAuthorityReader<'c> {
    connection: &'c mut PgConnection,
}
impl<'c> PgOrganizationAuthorityReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    pub async fn current_hr_seat(
        &mut self,
        company: &CompanyId,
        now: TimestampMillis,
    ) -> Result<Option<HrAuthoritySeat>, PgTransactionError<BusinessError>> {
        let row=sqlx::query("SELECT company_id,person_id,position_id,version::bigint version,effective_at FROM authority_seats WHERE company_id=$1 AND kind='hr' AND scope_id=$1 AND person_id IS NOT NULL AND effective_at<=$2")
            .bind(company.as_str()).bind(now.get()).fetch_optional(&mut *self.connection).await?;
        row.map(|row| {
            Ok(HrAuthoritySeat {
                company_id: CompanyId::new(row.try_get::<String, _>("company_id")?)?,
                person_id: RecordId::new(row.try_get::<String, _>("person_id")?)?,
                position_id: row
                    .try_get::<Option<String>, _>("position_id")?
                    .map(RecordId::new)
                    .transpose()?,
                version: row.try_get("version")?,
                effective_at: TimestampMillis::new(row.try_get("effective_at")?),
            })
        })
        .transpose()
    }
}

/// Canonical Organization relations needed by the IAM Sales predicate. This
/// witnesses source identity only, not actor authority, contents, source-lock
/// timing or protection against concurrent DDL.
pub struct CurrentOrganizationSalesAuthorityBoundary {
    unit: (String,i32,String,String),
    boundary: (String,String,Vec<i64>),
}
impl CurrentOrganizationSalesAuthorityBoundary {
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>> {
        if organization_sales_unit(c).await? != self.unit || organization_sales_boundary(c).await? != self.boundary {
            return Err(PersistenceError::new("ORGANIZATION_SALES_AUTHORITY_SOURCE_CHANGED").into());
        }
        Ok(())
    }
}
/// Caller must set the canonical path and acquire all owner source locks before
/// the first snapshot. This checks currently held Organization locks only.
pub async fn inspect_organization_sales_authority_boundary(c:&mut PgConnection)->Result<CurrentOrganizationSalesAuthorityBoundary,PgTransactionError<BusinessError>> {
    let unit=organization_sales_unit(c).await?;
    let boundary=organization_sales_boundary(c).await?;
    Ok(CurrentOrganizationSalesAuthorityBoundary{unit,boundary})
}
async fn organization_sales_unit(c:&mut PgConnection)->Result<(String,i32,String,String),PgTransactionError<BusinessError>> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("ORGANIZATION_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn organization_sales_boundary(c:&mut PgConnection)->Result<(String,String,Vec<i64>),PgTransactionError<BusinessError>> {
    let (valid,role,session,oids):(bool,String,String,Vec<i64>)=sqlx::query_as(r#"
      WITH names(name) AS (SELECT unnest(ARRAY['organization_nodes','authority_seats','organization_positions']::text[])),
      rels AS (SELECT n.name,to_regclass('public.'||n.name) oid FROM names n)
      SELECT current_setting('search_path')='pg_catalog, public, pg_temp'
       AND (SELECT count(*)=3 AND bool_and(c.oid IS NOT NULL AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM rels r LEFT JOIN pg_class c ON c.oid=r.oid),
       current_user::text,session_user::text,(SELECT array_agg(r.oid::oid::bigint ORDER BY r.name) FROM rels r)
    "#).fetch_one(c).await?;
    if !valid {return Err(PersistenceError::new("ORGANIZATION_SALES_AUTHORITY_SOURCE_REQUIRED").into());}
    Ok((role,session,oids))
}
