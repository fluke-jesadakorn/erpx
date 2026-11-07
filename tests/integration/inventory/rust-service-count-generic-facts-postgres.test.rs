//! Source-bound Inventory projection from a disposable synthetic Count source.
#[path = "../../support/inventory/consumption-facts.rs"]
#[allow(dead_code)] mod facts;
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_server_inventory::public::{service_count_generic_detail::*,service_initial_physical_count::*};
use erpx_server_runtime::public::postgres::{PgTransactionError,PgUnitOfWork,TransactionOptions};
use erpx_server_shared::public::BusinessError;

#[tokio::test]
async fn facts_are_scoped_and_capture_current_stock_and_lot_balances() {
 fixture::with_database(2,|pool|async move{
  facts::setup(&pool,"none").await;
  sqlx::query("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").execute(pool.raw()).await.unwrap();
  sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0)").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_initial_count_inventory_sources(c).await?;
   sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
   let history=read_initial_count_inventory_history(c,"warehouse","product",None).await?;
   let draft=InitialCountDraft{proof:InitialCountSourceProof{count_id:"count-cmd".into(),selection:InitialCountSelection{company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into()},count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis{chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into()}},product:InitialCountProductSnapshot{version:1,description:"Product".into(),unit:"piece".into(),tracking:TrackingMode::Lot},reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(TrackingMode::Lot,0,None,vec![]).unwrap(),created_at:1};
   insert_initial_count_draft(c,&draft,&history).await?;
   let empty_source=read_initial_count_record(c,"company",Some("branch"),"count-cmd").await?.unwrap();
   let empty=read_service_count_generic_facts(c,&empty_source).await?;
   assert!(empty["lines"][0]["current_quantity_milli"].is_null());assert!(empty["lines"][0]["current_value_minor"].is_null());assert_eq!(empty["lines"][0]["current_version"],0);
   sqlx::raw_sql("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES('warehouse','product',2000,400); INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES('lot','company','product','lot','A','maker',1); INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES('lot','warehouse',2000,400); INSERT INTO inventory_count_lot_lines(id,count_line_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) VALUES('line-lot','count-cmd-0','lot','lot','A',0,0,0,0,0)").execute(&mut *c).await?;
   let source=read_initial_count_record(c,"company",Some("branch"),"count-cmd").await?.unwrap();
   let value=read_service_count_generic_facts(c,&source).await?;
   assert_eq!(value["count"]["id"],"count-cmd");assert_eq!(value["lines"][0]["current_quantity_milli"],2000);assert_eq!(value["lines"][0]["current_value_minor"],400);assert_eq!(value["lotLines"][0]["current_quantity_milli"],2000);assert_eq!(value["lotLines"][0]["current_value_minor"],400);assert_eq!(value["decisions"],serde_json::json!([]));
   sqlx::query("UPDATE inventory_counts SET reason='changed' WHERE id='count-cmd'").execute(&mut *c).await?;assert!(read_service_count_generic_facts(c,&source).await.is_err());
   Err(BusinessError::invalid("ROLLBACK_FIXTURE","Rollback synthetic receipt").into())
  })).await;
  assert!(matches!(result,Err(PgTransactionError::Business(e)) if e.code=="ROLLBACK_FIXTURE"));
 }).await;
}
