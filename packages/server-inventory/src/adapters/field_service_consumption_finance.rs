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
    let row=sqlx::query("SELECT CASE WHEN b.bounded THEN p.id END id,CASE WHEN b.bounded AND octet_length(p.company_id)<=200 THEN p.company_id END company_id,CASE WHEN b.bounded THEN p.source_id END order_id,p.source_version,CASE WHEN b.bounded THEN p.branch_id END branch_id,CASE WHEN b.bounded THEN p.department_id END department_id,CASE WHEN b.bounded THEN p.journal_id END journal_id,CASE WHEN b.bounded AND octet_length(p.currency)<=16 THEN p.currency END currency,p.quantity_milli,p.value_minor,CASE WHEN b.bounded THEN p.warehouse_id END warehouse_id,CASE WHEN b.bounded THEN p.product_id END product_id,CASE WHEN b.bounded THEN p.created_by END created_by,b.bounded FROM operational_inventory_consumptions p CROSS JOIN LATERAL (SELECT octet_length(p.id)::bigint+octet_length(p.source_id)::bigint+octet_length(p.branch_id)::bigint+coalesce(octet_length(p.department_id),0)::bigint+octet_length(p.journal_id)::bigint+octet_length(p.warehouse_id)::bigint+octet_length(p.product_id)::bigint+octet_length(p.created_by)::bigint<=2097152 AS bounded) b WHERE p.id=$1 AND p.company_id=$2 AND p.source_kind='field_service' FOR SHARE OF p")
        .bind(id).bind(company.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if !row.try_get::<bool, _>("bounded")? {
        return Err(invalid());
    }
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

/// Inventory's persisted Field partition, including a journal whose Finance
/// label was corrupted. The caller retains the original Holding transaction.
pub async fn journal_has_field_service_consumption(
    c:&mut PgConnection,company:&CompanyId,journal:&str,
)->Result<bool,Error>{
    unit(c).await?;
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operational_inventory_consumptions WHERE company_id=$1 AND journal_id=$2 AND source_kind='field_service')")
        .bind(company.as_str()).bind(journal).fetch_one(c).await?)
}

#[derive(Clone,Debug,PartialEq,Eq,serde::Deserialize)]
pub struct ServiceSettlementConsumption {
    pub id:String,pub company_id:String,pub source_id:String,pub source_version:i64,
    pub branch_id:String,pub department_id:Option<String>,pub warehouse_id:String,pub product_id:String,
    pub journal_id:String,pub currency:String,pub quantity_milli:i64,pub value_minor:i64,pub created_by:String,pub digest:String,
}
/// Raw Company Field consumption references for package settlement. A valid-only
/// Finance projection must never hide an orphan or malformed consumption.
/// Known Field order references also retain cross-Company corrupt markers,
/// matching the original raw presence predicate without exposing their data.
pub struct CurrentServiceSettlementConsumptions {
    company:CompanyId,
    known_orders:Vec<String>,
    unit:Unit,
    order_ids:Vec<String>,
    data:Vec<ServiceSettlementConsumption>,
    digest:String,
}
impl CurrentServiceSettlementConsumptions {
    pub fn order_ids(&self)->&[String]{&self.order_ids}
    pub fn data(&self)->&[ServiceSettlementConsumption]{&self.data}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await?!=self.unit{return Err(changed());}
        let(data,digest)=capture_service_settlement_consumptions(c,&self.company,&self.known_orders).await?;
        if data!=self.data||digest!=self.digest{return Err(changed());}
        Ok(())
    }
}
async fn capture_service_settlement_consumptions(c:&mut PgConnection,company:&CompanyId,known_orders:&[String])->Result<(Vec<ServiceSettlementConsumption>,String),Error>{
    let row=sqlx::query("WITH source AS MATERIALIZED(SELECT * FROM operational_inventory_consumptions WHERE source_kind='field_service' AND (company_id=$1 OR source_id=ANY($2::text[]))),snapshot AS(SELECT coalesce(jsonb_agg(jsonb_build_object('id',s.id,'company_id',s.company_id,'source_id',s.source_id,'source_version',s.source_version,'branch_id',s.branch_id,'department_id',s.department_id,'warehouse_id',s.warehouse_id,'product_id',s.product_id,'journal_id',s.journal_id,'currency',s.currency,'quantity_milli',s.quantity_milli,'value_minor',s.value_minor,'created_by',s.created_by,'digest',encode(sha256(convert_to(to_jsonb(s)::text,'UTF8')),'hex')) ORDER BY id),'[]'::jsonb) value,encode(sha256(convert_to(coalesce(string_agg(encode(sha256(convert_to(to_jsonb(s)::text,'UTF8')),'hex'),'' ORDER BY id),''),'UTF8')),'hex') digest FROM source s) SELECT CASE WHEN octet_length(value::text)<=2097152 THEN value::text END payload,digest FROM snapshot").bind(company.as_str()).bind(known_orders).fetch_one(c).await?;
    let payload:Option<String>=row.try_get("payload")?;
    let limited=||BusinessError::new(FailureKind::Unavailable,"SERVICE_CONSUMPTION_SETTLEMENT_LIMIT","The raw Service consumption references exceed this bounded read.");
    let data=serde_json::from_str(&payload.ok_or_else(limited)?).map_err(|_|limited())?;
    Ok((data,row.try_get("digest")?))
}
pub async fn read_service_settlement_consumptions(c:&mut PgConnection,company:&CompanyId,known_orders:&[String])->Result<CurrentServiceSettlementConsumptions,Error>{
    let unit=unit(c).await?;
    let(data,digest)=capture_service_settlement_consumptions(c,company,known_orders).await?;
    let order_ids=data.iter().map(|p|p.source_id.clone()).collect();
    Ok(CurrentServiceSettlementConsumptions{company:company.clone(),known_orders:known_orders.to_vec(),unit,order_ids,data,digest})
}
