//! Raw Service base/addon obligations. Field supplies scoped owning references;
//! Files supplies its compiled version relation. This is not write authority.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind};
use serde::Serialize;
use sqlx::{Connection,PgConnection};
use std::collections::BTreeSet;
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceInventorySettlementOrder {pub id:String,pub company_id:String,pub branch_id:String,pub department_id:Option<String>,pub version:i64}
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceInventorySettlementReservation {pub id:String,pub order_id:String,pub warehouse_id:String,pub product_id:String,pub quantity_milli:i64,pub consumed_milli:i64,pub released_milli:i64,pub digest:String}
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceInventorySettlementEvent {pub order_id:String,pub version:i64,pub action:String,pub details_json:serde_json::Value}
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceInventorySettlementReferences {pub orders:Vec<ServiceInventorySettlementOrder>,pub reservations:Vec<ServiceInventorySettlementReservation>,pub events:Vec<ServiceInventorySettlementEvent>}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct ServiceInventorySettlementObservation {pub outstanding:i64,pub revision:String}
pub struct CurrentServiceInventorySettlement {
    company:CompanyId,unit:Unit,references:ServiceInventorySettlementReferences,
    record_version_steps:&'static str,observation:ServiceInventorySettlementObservation,
}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"SERVICE_INVENTORY_PACKAGE_SOURCE_CHANGED","The original Inventory settlement source changed.").into()}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// The complete outer preparation also acquires Workflow receipts, Files
/// record-version and the other Field settlement owners before any snapshot.
pub async fn lock_service_inventory_settlement(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("INVENTORY_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE commerce_products,commerce_warehouses,commerce_stock,inventory_lots,inventory_lot_balances,inventory_lot_moves,operational_inventory_consumptions,operational_inventory_consumption_lots,inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves,inventory_service_initial_physical_sources IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn capture(c:&mut PgConnection,company:&CompanyId,references:&ServiceInventorySettlementReferences,record_version_steps:&'static str)->Result<ServiceInventorySettlementObservation,Error>{
    let ids:BTreeSet<_>=references.orders.iter().map(|o|o.id.as_str()).collect();
    if ids.len()!=references.orders.len()||references.orders.iter().any(|o|o.company_id!=company.as_str())||references.reservations.iter().any(|r|!ids.contains(r.order_id.as_str()))||references.events.iter().any(|e|!ids.contains(e.order_id.as_str())){return Err(changed());}
    let input=serde_json::to_value(references).map_err(|_|changed())?;
    // The only SQL substitution is an explicit compiled public Files relation,
    // never request text. All Company and Field data use bound JSON parameters.
    let sql=include_str!("service_package_settlement.sql").replace("__RECORD_VERSION_STEPS__",record_version_steps);
    let(outstanding,revision):(i64,String)=sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(company.as_str()).bind(input).fetch_one(c).await?;
    let result=ServiceInventorySettlementObservation{outstanding,revision};
    if result.outstanding<0||result.revision.len()!=64{return Err(changed());}
    Ok(result)
}
impl CurrentServiceInventorySettlement {
    pub fn observation(&self)->&ServiceInventorySettlementObservation{&self.observation}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await?!=self.unit||capture(c,&self.company,&self.references,self.record_version_steps).await?!=self.observation{return Err(changed());}
        Ok(())
    }
}
pub async fn read_service_inventory_settlement(c:&mut PgConnection,company:&CompanyId,references:ServiceInventorySettlementReferences,record_version_steps:&'static str)->Result<CurrentServiceInventorySettlement,Error>{
    let unit=unit(c).await?;let observation=capture(c,company,&references,record_version_steps).await?;
    Ok(CurrentServiceInventorySettlement{company:company.clone(),unit,references,record_version_steps,observation})
}
