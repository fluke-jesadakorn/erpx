//! Immutable consumption facts for an independent Finance reviewer. This port
//! neither checks operational worker rights nor grants stock/posting authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceConsumptionFinanceFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub consumption_id: String,
    pub order_id: String,
    pub source_version: i64,
    pub warehouse_id: String,
    pub product_id: String,
    pub creator_user_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub journal_id: String,
    pub currency: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
}
/// Positive same-unit source proof; private fields and no Clone/Serde factory.
pub struct CurrentFieldServiceConsumptionFinance {
    facts: FieldServiceConsumptionFinanceFacts,
    unit: Unit,
}
impl CurrentFieldServiceConsumptionFinance {
    pub fn facts(&self) -> &FieldServiceConsumptionFinanceFacts {
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
pub async fn read_field_service_consumption_finance(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    id: &str,
) -> Result<Option<CurrentFieldServiceConsumptionFinance>, Error> {
    let unit = unit(c).await?;
    Ok(read(c, holding, company, id)
        .await?
        .map(|facts| CurrentFieldServiceConsumptionFinance { facts, unit }))
}
async fn read(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    id: &str,
) -> Result<Option<FieldServiceConsumptionFinanceFacts>, Error> {
    // Only facts used by the Finance owner predicate leave Inventory. Reason,
    // names, stock, Quality and current availability are intentionally absent.
    let row=sqlx::query("SELECT id id,CASE WHEN octet_length(company_id)<=200 THEN company_id END company_id,source_id order_id,source_version,branch_id branch_id,department_id department_id,octet_length(department_id)::bigint department_bytes,journal_id journal_id,CASE WHEN octet_length(currency)<=16 THEN currency END currency,quantity_milli,value_minor,warehouse_id warehouse_id,product_id product_id,created_by created_by FROM operational_inventory_consumptions WHERE id=$1 AND company_id=$2 AND source_kind='field_service' FOR SHARE")
        .bind(id).bind(company.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    let text = |name| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(name)?.ok_or_else(invalid)
    };
    let facts = FieldServiceConsumptionFinanceFacts {
        holding_id: holding.clone(),
        company_id: CompanyId::new(text("company_id")?)?,
        consumption_id: text("id")?,
        order_id: text("order_id")?,
        source_version: row.try_get("source_version")?,
        warehouse_id: text("warehouse_id")?,
        product_id: text("product_id")?,
        creator_user_id: text("created_by")?,
        branch_id: text("branch_id")?,
        department_id: row.try_get::<Option<String>, _>("department_id")?,
        journal_id: text("journal_id")?,
        currency: text("currency")?,
        quantity_milli: row.try_get("quantity_milli")?,
        value_minor: row.try_get("value_minor")?,
    };
    const SAFE: i64 = 9_007_199_254_740_991;
    if facts.company_id != *company
        || facts.consumption_id != id
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
