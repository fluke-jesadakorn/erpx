//! Inventory-only revision proof; synthetic source receipts are rolled back.
#[path = "../../support/inventory/consumption-facts.rs"]
#[allow(dead_code)]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::{
    service_initial_count_revision::*, service_initial_physical_count::*,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;

#[tokio::test]
async fn revision_replaces_only_planned_inventory_and_rolls_back_with_owner_unit() {
    fixture::with_database(2,|pool|async move {
  facts::setup(&pool,"none").await;
  sqlx::query("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").execute(pool.raw()).await.unwrap();
  sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0)").execute(pool.raw()).await.unwrap();
  for (tracking,existing_lot) in [(TrackingMode::None,false),(TrackingMode::Lot,false),(TrackingMode::Lot,true)] {
   let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
    lock_initial_count_inventory_sources(c).await?;
    sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
    let history=read_initial_count_inventory_history(c,"warehouse","product",None).await?;
    let draft=InitialCountDraft{proof:InitialCountSourceProof{count_id:"count-cmd".into(),selection:InitialCountSelection{company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into()},count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis{chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into()}},product:InitialCountProductSnapshot{version:1,description:"Product".into(),unit:"piece".into(),tracking},reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(tracking,0,None,vec![]).unwrap(),created_at:1};
    insert_initial_count_draft(c,&draft,&history).await?;
    if existing_lot { sqlx::raw_sql("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES('existing-lot','company','product','lot','Lot-A','maker',1); INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES('existing-lot','warehouse',2000,200)").execute(&mut *c).await?; }
    let before=read_initial_count_record(c,"company",Some("branch"),"count-cmd").await?.unwrap();
    let pair=read_initial_count_inventory_history(c,"warehouse","product",Some("count-cmd")).await?;
    let allocations=if tracking==TrackingMode::Lot {vec![InitialCountRevisionAllocation{lot_id:if existing_lot {Some("existing-lot".into())} else {None},lot_code:Some("Lot-A".into()),serial_number:None,manufactured_date:None,expiry_date:None,quantity_milli:2000,unit_cost_minor:if existing_lot {None} else {Some(100)}}]}else{vec![]};
    let input=InitialCountRevision{company_id:"company".into(),branch_id:"branch".into(),count_id:"count-cmd".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),expected_version:1,count_date:"2026-01-11".into(),reason:"Revised actual count".into(),actor_id:"maker".into(),now:2,product:draft.product.clone(),quantity_milli:2000,unit_cost_minor:if tracking==TrackingMode::None{Some(100)}else{None},allocations};
    let plan=prepare_initial_count_revision(c,&before,&pair,input).await?;plan.apply(c,&before,&pair).await?;plan.require_current(c).await?;
    let revised=read_initial_count_record(c,"company",Some("branch"),"count-cmd").await?.unwrap();let image:serde_json::Value=serde_json::from_str(&revised.data().image).unwrap();assert_eq!(image["count"]["version"],2);assert_eq!(image["count"]["status"],"draft");assert_eq!(image["lines"][0]["counted_value_minor"],200);
    sqlx::query("UPDATE inventory_counts SET reason='tampered' WHERE id='count-cmd'").execute(&mut *c).await?;assert!(plan.require_current(c).await.is_err());
    Err(BusinessError::invalid("ROLLBACK_FIXTURE","Rollback synthetic receipt").into())
   })).await;
   assert!(matches!(result,Err(PgTransactionError::Business(e)) if e.code=="ROLLBACK_FIXTURE"));
   let count:i64=sqlx::query_scalar("SELECT count(*) FROM inventory_counts WHERE id='count-cmd'").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);
  }
 }).await;
}
