//! Finance-owned metadata CAS for supporting documents of a Field journal.
//! It changes only version/updated_at and retains the exact ledger source row.
use crate::public::FinanceActor;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_TARGET_CHANGED",
        "The journal changed before evidence was bound.",
    )
}
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_EVIDENCE_TRANSACTION_REQUIRED").into());
    }
    let level: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if level != "serializable" {
        return Err(PersistenceError::new("FINANCE_EVIDENCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn row(c: &mut PgConnection, company: &CompanyId, id: &str) -> Result<Value, Error> {
    let value:Option<Option<Value>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(j)::text)<=2097152 THEN to_jsonb(j) END FROM finance_journals j WHERE j.id=$1 AND j.company_id=$2 FOR SHARE OF j").bind(id).bind(company.as_str()).fetch_optional(c).await?;
    value.flatten().ok_or_else(|| changed().into())
}
/// The compiled outer composition owns current IAM/Company permission checks.
/// This is the same owner-port boundary as native Finance journal review; no
/// Identity adapter or foreign table enters the Finance crate.
pub trait PgFieldJournalEvidenceAuthority: Send {
    fn current(
        &mut self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<FinanceActor, Error>> + Send;
}
fn same_actor(a: &FinanceActor, b: &FinanceActor) -> bool {
    a.holding_id == b.holding_id
        && a.company_id == b.company_id
        && a.user_id == b.user_id
        && a.assignment_id == b.assignment_id
}
pub struct PreparedFieldJournalEvidenceVersion {
    actor: FinanceActor,
    unit: (String, i32, String, String),
    company: CompanyId,
    id: String,
    before: Value,
    after: Value,
    version: i64,
    now: i64,
}
pub async fn prepare_field_journal_evidence_version<A: PgFieldJournalEvidenceAuthority>(
    c: &mut PgConnection,
    authority: &mut A,
    id: &str,
    expected: i64,
    now: i64,
) -> Result<PreparedFieldJournalEvidenceVersion, Error> {
    let actor = authority.current(c).await?;
    let company = actor.company_id.clone();
    let unit = unit(c).await?;
    let before = row(c, &company, id).await?;
    if before["source_type"] != "field_service_parts"
        || before["version"].as_i64() != Some(expected)
        || expected < 1
        || !(0..=9_007_199_254_740_991).contains(&now)
    {
        return Err(changed().into());
    }
    let version = expected
        .checked_add(1)
        .filter(|n| *n <= 9_007_199_254_740_991)
        .ok_or_else(changed)?;
    let mut after = before.clone();
    after["version"] = json!(version);
    after["updated_at"] = json!(now);
    Ok(PreparedFieldJournalEvidenceVersion {
        actor,
        unit,
        company,
        id: id.into(),
        before,
        after,
        version,
        now,
    })
}
impl PreparedFieldJournalEvidenceVersion {
    pub fn version(&self) -> i64 {
        self.version
    }
    pub async fn apply<A: PgFieldJournalEvidenceAuthority>(
        &self,
        c: &mut PgConnection,
        authority: &mut A,
    ) -> Result<(), Error> {
        if !same_actor(&authority.current(c).await?, &self.actor) {
            return Err(changed().into());
        }
        if unit(c).await? != self.unit || row(c, &self.company, &self.id).await? != self.before {
            return Err(changed().into());
        }
        let expected = self.before["version"].as_i64().ok_or_else(changed)?;
        let count=sqlx::query("UPDATE finance_journals SET version=$1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5 AND source_type='field_service_parts'").bind(self.version).bind(self.now).bind(&self.id).bind(self.company.as_str()).bind(expected).execute(&mut*c).await?.rows_affected();
        if count != 1 {
            return Err(changed().into());
        }
        self.require_after(c).await
    }
    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || row(c, &self.company, &self.id).await? != self.after {
            return Err(changed().into());
        }
        Ok(())
    }
}
