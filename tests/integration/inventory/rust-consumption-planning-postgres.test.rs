#[path = "../../support/inventory/consumption-facts.rs"]
#[allow(dead_code)]
mod facts;
#[path = "../../support/inventory/consumption-finance.rs"]
mod finance;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_finance::public::inventory_consumption::CurrentConsumptionFinance;
use erpx_server_inventory::public::{consumption::*, postgres_consumption::*};
use erpx_server_runtime::public::postgres::{DatabasePool, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;
use facts::*;
use finance::FinanceFacts;
use std::sync::Arc;
fn selection(quantity: i64) -> ConsumptionSelection {
    ConsumptionSelection {
        company_id: company(),
        branch_id: id("branch"),
        warehouse_id: id("warehouse"),
        product_id: id("product"),
        entry_date: "2026-01-10".into(),
        quantity_milli: quantity,
        allocations: vec![],
    }
}
fn lot(id: &str, quantity: i64) -> LotAllocation {
    LotAllocation {
        lot_id: id.into(),
        quantity_milli: quantity,
        balance_version: 1,
    }
}
async fn prepare(
    pool: &DatabasePool,
    s: ConsumptionSelection,
) -> Result<PgConsumptionPreparation<CurrentConsumptionFinance>, Error> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let s = s.clone();
            Box::pin(async move {
                let mut f = FinanceFacts;
                lock_inventory_consumption_sources(c, &mut f).await?;
                prepare_inventory_consumption(c, &mut f, s).await
            })
        })
        .await
}
#[tokio::test]
async fn real_lot_inputs_use_current_public_owner_facts_without_effects_and_keep_actual_base_currency()
 {
    fixture::with_database(2,|pool|async move{setup(&pool,"lot").await;let mut s=selection(4000);s.allocations=vec![lot("lot-1",3000),lot("lot-2",1000)];
 PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|{let s=s.clone();Box::pin(async move{let mut f=FinanceFacts;lock_inventory_consumption_sources(c,&mut f).await?;let source=prepare_inventory_consumption(c,&mut f,s).await?;assert_eq!(source.plan().valuation.value_minor,800);assert_eq!(source.plan().config.currency,"THB");assert_eq!(source.plan().config.base_currency,"THB");assert_eq!(source.plan().config.expense_account_id.as_str(),"expense");assert_eq!(source.plan().period_id.as_str(),"period");assert_eq!(source.plan().valuation.lots.len(),2);source.require_current(c,&mut f).await})}).await.unwrap();
 let effects:(i64,i64,i64)=sqlx::query_as("SELECT(SELECT count(*)FROM finance_journals),(SELECT count(*)FROM money_transaction_snapshots),(SELECT count(*)FROM operational_inventory_consumptions)").fetch_one(pool.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
 }).await;
}
#[tokio::test]
async fn actual_untracked_and_whole_serial_costs_retain_canonical_valuation() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, "none").await;
        assert_eq!(
            prepare(&pool, selection(5000))
                .await
                .unwrap()
                .plan()
                .valuation
                .value_minor,
            1000
        );
        assert_eq!(
            prepare(&pool, selection(2500))
                .await
                .unwrap()
                .plan()
                .valuation
                .value_minor,
            500
        );
    })
    .await;
    fixture::with_database(2, |pool| async move {
        setup(&pool, "serial").await;
        let mut s = selection(1000);
        s.allocations = vec![lot("serial-0", 1000)];
        assert_eq!(
            prepare(&pool, s.clone())
                .await
                .unwrap()
                .plan()
                .valuation
                .value_minor,
            200
        );
        s.quantity_milli = 500;
        s.allocations[0].quantity_milli = 500;
        denied(prepare(&pool, s).await, "PART_LOT_CHANGED");
    })
    .await;
}
#[tokio::test]
async fn current_scope_config_period_and_lot_failures_preserve_b0_planning_order() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, "lot").await;
        let mut s = selection(1000);
        s.branch_id = id("foreign");
        denied(prepare(&pool, s).await, "PART_STOCK_UNAVAILABLE");
        let mut s = selection(1000);
        s.allocations = vec![lot("lot-1", 1000)];
        s.allocations[0].balance_version = 99;
        sqlx::query("UPDATE finance_accounts SET status='inactive' WHERE id='expense'")
            .execute(pool.raw())
            .await
            .unwrap();
        denied(prepare(&pool, s.clone()).await, "PART_LOT_CHANGED");
        sqlx::query("UPDATE finance_periods SET state='closed' WHERE id='period'")
            .execute(pool.raw())
            .await
            .unwrap();
        denied(
            prepare(&pool, s.clone()).await,
            "PART_POSTING_PERIOD_REQUIRED",
        );
        sqlx::query("DELETE FROM commerce_settings WHERE company_id='company'")
            .execute(pool.raw())
            .await
            .unwrap();
        denied(prepare(&pool, s).await, "PART_FUNCTIONAL_CURRENCY_REQUIRED");
    })
    .await;
}
#[tokio::test]
async fn same_unit_inventory_commerce_quality_and_finance_changes_reject_retained_planning_inputs()
{
    fixture::with_database(2,|pool|async move{setup(&pool,"none").await;hold(&pool,"partial","partial",None,Some(10)).await;
 for (sql,code) in [("UPDATE commerce_stock SET quantity_milli=4000,value_minor=800,version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'","INVENTORY_CONSUMPTION_SOURCE_CHANGED"),("UPDATE commerce_products SET version=version+1 WHERE id='product'","COMMERCE_CONSUMPTION_SOURCE_CHANGED"),("UPDATE inventory_quality_holds SET held_quantity_milli=11 WHERE id='partial'","QUALITY_CONSUMPTION_SOURCE_CHANGED"),("UPDATE finance_periods SET state='closed' WHERE id='period'","INVENTORY_FINANCE_SOURCE_CHANGED")]{
 let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{let mut f=FinanceFacts;lock_inventory_consumption_sources(c,&mut f).await?;let source=prepare_inventory_consumption(c,&mut f,selection(1000)).await?;sqlx::query(sql).execute(&mut *c).await?;source.require_current(c,&mut f).await})).await;denied(result,code);
 }
 }).await;
}
#[tokio::test]
async fn selected_lot_identity_and_balance_changes_reject_the_original_planning_proof() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, "lot").await;
        for sql in [
            "UPDATE inventory_lots SET lot_code='different-physical-lot' WHERE id='lot-1'",
            "UPDATE inventory_lot_balances SET version=version+1 WHERE lot_id='lot-1' AND warehouse_id='warehouse'",
        ] {
            let result = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                    Box::pin(async move {
                        let mut f = FinanceFacts;
                        lock_inventory_consumption_sources(c, &mut f).await?;
                        let mut input = selection(1000);
                        input.allocations = vec![lot("lot-1", 1000)];
                        let source = prepare_inventory_consumption(c, &mut f, input).await?;
                        sqlx::query(sql).execute(&mut *c).await?;
                        source.require_current(c, &mut f).await
                    })
                })
                .await;
            denied(result, "INVENTORY_CONSUMPTION_SOURCE_CHANGED");
        }
        sqlx::query("UPDATE inventory_lots SET lot_code=repeat('l',201) WHERE id='lot-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut input = selection(1000);
        input.allocations = vec![lot("lot-1", 1000)];
        denied(prepare(&pool, input).await, "INVENTORY_CONSUMPTION_SOURCE_LIMIT");
    }).await;
}
#[tokio::test]
async fn a_plain_plan_and_active_hold_are_evidence_not_a_cut_stock_capability_and_proofs_do_not_cross_units()
 {
    fixture::with_database(1,|pool|async move{setup(&pool,"lot").await;hold(&pool,"held","partial",Some("lot-1"),Some(1000)).await;let mut s=selection(1000);s.allocations=vec![lot("lot-1",1000)];let source=Arc::new(prepare(&pool,s).await.unwrap());assert_eq!(source.quality().blocked_lots(),[id("lot-1")]);let _data_only=source.plan().clone();
 let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|{let source=source.clone();Box::pin(async move{let mut f=FinanceFacts;lock_inventory_consumption_sources(c,&mut f).await?;source.require_current(c,&mut f).await})}).await;denied(result,"INVENTORY_CONSUMPTION_SOURCE_CHANGED");
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap(),5000);
 }).await;
}
#[tokio::test]
async fn committed_stock_change_before_utility_acquisition_is_seen_at_first_snapshot() {
    fixture::with_database(3,|pool|async move{setup(&pool,"none").await;let mut writer=pool.raw().begin().await.unwrap();sqlx::query("UPDATE commerce_stock SET quantity_milli=4000,value_minor=800,version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *writer).await.unwrap();let read_pool=pool.clone();let task=tokio::spawn(async move{prepare(&read_pool,selection(4500)).await});
 tokio::time::timeout(std::time::Duration::from_secs(10),async{loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareRowExclusiveLock' AND relation='commerce_stock'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();if waiting{break}tokio::task::yield_now().await;}}).await.unwrap();writer.commit().await.unwrap();denied(task.await.unwrap(),"PART_STOCK_UNAVAILABLE");}).await;
}
