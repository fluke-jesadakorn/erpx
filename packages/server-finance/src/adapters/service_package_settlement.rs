//! Service-only raw Finance debt. Compiled outer composition retains the Field,
//! Inventory and Workflow lineage witnesses in this exact Holding transaction.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,CompanyId,FailureKind};
use serde::Serialize;
use sqlx::{Connection,PgConnection};
use std::collections::BTreeSet;
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String);
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceFinanceSettlementOrder {pub id:String,pub company_id:String}
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceFinanceSettlementRelease {pub id:String,pub order_id:String,pub digest:String}
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceFinanceSettlementPart {pub id:String,pub company_id:String,pub source_id:String,pub journal_id:String,pub currency:String,pub value_minor:i64,pub digest:String}
/// These are same-unit peer facts, not request source labels or capabilities.
/// The outer producer obtains them only from current native lineage witnesses.
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceFinanceSettlementInvoiceOrigin {pub id:String,pub company_id:String,pub release_id:String,pub order_id:String,pub customer_id:String,pub currency:String,pub amount_minor:i64,pub pricing_date:String}
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceFinanceSettlementPartsOrigin {pub company_id:String,pub journal_id:String,pub consumption_id:String}
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub struct ServiceFinanceSettlementReferences {
    pub orders:Vec<ServiceFinanceSettlementOrder>,pub releases:Vec<ServiceFinanceSettlementRelease>,pub parts:Vec<ServiceFinanceSettlementPart>,
    pub invoice_origins:Vec<ServiceFinanceSettlementInvoiceOrigin>,pub parts_origins:Vec<ServiceFinanceSettlementPartsOrigin>,
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct ServiceFinanceSettlementObservation {pub outstanding:i64,pub revision:String}
pub struct CurrentServiceFinanceSettlement {
    company:CompanyId,package_id:String,unit:Unit,references:ServiceFinanceSettlementReferences,observation:ServiceFinanceSettlementObservation,
}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"SERVICE_FINANCE_PACKAGE_SOURCE_CHANGED","The original Finance settlement source changed.").into()}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_service_finance_settlement(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE finance_invoices,finance_payments,finance_allocations,finance_payment_reversals,finance_journals,finance_journal_lines,finance_accounts,finance_charts,finance_book_postings,finance_book_rules,finance_book_account_mappings,finance_bank_statements,finance_bank_transactions,finance_refund_obligations,finance_refund_instructions,finance_billing_corrections,money_transaction_snapshots IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn capture(c:&mut PgConnection,company:&CompanyId,package_id:&str,references:&ServiceFinanceSettlementReferences)->Result<ServiceFinanceSettlementObservation,Error>{
    let base=match package_id{"service"=>true,"service-parts-assets"=>false,_=>return Err(changed())};
    let orders:BTreeSet<_>=references.orders.iter().map(|o|o.id.as_str()).collect();
    if orders.len()!=references.orders.len()||references.orders.iter().any(|o|o.company_id!=company.as_str())||references.releases.iter().any(|b|!orders.contains(b.order_id.as_str()))||references.invoice_origins.iter().any(|i|i.company_id!=company.as_str())||references.parts_origins.iter().any(|p|p.company_id!=company.as_str()){return Err(changed());}
    let input=serde_json::to_value(references).map_err(|_|changed())?;
    let(outstanding,revision):(i64,String)=sqlx::query_as(include_str!("service_package_settlement.sql")).bind(company.as_str()).bind(input).bind(base).fetch_one(c).await?;
    if outstanding<0||revision.len()!=64{return Err(changed());}
    Ok(ServiceFinanceSettlementObservation{outstanding,revision})
}
impl CurrentServiceFinanceSettlement {
    pub fn observation(&self)->&ServiceFinanceSettlementObservation{&self.observation}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await?!=self.unit||capture(c,&self.company,&self.package_id,&self.references).await?!=self.observation{return Err(changed());}
        Ok(())
    }
}
pub async fn read_service_finance_settlement(c:&mut PgConnection,company:&CompanyId,package_id:&str,references:ServiceFinanceSettlementReferences)->Result<CurrentServiceFinanceSettlement,Error>{
    let unit=unit(c).await?;let observation=capture(c,company,package_id,&references).await?;
    Ok(CurrentServiceFinanceSettlement{company:company.clone(),package_id:package_id.into(),unit,references,observation})
}
