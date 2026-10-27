#[path = "../../support/inventory/consumption-facts.rs"]
#[allow(dead_code)]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commerce::public::inventory_consumption::*;
use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, CompanyId};
use facts::*;
use std::sync::Arc;
fn request() -> ConsumptionCommerceRequest {
    ConsumptionCommerceRequest {
        company_id: company(),
        branch_id: id("branch"),
        warehouse_id: id("warehouse"),
        product_id: id("product"),
    }
}
async fn read(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
) -> Result<CurrentConsumptionCommerce, Error> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), |c| {
            Box::pin(async move {
                lock_consumption_sources(c).await?;
                Ok(read_consumption_source(c, &request()).await?.unwrap())
            })
        })
        .await
}
#[tokio::test]
async fn actual_current_ids_tracking_and_posting_settings_ignore_irrelevant_large_bodies_and_settings_currency()
 {
    fixture::with_database(2,|pool|async move{setup(&pool,"lot").await;
 PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
 lock_consumption_sources(c).await?;let source=read_consumption_source(c,&request()).await?.unwrap();assert_eq!(source.tracking_mode(),CommerceTrackingMode::Lot);let s=source.posting_settings().unwrap();assert_eq!(s.chart_id.as_str(),"chart");assert_eq!(s.cogs_account_id.as_str(),"expense");
 sqlx::query("UPDATE commerce_products SET name=repeat('n',36*1024*1024) WHERE id='product'").execute(&mut *c).await?;
 sqlx::query("UPDATE commerce_settings SET currency='EUR' WHERE company_id='company'").execute(&mut *c).await?;source.require_current(c).await
 })).await.unwrap();}).await;
}
#[tokio::test]
async fn wrong_company_branch_product_and_archived_sources_are_absent_but_missing_settings_remains_a_fact()
 {
    fixture::with_database(2, |pool| async move {
        setup(&pool, "none").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_consumption_sources(c).await?;
                    for kind in 0..4 {
                        let mut r = request();
                        match kind {
                            0 => r.company_id = CompanyId::new("foreign").unwrap(),
                            1 => r.branch_id = id("foreign"),
                            2 => r.product_id = id("foreign"),
                            _ => r.warehouse_id = id("foreign"),
                        };
                        assert!(read_consumption_source(c, &r).await?.is_none());
                    }
                    sqlx::query("DELETE FROM commerce_settings WHERE company_id='company'")
                        .execute(&mut *c)
                        .await?;
                    assert!(
                        read_consumption_source(c, &request())
                            .await?
                            .unwrap()
                            .posting_settings()
                            .is_none()
                    );
                    sqlx::query(
                        "UPDATE commerce_products SET status='archived' WHERE id='product'",
                    )
                    .execute(&mut *c)
                    .await?;
                    assert!(read_consumption_source(c, &request()).await?.is_none());
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
#[tokio::test]
async fn retained_settings_and_versions_reject_same_unit_change_and_same_backend_new_transaction() {
    fixture::with_database(1,|pool|async move{setup(&pool,"none").await;
 let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{lock_consumption_sources(c).await?;let source=read_consumption_source(c,&request()).await?.unwrap();sqlx::query("UPDATE commerce_products SET version=version+1 WHERE id='product'").execute(&mut *c).await?;source.require_current(c).await})).await;denied(result,"COMMERCE_CONSUMPTION_SOURCE_CHANGED");
 let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{lock_consumption_sources(c).await?;let source=read_consumption_source(c,&request()).await?.unwrap();sqlx::query("UPDATE commerce_settings SET cogs_account_id='inventory' WHERE company_id='company'").execute(&mut *c).await?;source.require_current(c).await})).await;denied(result,"COMMERCE_CONSUMPTION_SOURCE_CHANGED");
 let source=Arc::new(read(&pool).await.unwrap());let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|{let source=source.clone();Box::pin(async move{lock_consumption_sources(c).await?;source.require_current(c).await})}).await;denied(result,"COMMERCE_CONSUMPTION_SOURCE_CHANGED");
 }).await;
}
#[tokio::test]
async fn archival_committed_while_utility_fence_waits_is_seen_before_first_snapshot() {
    fixture::with_database(3,|pool|async move{setup(&pool,"none").await;let mut writer=pool.raw().begin().await.unwrap();sqlx::query("UPDATE commerce_products SET status='archived' WHERE id='product'").execute(&mut *writer).await.unwrap();
 let read_pool=pool.clone();let task=tokio::spawn(async move{PgUnitOfWork::new(read_pool).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{lock_consumption_sources(c).await?;Ok(read_consumption_source(c,&request()).await?.is_none())})).await});
 tokio::time::timeout(std::time::Duration::from_secs(10),async{loop{let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareLock' AND relation='commerce_products'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();if blocked{break}tokio::task::yield_now().await;}}).await.unwrap();writer.commit().await.unwrap();assert!(task.await.unwrap().unwrap());}).await;
}
