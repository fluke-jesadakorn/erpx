#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commerce::public::service_initial_count_masters::*;
use erpx_server_commerce::public::service_stock_masters::lock_service_stock_masters;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
#[tokio::test]
async fn selected_scope_is_active_and_full_master_changes_are_fenced() {
    fixture::with_database(2,|pool|async move{
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let m=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&m).await.unwrap();
 sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('co','company','co','Company','co'); INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES('br','branch','br','Branch','co','co','br'); INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('wh','co','br','W','Warehouse'); INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES('p','co','P','Product',10)").execute(pool.raw()).await.unwrap();
 let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{lock_service_stock_masters(c).await?;let masters=read_service_count_masters(c,"co",Some("br"),Some("wh"),Some("p")).await?;let d=masters.data();assert!(d.warehouse.as_ref().unwrap().starts_with("{\"id\":\"wh\",\"company_id\":\"co\""));assert!(d.product.is_some());assert_eq!(serde_json::from_str::<serde_json::Value>(&d.warehouses).unwrap()[0]["id"],"wh");sqlx::query("UPDATE commerce_products SET name='drift' WHERE id='p'").execute(&mut *c).await?;masters.require_current(c).await})).await;
 assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));
 let name:String=sqlx::query_scalar("SELECT name FROM commerce_products WHERE id='p'").fetch_one(pool.raw()).await.unwrap();assert_eq!(name,"Product");
}).await
}
