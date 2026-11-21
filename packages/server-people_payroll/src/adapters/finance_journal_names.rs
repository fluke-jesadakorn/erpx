//! Display facts only. No status/engagement/alias permission inference.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, UserId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_JOURNAL_PERSON_NAME_UNAVAILABLE",
        "The journal's People display facts are unavailable.",
    )
    .into()
}
pub async fn lock_finance_journal_name_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE hr_people IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn transaction(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_OWNER_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn names(
    c: &mut PgConnection,
    company: &CompanyId,
    users: &[UserId],
) -> Result<Vec<(UserId, Option<(String, String)>)>, Error> {
    let mut output = Vec::new();
    for user in users {
        let rows:Vec<(Option<String>,Option<String>)>=sqlx::query_as("SELECT CASE WHEN octet_length(id)<=200 THEN id END,CASE WHEN octet_length(legal_name)<=16384 THEN legal_name END FROM hr_people WHERE company_id=$1 AND user_id=$2 AND merged_into_id IS NULL LIMIT 2 FOR SHARE").bind(company.as_str()).bind(user.as_str()).fetch_all(&mut*c).await?;
        let value = match rows.as_slice() {
            [] => None,
            [(Some(id), Some(name))] => Some((id.clone(), name.clone())),
            _ => return Err(unavailable()),
        };
        output.push((user.clone(), value));
    }
    Ok(output)
}
pub struct CurrentFinanceJournalNames {
    company: CompanyId,
    users: Vec<UserId>,
    rows: Vec<(UserId, Option<(String, String)>)>,
    transaction: (String, i32, String, String),
}
impl CurrentFinanceJournalNames {
    pub fn name(&self, user: &UserId) -> Result<&str, BusinessError> {
        match self.rows.iter().find(|(id, _)| id == user) {
            Some((_, Some((_, name)))) => Ok(name),
            Some((_, None)) => Ok("Unlinked account"),
            None => Err(BusinessError::invalid(
                "FINANCE_JOURNAL_NAME_SCOPE_MISMATCH",
                "Use only the journal's observed user identities.",
            )),
        }
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.transaction
            || names(c, &self.company, &self.users).await? != self.rows
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
pub async fn read_finance_journal_names(
    c: &mut PgConnection,
    company: &CompanyId,
    users: &[UserId],
) -> Result<CurrentFinanceJournalNames, Error> {
    let mut users = users.to_vec();
    users.sort();
    users.dedup();
    if users.is_empty() || users.len() > 2 {
        return Err(unavailable());
    }
    let transaction = transaction(c).await?;
    let rows = names(c, company, &users).await?;
    Ok(CurrentFinanceJournalNames {
        company: company.clone(),
        users,
        rows,
        transaction,
    })
}
