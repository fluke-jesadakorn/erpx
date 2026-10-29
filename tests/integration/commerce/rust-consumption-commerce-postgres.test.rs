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

#[tokio::test]
async fn service_posting_commerce_witness_retains_mapping_history_and_same_unit() {
    use erpx_server_commerce::public::service_parts_posting::*;
    fixture::with_database(2, |pool| async move {
        setup(&pool, "none").await;
        let original = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_service_parts_posting_sources(c).await?;
                    let current = read_service_parts_posting(c, &company()).await?;
                    assert!(current.facts().schema_available);
                    assert!(current.facts().mapping_json.is_some());
                    assert!(current.facts().source_json.is_none());
                    assert!(!current.facts().has_commerce_history);
                    assert_eq!(current.facts().commerce_orders_json, "null");
                    assert_eq!(current.facts().warehouse_ids, vec!["warehouse"]);
                    let foreign =
                        read_service_parts_posting(c, &CompanyId::new("foreign").unwrap()).await?;
                    assert!(foreign.facts().mapping_json.is_none());
                    assert!(foreign.facts().source_json.is_none());
                    assert!(!foreign.facts().has_commerce_history);
                    assert!(foreign.facts().warehouse_ids.is_empty());
                    assert_eq!(
                        posting_admission(PostingMode::Initialize, true, false, true),
                        PostingAdmission::ExistingMappingRequiresConfirmation
                    );
                    assert_eq!(
                        posting_admission(PostingMode::ConfirmExisting, true, false, true),
                        PostingAdmission::Allowed
                    );
                    current.require_current(c).await?;
                    Ok(current)
                })
            })
            .await
            .unwrap();
        let original = Arc::new(original);
        let cross = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                let original = original.clone();
                Box::pin(async move { original.require_current(c).await })
            })
            .await;
        denied(cross, "SERVICE_PARTS_POSTING_SOURCE_CHANGED");
        let drift = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_service_parts_posting_sources(c).await?;
                    let current = read_service_parts_posting(c, &company()).await?;
                    sqlx::query(
                        "UPDATE commerce_settings SET currency='EUR' WHERE company_id='company'",
                    )
                    .execute(&mut *c)
                    .await?;
                    current.require_current(c).await
                })
            })
            .await;
        denied(drift, "SERVICE_PARTS_POSTING_SOURCE_CHANGED");
        let currency: String =
            sqlx::query_scalar("SELECT currency FROM commerce_settings WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(currency, "USD");
    })
    .await;
}

#[tokio::test]
async fn service_posting_lock_waits_before_serializable_snapshot() {
    use erpx_server_commerce::public::service_parts_posting::*;
    fixture::with_database(3, |pool| async move {
        setup(&pool, "none").await;
        let mut writer = pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE commerce_settings SET currency='EUR' WHERE company_id='company'").execute(&mut *writer).await.unwrap();
        let read_pool = pool.clone();
        let task = tokio::spawn(async move {
            PgUnitOfWork::new(read_pool).run::<_,BusinessError,_>(TransactionOptions::default(), |c| Box::pin(async move {
                lock_service_parts_posting_sources(c).await?;
                let current = read_service_parts_posting(c, &company()).await?;
                current.require_current(c).await?;
                Ok(current.facts().mapping_json.clone().unwrap())
            })).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(10),async {
            loop {
                let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareRowExclusiveLock' AND relation='commerce_settings'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();
                if waiting { break; }
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        writer.commit().await.unwrap();
        let mapping=task.await.unwrap().unwrap();
        assert!(mapping.contains("\"currency\": \"EUR\""));
    }).await;
}

#[tokio::test]
async fn service_posting_absent_schema_is_reported_without_leaving_transaction_aborted() {
    use erpx_server_commerce::public::service_parts_posting::*;
    fixture::with_database(2, |pool| async move {
        setup(&pool, "none").await;
        let result = PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            sqlx::query("ALTER TABLE commerce_service_parts_posting_sources RENAME TO fixture_hidden_service_sources").execute(&mut *c).await?;
            lock_service_parts_posting_sources(c).await?;
            let current=read_service_parts_posting(c,&company()).await?;
            assert!(!current.facts().schema_available);
            assert!(current.facts().source_json.is_none());
            Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"FIXTURE_ROLLBACK","Fixture rollback").into())
        })).await;
        denied(result, "FIXTURE_ROLLBACK");
        let present:bool=sqlx::query_scalar("SELECT to_regclass('commerce_service_parts_posting_sources') IS NOT NULL").fetch_one(pool.raw()).await.unwrap();
        assert!(present);
    }).await;
}
