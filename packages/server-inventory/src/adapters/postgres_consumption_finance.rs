//! Immutable consumption facts for an independent Finance reviewer. This port
//! neither checks operational worker rights nor grants stock/posting authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceConsumptionFinanceFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub consumption_id: RecordId,
    pub work_id: RecordId,
    pub source_version: i64,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub journal_id: RecordId,
    pub currency: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
}
/// Positive same-unit source proof; private fields and no Clone/Serde factory.
pub struct CurrentMaintenanceConsumptionFinance {
    facts: MaintenanceConsumptionFinanceFacts,
    unit: Unit,
}
impl CurrentMaintenanceConsumptionFinance {
    pub fn facts(&self) -> &MaintenanceConsumptionFinanceFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let actual = read(
            c,
            &self.facts.holding_id,
            &self.facts.company_id,
            &self.facts.consumption_id,
        )
        .await?;
        if actual.as_ref() != Some(&self.facts) {
            return Err(changed());
        }
        Ok(())
    }
}
/// Exact B0 source relation absence fence. Call before the first Holding data
/// snapshot; utility locks alone establish no personal or included purchase.
pub async fn lock_consumption_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE operational_inventory_consumptions IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_maintenance_consumption_finance(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    id: &RecordId,
) -> Result<Option<CurrentMaintenanceConsumptionFinance>, Error> {
    let unit = unit(c).await?;
    Ok(read(c, holding, company, id)
        .await?
        .map(|facts| CurrentMaintenanceConsumptionFinance { facts, unit }))
}
async fn read(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    id: &RecordId,
) -> Result<Option<MaintenanceConsumptionFinanceFacts>, Error> {
    // Only facts used by the Finance owner predicate leave Inventory. Reason,
    // names, stock, Quality and current availability are intentionally absent.
    let row=sqlx::query("SELECT CASE WHEN octet_length(id)<=200 THEN id END id,CASE WHEN octet_length(company_id)<=200 THEN company_id END company_id,CASE WHEN octet_length(source_id)<=200 THEN source_id END work_id,source_version,CASE WHEN octet_length(branch_id)<=200 THEN branch_id END branch_id,CASE WHEN octet_length(department_id)<=200 THEN department_id END department_id,octet_length(department_id)::bigint department_bytes,CASE WHEN octet_length(journal_id)<=200 THEN journal_id END journal_id,CASE WHEN octet_length(currency)<=16 THEN currency END currency,quantity_milli,value_minor FROM operational_inventory_consumptions WHERE id=$1 AND company_id=$2 AND source_kind='maintenance' FOR SHARE")
        .bind(id.as_str()).bind(company.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if row
        .try_get::<Option<i64>, _>("department_bytes")?
        .is_some_and(|n| n > 200)
    {
        return Err(invalid());
    }
    let text = |name| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(name)?.ok_or_else(invalid)
    };
    let facts = MaintenanceConsumptionFinanceFacts {
        holding_id: holding.clone(),
        company_id: CompanyId::new(text("company_id")?)?,
        consumption_id: RecordId::new(text("id")?)?,
        work_id: RecordId::new(text("work_id")?)?,
        source_version: row.try_get("source_version")?,
        branch_id: RecordId::new(text("branch_id")?)?,
        department_id: row
            .try_get::<Option<String>, _>("department_id")?
            .map(RecordId::new)
            .transpose()?,
        journal_id: RecordId::new(text("journal_id")?)?,
        currency: text("currency")?,
        quantity_milli: row.try_get("quantity_milli")?,
        value_minor: row.try_get("value_minor")?,
    };
    const SAFE: i64 = 9_007_199_254_740_991;
    if facts.company_id != *company
        || facts.consumption_id != *id
        || facts.currency.is_empty()
        || ![
            facts.source_version,
            facts.quantity_milli,
            facts.value_minor,
        ]
        .iter()
        .all(|n| (1..=SAFE).contains(n))
    {
        return Err(invalid());
    }
    Ok(Some(facts))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("INVENTORY_FINANCE_CONSUMPTION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_FINANCE_CONSUMPTION_CHANGED",
        "The original immutable consumption source changed.",
    )
    .into()
}
