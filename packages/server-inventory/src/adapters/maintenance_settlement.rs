//! Exact original Maintenance-close consumption/journal reference set. Facts
//! only: no posting, Work, purchase, quantity or current stock authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceSettlementScope {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub work_id: RecordId,
}
pub struct MaintenanceSettlementReference {
    pub consumption_id: RecordId,
    pub journal_id: RecordId,
}
pub struct CurrentMaintenanceSettlementReferences {
    scope: MaintenanceSettlementScope,
    unit: Unit,
    digest: String,
}
pub async fn lock_maintenance_settlement_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE operational_inventory_consumptions IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_maintenance_settlement_references(
    c: &mut PgConnection,
    scope: MaintenanceSettlementScope,
) -> Result<CurrentMaintenanceSettlementReferences, Error> {
    let unit = unit(c).await?;
    let digest = digest(c, &scope.work_id).await?;
    Ok(CurrentMaintenanceSettlementReferences {
        scope,
        unit,
        digest,
    })
}
impl CurrentMaintenanceSettlementReferences {
    pub fn scope(&self) -> &MaintenanceSettlementScope {
        &self.scope
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || digest(c, &self.scope.work_id).await? != self.digest {
            return Err(changed());
        }
        Ok(())
    }
    /// One bounded row at a time; there is no arbitrary history-length cutoff.
    pub async fn next(
        &self,
        c: &mut PgConnection,
        after: Option<&RecordId>,
    ) -> Result<Option<MaintenanceSettlementReference>, Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        // B0 predicates use source_kind/source_id only. Do not silently remove a
        // blocking inconsistent row by adding a new Company/quantity/cost predicate.
        let row=sqlx::query("SELECT CASE WHEN octet_length(id)<=200 THEN id END id,CASE WHEN octet_length(journal_id)<=200 THEN journal_id END journal_id FROM operational_inventory_consumptions WHERE source_kind='maintenance' AND source_id=$1 AND ($2::text IS NULL OR id COLLATE \"C\">$2 COLLATE \"C\") ORDER BY id COLLATE \"C\" LIMIT 1 FOR SHARE")
   .bind(self.scope.work_id.as_str()).bind(after.map(RecordId::as_str)).fetch_optional(c).await?;
        row.map(|r| {
            Ok(MaintenanceSettlementReference {
                consumption_id: RecordId::new(
                    r.try_get::<Option<String>, _>("id")?.ok_or_else(invalid)?,
                )?,
                journal_id: RecordId::new(
                    r.try_get::<Option<String>, _>("journal_id")?
                        .ok_or_else(invalid)?,
                )?,
            })
        })
        .transpose()
    }
}
async fn digest(c: &mut PgConnection, work: &RecordId) -> Result<String, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(COALESCE(jsonb_agg(jsonb_build_array(id,journal_id)ORDER BY id COLLATE \"C\"),'[]'::jsonb)::text,'UTF8')),'hex') FROM operational_inventory_consumptions WHERE source_kind='maintenance' AND source_id=$1")
 .bind(work.as_str()).fetch_one(c).await?)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("MAINTENANCE_SETTLEMENT_TRANSACTION_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_SETTLEMENT_REFERENCES_CHANGED",
        "The original Maintenance parts references changed.",
    )
    .into()
}
