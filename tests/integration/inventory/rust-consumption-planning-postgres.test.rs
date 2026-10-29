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

async fn guarded_unreserved(
    pool: &DatabasePool,
    input: ConsumptionSelection,
    check_lots: bool,
) -> Result<(), Error> {
    PgUnitOfWork::new(pool.clone())
        .run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
            let input = input.clone();
            Box::pin(async move {
                let mut finance = FinanceFacts;
                lock_unreserved_consumption_sources(c).await?;
                lock_inventory_consumption_sources(c, &mut finance).await?;
                let source = prepare_inventory_consumption(c, &mut finance, input).await?;
                source
                    .require_unreserved_stock_and_movement_date(c, &mut finance)
                    .await?;
                if check_lots {
                    for index in 0..source.plan().valuation.lots.len() {
                        source.require_selected_lot_available(c, index).await?;
                    }
                }
                Ok(())
            })
        })
        .await
}
async fn reservation_fixture(pool: &DatabasePool) {
    sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('reservation-party','company','customer','SYNTHETIC','Private synthetic party','THB',0,0);INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)VALUES('reservation-order','company','branch','sales','SYNTHETIC-RESERVATION','reservation-party','warehouse','THB','2026-01-01','2026-01-31',1000,0,'maker',0,0);INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal)VALUES('reservation-line','reservation-order','product','Synthetic stock source',5000,200,0,1000,0,0);INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at)VALUES('reservation','reservation-line','warehouse','product',4000,floor(extract(epoch FROM statement_timestamp())*1000)::bigint+60000,'maker','Synthetic reservation',0)").execute(pool.raw()).await.unwrap();
}
#[tokio::test]
async fn actual_active_reservations_reduce_unreserved_capacity_and_expiry_uses_the_owned_view() {
    fixture::with_database(2,|pool|async move{setup(&pool,"none").await;reservation_fixture(&pool).await;
  guarded_unreserved(&pool,selection(1000),false).await.unwrap();denied(guarded_unreserved(&pool,selection(1001),false).await,"CONFLICT");
  sqlx::query("UPDATE inventory_order_reservations SET expires_at=1 WHERE id='reservation'").execute(pool.raw()).await.unwrap();guarded_unreserved(&pool,selection(5000),false).await.unwrap();
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(pool.raw()).await.unwrap(),5000);
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM operational_inventory_consumptions").fetch_one(pool.raw()).await.unwrap(),0);
 }).await;
}
#[tokio::test]
async fn retained_quality_partial_whole_and_selected_lot_guards_are_not_inferred_from_valuation() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, "lot").await;
        hold(&pool, "selected-hold", "partial", Some("lot-1"), Some(1000)).await;
        let mut selected = selection(1000);
        selected.allocations = vec![lot("lot-1", 1000)];
        guarded_unreserved(&pool, selected.clone(), false)
            .await
            .unwrap();
        denied(guarded_unreserved(&pool, selected, true).await, "CONFLICT");
        let mut other = selection(1000);
        other.allocations = vec![lot("lot-2", 1000)];
        guarded_unreserved(&pool, other.clone(), true)
            .await
            .unwrap();
        hold(&pool, "whole-hold", "whole", None, None).await;
        denied(guarded_unreserved(&pool, other, true).await, "CONFLICT");
    })
    .await;
}
#[tokio::test]
async fn actual_future_movement_denies_backdating_while_equal_date_and_preparation_order_remain() {
    fixture::with_database(2,|pool|async move{setup(&pool,"none").await;
  sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,created_by,prepared_by,created_at,updated_at)VALUES('future-count','company','branch','warehouse','SYNTHETIC-FUTURE','2026-01-11','Synthetic date source','maker','maker',0,0);INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at)VALUES('future-move','future-count','warehouse','product',0,0,'2026-01-11',0)").execute(pool.raw()).await.unwrap();
  denied(guarded_unreserved(&pool,selection(1000),false).await,"CONFLICT");denied(guarded_unreserved(&pool,selection(6000),false).await,"PART_STOCK_UNAVAILABLE");let mut same=selection(1000);same.entry_date="2026-01-11".into();guarded_unreserved(&pool,same,false).await.unwrap();
 }).await;
}
#[tokio::test]
async fn reservation_writer_before_view_fence_is_seen_by_the_first_consumption_snapshot() {
    fixture::with_database(3,|pool|async move{setup(&pool,"none").await;reservation_fixture(&pool).await;sqlx::query("UPDATE inventory_order_reservations SET quantity_milli=1000 WHERE id='reservation'").execute(pool.raw()).await.unwrap();let mut writer=pool.raw().begin().await.unwrap();sqlx::query("UPDATE inventory_order_reservations SET quantity_milli=4000 WHERE id='reservation'").execute(&mut *writer).await.unwrap();
  let other=pool.clone();let reader=tokio::spawn(async move{guarded_unreserved(&other,selection(2000),false).await});
  let observed=tokio::time::timeout(std::time::Duration::from_secs(3),async{loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareRowExclusiveLock'AND relation='inventory_order_reservations'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.is_ok();writer.commit().await.unwrap();let outcome=reader.await.unwrap();assert!(observed,"actual reservation source writer did not block the view utility fence");denied(outcome,"CONFLICT");
 }).await;
}
#[tokio::test]
async fn reservation_mutation_after_preparation_cannot_reuse_the_original_stock_version() {
    fixture::with_database(2,|pool|async move{setup(&pool,"none").await;reservation_fixture(&pool).await;
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{let mut f=FinanceFacts;lock_unreserved_consumption_sources(c).await?;lock_inventory_consumption_sources(c,&mut f).await?;let source=prepare_inventory_consumption(c,&mut f,selection(1000)).await?;sqlx::query("UPDATE inventory_order_reservations SET quantity_milli=4500 WHERE id='reservation'").execute(&mut *c).await?;source.require_unreserved_stock_and_movement_date(c,&mut f).await})).await;denied(result,"INVENTORY_CONSUMPTION_SOURCE_CHANGED");
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT quantity_milli FROM inventory_order_reservations WHERE id='reservation'").fetch_one(pool.raw()).await.unwrap(),4000);
 }).await;
}

// These exercise the actual Inventory effect adapter and deferred constraints.
// Journal/Work rows are explicit synthetic fixture effects, not native owner
// authorization, Finance composition, a command receipt or cutover acceptance.
struct EffectClock;
impl erpx_server_shared::public::Clock for EffectClock {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        erpx_server_shared::public::TimestampMillis::new(1_800_000_000_003)
    }
}
async fn effect_work(pool: &DatabasePool) {
    sqlx::raw_sql("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('asset','company','branch','SYNTHETIC','Synthetic part source','synthetic','THB','maker',0,0);INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at)VALUES('work','company','branch','asset','Synthetic work','normal','in_progress','maker','maker',0,0)").execute(pool.raw()).await.unwrap();
}
async fn apply_component(
    pool: &DatabasePool,
    input: ConsumptionSelection,
    tamper: Option<&'static str>,
) -> Result<(), Error> {
    PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move |c| {
        let input=input.clone();
        Box::pin(async move {
            use erpx_server_shared::public::{CommandKey,TimestampMillis,UserId};
            let mut finance=FinanceFacts;
            // Match the actual writer's strongest intent order before first SELECT.
            sqlx::query("LOCK TABLE finance_journals,finance_journal_lines,money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
            lock_unreserved_consumption_sources(c).await?;
            lock_inventory_consumption_sources(c,&mut finance).await?;
            let prepared=prepare_inventory_consumption(c,&mut finance,input).await?;
            let effects=prepared.retain_unreserved_effects(c,MaintenanceConsumptionIdentity {
                owner_key:CommandKey::new("synthetic-effects")?,work_id:id("work"),work_version_after:2,
                department_id:None,actor_id:UserId::new("maker")?,reason:"Synthetic part issue".into(),
                created_at:TimestampMillis::new(1_800_000_000_001),journal_id:id("parts-journal"),
            }).await?;
            let money=erpx_server_finance::public::inventory_consumption::prepare_consumption_stock_cost_money(c,effects.finance_source(),id(effects.consumption_id()),effects.plan().valuation.value_minor,&EffectClock).await?;
            effects.require_before(c,&mut finance).await?;
            // A fixture DRAFT with the exact source and totals. Production must
            // use the mandatory current-owner Finance bridge instead.
            sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at)VALUES('parts-journal','company','chart','period','SYNTHETIC-PARTS','2026-01-10','THB','maintenance_parts',$1,'Synthetic parts','draft','maker',0,0)").bind(effects.consumption_id()).execute(&mut *c).await?;
            sqlx::query("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,ordinal)VALUES('parts-expense','parts-journal','expense',$1,0,0),('parts-inventory','parts-journal','inventory',0,$1,1)").bind(effects.plan().valuation.value_minor).execute(&mut *c).await?;
            effects.write_consumption_and_lots(c).await?;
            let money=money.persist(c).await?;
            effects.write_stock(c).await?;
            sqlx::raw_sql("UPDATE maintenance_work_orders SET version=2 WHERE id='work';INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,created_at)VALUES('synthetic-event','work',2,'issue-part','maker','Synthetic part issue','in_progress','maker',1800000000000)").execute(&mut *c).await?;
            if let Some(sql)=tamper {sqlx::query(sql).execute(&mut *c).await?;}
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
            money.require_current(c).await?;
            effects.require_inventory_after(c).await
        })
    }).await
}
#[tokio::test]
async fn actual_unreserved_effects_commit_exact_untracked_lot_and_serial_rows_with_deferred_guards()
{
    for (tracking, quantity, allocations, status_after) in [
        ("none", 1000, vec![], "active"),
        ("lot", 1000, vec![lot("lot-1", 1000)], "active"),
        ("lot", 3000, vec![lot("lot-1", 3000)], "depleted"),
        (
            "lot",
            4000,
            vec![lot("lot-1", 3000), lot("lot-2", 1000)],
            "depleted",
        ),
        ("serial", 1000, vec![lot("serial-0", 1000)], "depleted"),
    ] {
        fixture::with_database(2,move |pool|async move {
            setup(&pool,tracking).await;effect_work(&pool).await;
            let mut input=selection(quantity);input.allocations=allocations;
            apply_component(&pool,input,None).await.unwrap();
            let stock:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(stock,(5000-quantity,1000-quantity/5,2));
            if tracking!="none" {
                let status:String=sqlx::query_scalar("SELECT status FROM inventory_lots WHERE id=$1").bind(if tracking=="lot"{"lot-1"}else{"serial-0"}).fetch_one(pool.raw()).await.unwrap();
                assert_eq!(status,status_after);
            }
        }).await;
    }
}
#[tokio::test]
async fn after_images_reject_stock_trigger_rewrite_and_rollback_every_prior_effect() {
    fixture::with_database(2,|pool|async move {
        setup(&pool,"none").await;effect_work(&pool).await;
        sqlx::raw_sql("CREATE FUNCTION synthetic_stock_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN NEW.version:=NEW.version+7;RETURN NEW;END$$;CREATE TRIGGER synthetic_stock_rewrite BEFORE UPDATE ON commerce_stock FOR EACH ROW EXECUTE FUNCTION synthetic_stock_rewrite()").execute(pool.raw()).await.unwrap();
        denied(apply_component(&pool,selection(1000),None).await,"INVENTORY_CONSUMPTION_SOURCE_CHANGED");
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT(SELECT count(*)FROM operational_inventory_consumptions),(SELECT count(*)FROM finance_journals),(SELECT count(*)FROM money_transaction_snapshots)").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0,0));
        let stock:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(pool.raw()).await.unwrap();assert_eq!(stock,(5000,1000,1));
    }).await;
}
#[tokio::test]
async fn final_lot_identity_rewrite_is_not_adopted_as_the_expected_after_image() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, "lot").await;
        effect_work(&pool).await;
        let mut input = selection(1000);
        input.allocations = vec![lot("lot-1", 1000)];
        denied(
            apply_component(
                &pool,
                input,
                Some(
                    "UPDATE inventory_lots SET lot_code='rewritten-after-effect' WHERE id='lot-1'",
                ),
            )
            .await,
            "INVENTORY_CONSUMPTION_SOURCE_CHANGED",
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*)FROM operational_inventory_consumptions")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT lot_code FROM inventory_lots WHERE id='lot-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "L1"
        );
    })
    .await;
}
#[tokio::test]
async fn depletion_retains_other_warehouse_balances_and_keeps_the_shared_lot_active() {
    fixture::with_database(2,|pool|async move {
        setup(&pool,"lot").await;effect_work(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::raw_sql("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('other-warehouse','company','branch','SYNTHETIC-OTHER','Synthetic other warehouse');INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('other-warehouse','product',1000,200);INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES('lot-1','other-warehouse',1000,200)").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        let mut input=selection(3000);input.allocations=vec![lot("lot-1",3000)];
        apply_component(&pool,input,None).await.unwrap();
        let lot:(String,i64)=sqlx::query_as("SELECT status,version FROM inventory_lots WHERE id='lot-1'").fetch_one(pool.raw()).await.unwrap();assert_eq!(lot,("active".into(),1));
        let untouched:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM inventory_lot_balances WHERE lot_id='lot-1'AND warehouse_id='other-warehouse'").fetch_one(pool.raw()).await.unwrap();assert_eq!(untouched,(1000,200,1));
    }).await;
}

#[tokio::test]
async fn service_posting_inventory_history_uses_verified_warehouse_scope_and_detects_drift() {
    use erpx_server_inventory::public::service_parts_posting::*;
    fixture::with_database(2, |pool| async move {
        setup(&pool, "none").await;
        let outcome = PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_service_parts_inventory_history(c).await?;
            let current = read_service_parts_inventory_history(c, &company(), &["warehouse".into()]).await?;
            assert!(current.data().has_history);
            assert!(current.data().stock_json.contains("warehouse"));
            assert_eq!(current.data().ledger_json, "null");
            let foreign = read_service_parts_inventory_history(c, &company(), &[]).await?;
            assert!(!foreign.data().has_history);
            assert_eq!(foreign.data().stock_json, "null");
            sqlx::query("UPDATE commerce_stock SET quantity_milli=quantity_milli+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *c).await?;
            current.require_current(c).await
        })).await;
        denied(outcome, "SERVICE_PARTS_POSTING_SOURCE_CHANGED");
        let quantity: i64 = sqlx::query_scalar("SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(quantity, 5000);
    }).await;
}

#[tokio::test]
async fn service_posting_inventory_locks_before_first_serializable_snapshot() {
    use erpx_server_inventory::public::service_parts_posting::*;
    fixture::with_database(3, |pool| async move {
        setup(&pool, "none").await;
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE commerce_stock SET quantity_milli=5001 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *writer).await.unwrap();
        let read_pool=pool.clone();
        let task=tokio::spawn(async move { PgUnitOfWork::new(read_pool).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_service_parts_inventory_history(c).await?;
            let source=read_service_parts_inventory_history(c,&company(), &["warehouse".into()]).await?;
            source.require_current(c).await?;Ok(source.data().stock_json.clone())
        })).await });
        tokio::time::timeout(std::time::Duration::from_secs(10),async { loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareLock' AND relation='commerce_stock'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();
            if waiting {break} tokio::task::yield_now().await;
        }}).await.unwrap();
        writer.commit().await.unwrap();
        let image=task.await.unwrap().unwrap();assert!(image.contains("\"quantity_milli\": 5001"));
    }).await;
}

#[tokio::test]
async fn service_initial_stock_schema_presence_is_same_unit_and_absence_safe() {
    use erpx_server_inventory::public::service_parts_posting::{
        lock_service_initial_stock_schema, read_service_initial_stock_schema,
    };
    fixture::with_database(2,|pool|async move{
  setup(&pool,"none").await;
  PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_service_initial_stock_schema(c).await?;let source=read_service_initial_stock_schema(c).await?;assert!(source.available());source.require_current(c).await?;Ok(())
  })).await.unwrap();
  let mut tx=pool.raw().begin().await.unwrap();sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE;ALTER TABLE inventory_service_initial_physical_sources RENAME TO fixture_hidden_stock_sources").execute(&mut *tx).await.unwrap();lock_service_initial_stock_schema(&mut tx).await.unwrap();let source=read_service_initial_stock_schema(&mut tx).await.unwrap();assert!(!source.available());source.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
 }).await;
}
