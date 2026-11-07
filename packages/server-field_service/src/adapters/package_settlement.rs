//! Company-wide raw Field facts for Commercial settlement. No active-record,
//! purchase or UI visibility filter may erase an outstanding historical duty.
//! Organization/Finance/Inventory validity is composed through their own ports.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Deserialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ServiceSettlementAgreement {
    pub id: String,
    pub version: i64,
    pub status: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub customer_id: String,
    pub currency: String,
    pub asset_id: Option<String>,
    pub terminal_event: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ServiceSettlementVisit {
    pub id: String,
    pub version: i64,
    pub status: String,
    pub agreement_id: String,
    pub asset_id: Option<String>,
    pub ticket_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub digest: String,
    pub agreement_relation_valid: bool,
    pub reservation: bool,
    pub outstanding_reservation: bool,
    pub terminal_event: bool,
    pub billing_lineage: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ServiceSettlementReservation {
    pub id:String,pub order_id:String,pub warehouse_id:String,pub product_id:String,
    pub quantity_milli:i64,pub consumed_milli:i64,pub released_milli:i64,pub digest:String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ServiceSettlementIssueEvent {
    pub order_id:String,pub version:i64,pub action:String,pub details_json:serde_json::Value,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ServiceSettlementRelease {
    pub id:String,pub order_id:String,pub agreement_id:String,pub source_order_version:i64,
    pub customer_id:String,pub currency:String,pub amount_minor:i64,pub pricing_date:String,
    pub released_by:String,pub status:String,pub digest:String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ServiceSettlementRows {
    pub agreements: Vec<ServiceSettlementAgreement>,
    pub visits: Vec<ServiceSettlementVisit>,
    pub reservations:Vec<ServiceSettlementReservation>,
    pub issue_events:Vec<ServiceSettlementIssueEvent>,
    pub releases:Vec<ServiceSettlementRelease>,
}

fn changed() -> Error {
    BusinessError::new(FailureKind::Conflict,"SERVICE_PACKAGE_OBLIGATION_SOURCE_CHANGED","The original Service settlement source changed.").into()
}
fn limited() -> Error {
    BusinessError::new(FailureKind::Unavailable,"SERVICE_PACKAGE_OBLIGATION_LIMIT","The raw Service settlement exceeds this bounded read.").into()
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Outer composition acquires all required owner leases before its first
/// source snapshot, using the same deterministic source preparation as grants.
pub async fn lock_service_package_settlement(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FIELD_SERVICE_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE field_service_agreements,field_service_agreement_events,field_service_orders,field_service_order_events,field_service_part_reservations,field_service_billing_releases IN SHARE MODE").execute(c).await?;
    Ok(())
}

async fn image(c:&mut PgConnection,company:&CompanyId)->Result<(ServiceSettlementRows,String),Error>{
    let row=sqlx::query(r#"
WITH agreements AS MATERIALIZED (
 SELECT * FROM field_service_agreements WHERE company_id=$1
), visits AS MATERIALIZED (
 SELECT * FROM field_service_orders WHERE company_id=$1
), payload AS (
 SELECT jsonb_build_object(
  'agreements',coalesce((SELECT jsonb_agg(jsonb_build_object(
   'id',g.id,'version',g.version,'status',g.status,'branch_id',g.branch_id,
   'department_id',g.department_id,'customer_id',g.customer_id,'currency',g.currency,'asset_id',g.asset_id,
   'terminal_event',EXISTS(SELECT 1 FROM field_service_agreement_events e WHERE e.agreement_id=g.id
    AND e.version=g.version AND e.status=g.status
    AND e.action=CASE g.status WHEN 'retired' THEN 'retire' WHEN 'rejected' THEN 'reject' ELSE '' END)
  ) ORDER BY g.id) FROM agreements g),'[]'::jsonb),
  'visits',coalesce((SELECT jsonb_agg(jsonb_build_object(
   'id',v.id,'version',v.version,'status',v.status,'agreement_id',v.agreement_id,'asset_id',v.asset_id,
   'ticket_id',v.ticket_id,'branch_id',v.branch_id,'department_id',v.department_id,
   'digest',encode(sha256(convert_to(to_jsonb(v)::text,'UTF8')),'hex'),
   'agreement_relation_valid',coalesce(g.branch_id=v.branch_id
    AND g.department_id IS NOT DISTINCT FROM v.department_id AND v.agreement_version<=g.version,false),
   'reservation',EXISTS(SELECT 1 FROM field_service_part_reservations p WHERE p.order_id=v.id),
   'outstanding_reservation',EXISTS(SELECT 1 FROM field_service_part_reservations p WHERE p.order_id=v.id
    AND p.quantity_milli>p.consumed_milli+p.released_milli),
   'terminal_event',EXISTS(SELECT 1 FROM field_service_order_events e WHERE e.order_id=v.id
    AND e.version=v.version AND e.status=v.status
    AND e.action=CASE v.status WHEN 'cancelled' THEN 'cancel' WHEN 'billed' THEN 'release-billing' ELSE '' END),
   'billing_lineage',EXISTS(SELECT 1 FROM field_service_billing_releases b WHERE b.order_id=v.id
    AND b.agreement_id=v.agreement_id AND b.source_order_version=v.version
    AND b.customer_id=g.customer_id AND b.currency=g.currency)
  ) ORDER BY v.id) FROM visits v LEFT JOIN agreements g ON g.id=v.agreement_id),'[]'::jsonb),
  'reservations',coalesce((SELECT jsonb_agg(jsonb_build_object(
   'id',p.id,'order_id',p.order_id,'warehouse_id',p.warehouse_id,'product_id',p.product_id,
   'quantity_milli',p.quantity_milli,'consumed_milli',p.consumed_milli,'released_milli',p.released_milli,
   'digest',encode(sha256(convert_to(to_jsonb(p)::text,'UTF8')),'hex')) ORDER BY p.id)
   FROM field_service_part_reservations p JOIN visits v ON v.id=p.order_id),'[]'::jsonb),
  'issue_events',coalesce((SELECT jsonb_agg(jsonb_build_object(
   'order_id',e.order_id,'version',e.version,'action',e.action,'details_json',jsonb_build_object(
    'consumptionId',e.details_json->'consumptionId','journalId',e.details_json->'journalId',
    'quantityMilli',e.details_json->'quantityMilli','costMinor',e.details_json->'costMinor',
    'reservationId',e.details_json->'reservationId')) ORDER BY e.order_id,e.version)
   FROM field_service_order_events e JOIN visits v ON v.id=e.order_id WHERE e.action='issue-part'),'[]'::jsonb),
  'releases',coalesce((SELECT jsonb_agg(jsonb_build_object(
   'id',b.id,'order_id',b.order_id,'agreement_id',b.agreement_id,'source_order_version',b.source_order_version,
   'customer_id',b.customer_id,'currency',b.currency,'amount_minor',b.amount_minor,'pricing_date',b.pricing_date,
   'released_by',b.released_by,'status',b.status,'digest',encode(sha256(convert_to(to_jsonb(b)::text,'UTF8')),'hex')) ORDER BY b.id)
   FROM field_service_billing_releases b JOIN visits v ON v.id=b.order_id),'[]'::jsonb)
 ) value
), hashes AS (
 SELECT 'agreement' kind,g.id,encode(sha256(convert_to(to_jsonb(g)::text,'UTF8')),'hex') digest FROM agreements g
 UNION ALL SELECT 'agreement-event',e.id,encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM field_service_agreement_events e JOIN agreements g ON g.id=e.agreement_id
 UNION ALL SELECT 'visit',v.id,encode(sha256(convert_to(to_jsonb(v)::text,'UTF8')),'hex') FROM visits v
 UNION ALL SELECT 'visit-event',e.id,encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM field_service_order_events e JOIN visits v ON v.id=e.order_id
 UNION ALL SELECT 'reservation',p.id,encode(sha256(convert_to(to_jsonb(p)::text,'UTF8')),'hex') FROM field_service_part_reservations p JOIN visits v ON v.id=p.order_id
 UNION ALL SELECT 'billing',b.id,encode(sha256(convert_to(to_jsonb(b)::text,'UTF8')),'hex') FROM field_service_billing_releases b JOIN visits v ON v.id=b.order_id
)
SELECT CASE WHEN octet_length(value::text)<=2097152 THEN value::text END payload,
 (SELECT encode(sha256(convert_to(coalesce(string_agg(digest,'' ORDER BY kind,id),''),'UTF8')),'hex') FROM hashes) digest
FROM payload
"#).bind(company.as_str()).fetch_one(c).await?;
    let payload:Option<String>=row.try_get("payload")?;
    let data=serde_json::from_str(&payload.ok_or_else(limited)?).map_err(|_|limited())?;
    Ok((data,row.try_get("digest")?))
}

/// This witness proves Field-owned rows only. A consuming grant operation must
/// also retain its current IAM/review and the other four settlement owners.
pub struct CurrentServicePackageSettlement {
    company:CompanyId,
    unit:Unit,
    data:ServiceSettlementRows,
    digest:String,
}
impl CurrentServicePackageSettlement {
    pub fn data(&self)->&ServiceSettlementRows{&self.data}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await?!=self.unit{return Err(changed());}
        let(data,digest)=image(c,&self.company).await?;
        if data!=self.data || digest!=self.digest{return Err(changed());}
        Ok(())
    }
}
pub async fn read_service_package_settlement(c:&mut PgConnection,company:&CompanyId)->Result<CurrentServicePackageSettlement,Error>{
    let unit=unit(c).await?;
    let(data,digest)=image(c,company).await?;
    Ok(CurrentServicePackageSettlement{company:company.clone(),unit,data,digest})
}
