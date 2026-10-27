#[path = "../../support/inventory/consumption-facts.rs"]
#[allow(dead_code)]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_quality::public::inventory_consumption::*;
use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;
use facts::*;
use std::sync::Arc;
fn request() -> ConsumptionHoldRequest {
    ConsumptionHoldRequest {
        company_id: company(),
        branch_id: id("branch"),
        warehouse_id: id("warehouse"),
        product_id: id("product"),
        lot_ids: vec![id("lot-1"), id("lot-2")],
    }
}
async fn read(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
) -> Result<CurrentConsumptionHolds, Error> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), |c| {
            Box::pin(async move {
                lock_consumption_sources(c).await?;
                read_consumption_holds(c, &request()).await
            })
        })
        .await
}
#[tokio::test]
async fn actual_partial_lot_and_whole_quantity_decisions_keep_0252_and_0283_distinctions() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, "lot").await;
        let source = read(&pool).await.unwrap();
        assert!(!source.whole_quantity_hold());
        assert_eq!(source.partial_held_milli(), 0);
        assert!(source.blocked_lots().is_empty());
        hold(&pool, "partial", "partial", None, Some(700)).await;
        hold(&pool, "lot", "partial", Some("lot-1"), Some(1000)).await;
        let source = read(&pool).await.unwrap();
        assert_eq!(source.partial_held_milli(), 1700);
        assert_eq!(source.blocked_lots(), [id("lot-1")]);
        assert!(!source.unscoped_whole_quantity_hold());
        hold(&pool, "whole-lot", "whole", Some("lot-2"), Some(1000)).await;
        let source = read(&pool).await.unwrap();
        assert!(source.whole_quantity_hold());
        assert!(!source.unscoped_whole_quantity_hold());
        hold(&pool, "whole", "whole", None, None).await;
        assert!(read(&pool).await.unwrap().unscoped_whole_quantity_hold());
    })
    .await;
}
#[tokio::test]
async fn effective_hold_facts_ignore_giant_reason_bodies_but_not_new_denials_or_cross_transaction_proofs()
 {
    fixture::with_database(1,|pool|async move{setup(&pool,"lot").await;hold(&pool,"held","partial",None,Some(500)).await;
 PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{lock_consumption_sources(c).await?;let source=read_consumption_holds(c,&request()).await?;sqlx::query("UPDATE inventory_quality_holds SET reason=repeat('r',36*1024*1024) WHERE id='held'").execute(&mut *c).await?;source.require_current(c).await})).await.unwrap();
 let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{lock_consumption_sources(c).await?;let source=read_consumption_holds(c,&request()).await?;sqlx::query("UPDATE inventory_quality_holds SET held_quantity_milli=501 WHERE id='held'").execute(&mut *c).await?;source.require_current(c).await})).await;denied(result,"QUALITY_CONSUMPTION_SOURCE_CHANGED");
 let source=Arc::new(read(&pool).await.unwrap());let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|{let source=source.clone();Box::pin(async move{lock_consumption_sources(c).await?;source.require_current(c).await})}).await;denied(result,"QUALITY_CONSUMPTION_SOURCE_CHANGED");
 }).await;
}
#[tokio::test]
async fn aggregate_partial_sum_caps_before_bigint_conversion_and_terminal_cancellation_is_excluded()
{
    fixture::with_database(2,|pool|async move{setup(&pool,"none").await;hold(&pool,"large-1","partial",None,Some(i64::MAX)).await;hold(&pool,"large-2","partial",None,Some(i64::MAX)).await;assert_eq!(read(&pool).await.unwrap().partial_held_milli(),i64::MAX);
 sqlx::raw_sql("UPDATE inventory_quality_holds SET status='cancel_pending',cancel_requested_by='requester' WHERE id='large-1';UPDATE inventory_quality_holds SET status='cancelled',cancelled_by='reviewer',cancelled_at=1 WHERE id='large-1';UPDATE inventory_quality_holds SET status='cancel_pending',cancel_requested_by='requester' WHERE id='large-2';UPDATE inventory_quality_holds SET status='cancelled',cancelled_by='reviewer',cancelled_at=1 WHERE id='large-2'").execute(pool.raw()).await.unwrap();assert_eq!(read(&pool).await.unwrap().partial_held_milli(),0);
 }).await;
}
#[tokio::test]
async fn a_relevant_foreign_company_hold_is_not_hidden_from_denial_facts() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, "none").await;
        hold(&pool, "held", "whole", None, None).await;
        sqlx::query("UPDATE inventory_quality_holds SET company_id='foreign' WHERE id='held'")
            .execute(pool.raw())
            .await
            .unwrap();
        denied(read(&pool).await, "QUALITY_CONSUMPTION_SCOPE_INVALID");
    })
    .await;
}
#[tokio::test]
async fn a_new_hold_committed_while_utility_absence_fence_waits_is_visible() {
    fixture::with_database(3,|pool|async move{setup(&pool,"none").await;let mut writer=pool.raw().begin().await.unwrap();sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at)VALUES('held','company','branch','warehouse','product','Hold','2026-12-31','maker',0,0)").execute(&mut *writer).await.unwrap();let read_pool=pool.clone();let task=tokio::spawn(async move{read(&read_pool).await});
 tokio::time::timeout(std::time::Duration::from_secs(10),async{loop{let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareLock' AND relation='inventory_quality_holds'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();if blocked{break}tokio::task::yield_now().await;}}).await.unwrap();writer.commit().await.unwrap();assert!(task.await.unwrap().unwrap().whole_quantity_hold());}).await;
}
