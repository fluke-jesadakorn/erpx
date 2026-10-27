use crate::domain::maintenance::MaintenanceAssetFacts;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Transaction = (String, i32, String, String);
/// Same-unit physical-source descriptor; never a Person/Work/purchase grant.
pub struct CurrentMaintenanceAsset {
    facts: MaintenanceAssetFacts,
    transaction: Transaction,
}
impl CurrentMaintenanceAsset {
    pub fn facts(&self) -> &MaintenanceAssetFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.transaction {
            return Err(changed().into());
        }
        let current = read_current_maintenance_asset(
            c,
            &self.facts.holding_id,
            &self.facts.company_id,
            &self.facts.asset_id,
        )
        .await?
        .ok_or_else(changed)?;
        if current.facts != self.facts {
            return Err(changed().into());
        }
        Ok(())
    }
}
/// Preparation occurs before the owning coordinator's first data snapshot.
pub async fn lock_maintenance_asset_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSET_MAINTENANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE operational_assets IN ROW SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_current_maintenance_asset(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    asset: &RecordId,
) -> Result<Option<CurrentMaintenanceAsset>, Error> {
    let transaction = transaction(c).await?;
    // Names, costs, currency, custodians and private profile fields are not
    // needed to prove this relationship and are never transferred here.
    let row=sqlx::query("SELECT CASE WHEN octet_length(id)<=200 THEN id END id,CASE WHEN octet_length(company_id)<=200 THEN company_id END company_id,CASE WHEN octet_length(branch_id)<=200 THEN branch_id END branch_id,octet_length(department_id)::bigint department_bytes,CASE WHEN octet_length(department_id)<=200 THEN department_id END department_id,version::bigint version FROM operational_assets WHERE id=$1 AND company_id=$2 FOR UPDATE")
        .bind(asset.as_str()).bind(company.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if row
        .try_get::<Option<i64>, _>("department_bytes")?
        .is_some_and(|n| n > 200)
    {
        return Err(invalid().into());
    }
    let facts = MaintenanceAssetFacts {
        holding_id: holding.clone(),
        company_id: CompanyId::new(
            row.try_get::<Option<String>, _>("company_id")?
                .ok_or_else(invalid)?,
        )?,
        asset_id: RecordId::new(
            row.try_get::<Option<String>, _>("id")?
                .ok_or_else(invalid)?,
        )?,
        branch_id: RecordId::new(
            row.try_get::<Option<String>, _>("branch_id")?
                .ok_or_else(invalid)?,
        )?,
        department_id: row
            .try_get::<Option<String>, _>("department_id")?
            .map(RecordId::new)
            .transpose()?,
        version: row.try_get("version")?,
    };
    if &facts.company_id != company || &facts.asset_id != asset {
        return Err(changed().into());
    }
    Ok(Some(CurrentMaintenanceAsset { facts, transaction }))
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSET_MAINTENANCE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("ASSET_MAINTENANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ASSET_MAINTENANCE_SOURCE_CHANGED",
        "The actual physical Asset source changed.",
    )
}
fn invalid() -> PersistenceError {
    PersistenceError::new("ASSET_MAINTENANCE_SOURCE_INVALID")
}
