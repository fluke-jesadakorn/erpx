//! Current Organization setup evidence, never a release or personal grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanyBusinessAcceptance {
    pub status: String,
    pub version: i64,
}

/// Utility source fence, required before the owning release snapshot. It does
/// not perform a SELECT or claim that a previously acquired snapshot is fresh.
pub async fn lock_business_acceptance_sources(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE company_business_acceptance IN SHARE MODE")
        .execute(connection)
        .await?;
    Ok(())
}

pub struct PgCompanyBusinessAcceptanceReader<'c> {
    connection: &'c mut PgConnection,
}
impl<'c> PgCompanyBusinessAcceptanceReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    pub async fn current(
        &mut self,
        company: &CompanyId,
    ) -> Result<Option<CompanyBusinessAcceptance>, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ORGANIZATION_OWNER_TRANSACTION_REQUIRED").into());
        }
        // A known setup changed after an older serializable snapshot raises
        // 40001 here; preserve it for the owning whole-attempt retry.
        let row = sqlx::query(
            "SELECT status,version FROM company_business_acceptance WHERE company_id=$1 FOR SHARE",
        )
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(CompanyBusinessAcceptance {
                status: row.try_get("status")?,
                version: row.try_get("version")?,
            })
        })
        .transpose()
    }
}
