use crate::{application::beta_policy::BetaPolicyReadPort, domain::beta_policy::CompanyBetaPolicy};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

/// The owning authorization coordinator calls this before its first snapshot.
/// Acquiring a table lock after an older SERIALIZABLE snapshot is insufficient.
pub async fn lock_beta_policy_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "TRANSACTION_REQUIRED",
            "A caller transaction is required.",
        )
        .into());
    }
    sqlx::query("LOCK TABLE beta_policy IN SHARE MODE")
        .execute(connection)
        .await?;
    Ok(())
}

/// Check the existing beta policy fence without acquiring or upgrading it.
/// The caller remains responsible for pre-snapshot ordering and authority.
pub async fn require_beta_policy_sources_held(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("BETA_POLICY_TRANSACTION_REQUIRED").into());
    }
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='beta_policy'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(connection).await?;
    if !held{return Err(PersistenceError::new("BETA_POLICY_LOCK_REQUIRED").into());}Ok(())
}
pub struct PgBetaPolicyReader<'c> {
    connection: &'c mut PgConnection,
}
impl<'c> PgBetaPolicyReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}
impl BetaPolicyReadPort for PgBetaPolicyReader<'_> {
    type Error = PgTransactionError<BusinessError>;
    async fn company_beta_policy(
        &mut self,
        company: &CompanyId,
    ) -> Result<CompanyBetaPolicy, Self::Error> {
        // A fact reader never opens, commits, rolls back or retries an owner's transaction.
        if !self.connection.is_in_transaction() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "TRANSACTION_REQUIRED",
                "A caller transaction is required.",
            )
            .into());
        }
        let enabled = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM beta_policy WHERE company_id=$1)",
        )
        .bind(company.as_str())
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(CompanyBetaPolicy {
            company_id: company.clone(),
            enabled,
        })
    }
}

/// Canonical beta policy fence retained for the existing Identity authority
/// source preparation. This is not an entitlement or a selected actor grant.
/// It does not prove lock timing, migration provenance, or fence concurrent DDL.
pub struct CurrentBetaPolicyAuthorityBoundary {
    unit: (String,i32,String,String),
    boundary: (String,String,i64),
}
impl CurrentBetaPolicyAuthorityBoundary {
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>> {
        if beta_authority_unit(c).await? != self.unit || beta_authority_boundary(c).await? != self.boundary {
            return Err(PersistenceError::new("BETA_POLICY_AUTHORITY_SOURCE_CHANGED").into());
        }
        Ok(())
    }
}
/// The coordinator sets the canonical path and acquires all owner locks before
/// the first snapshot. This validates the current Management fence only.
pub async fn inspect_beta_policy_authority_boundary(c:&mut PgConnection)->Result<CurrentBetaPolicyAuthorityBoundary,PgTransactionError<BusinessError>> {
    let unit=beta_authority_unit(c).await?;
    let boundary=beta_authority_boundary(c).await?;
    Ok(CurrentBetaPolicyAuthorityBoundary{unit,boundary})
}
async fn beta_authority_unit(c:&mut PgConnection)->Result<(String,i32,String,String),PgTransactionError<BusinessError>> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("BETA_POLICY_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn beta_authority_boundary(c:&mut PgConnection)->Result<(String,String,i64),PgTransactionError<BusinessError>> {
    let (valid,visible,role,session,oid):(bool,bool,String,String,i64)=sqlx::query_as("SELECT current_setting('search_path')='pg_catalog, public, pg_temp' AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active(c.oid),current_user::text,session_user::text,c.oid::bigint FROM pg_class c WHERE c.oid='public.beta_policy'::regclass").fetch_one(c).await?;
    if !valid || !visible {return Err(PersistenceError::new("BETA_POLICY_AUTHORITY_SOURCE_REQUIRED").into());}
    Ok((role,session,oid))
}
