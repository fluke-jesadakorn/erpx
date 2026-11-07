//! Current roster facts for journal Flow. This port grants no IAM authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalFlowPerson {
    pub user_id: String,
    pub name: String,
    pub department_id: Option<String>,
}
/// Must be called before any Holding snapshot, in the same retained unit.
pub async fn lock_journal_flow_people_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE hr_people,hr_engagements IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
/// Mirrors the current activeStaffSelect GROUP BY user HAVING COUNT(*)=1:
/// active persons and date-current active engagements, exactly one resulting row.
pub async fn current_journal_flow_people(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<Vec<JournalFlowPerson>, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    let rows = sqlx::query("SELECT p.user_id, min(p.legal_name) name, min(e.department_id) department_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) GROUP BY p.user_id HAVING count(*)=1 ORDER BY p.user_id")
      .bind(company.as_str()).fetch_all(c).await?;
    rows.into_iter()
        .map(|r| {
            Ok(JournalFlowPerson {
                user_id: r.try_get("user_id")?,
                name: r.try_get("name")?,
                department_id: r.try_get("department_id")?,
            })
        })
        .collect()
}
