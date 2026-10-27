//! Current employment reference for operational owners. This is not a login,
//! selected assignment, permission, purchase or stock-mutation capability.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis, UserId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Transaction = (String, i32, String, String);

/// Opaque current People source in one actual transaction. Only the required
/// eligibility predicate is transferred; no contact, salary or profile data.
pub struct CurrentWorkforceReference {
    company: CompanyId,
    user: UserId,
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
        let current = inspect_workforce_reference(c, &self.company, &self.user, now).await?;
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
    let transaction = transaction(c).await?;
    // This exactly preserves Maintenance's current reference rule: an active,
    // unmerged linked person and any active engagement effective on the UTC day.
    // Engagement placement does not replace the separate assignment scope rule.
    // LIMIT and SELECT true keep client retention constant even with many rows.
    let eligible = sqlx::query_scalar::<_,bool>("SELECT true FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.user_id=$2 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(to_timestamp($3::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(to_timestamp($3::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY p.id,e.id LIMIT 1 FOR SHARE OF p,e")
        .bind(company.as_str()).bind(user.as_str()).bind(now.get()).fetch_optional(c).await?.unwrap_or(false);
    Ok(CurrentWorkforceReference {
        company: company.clone(),
        user: user.clone(),
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
