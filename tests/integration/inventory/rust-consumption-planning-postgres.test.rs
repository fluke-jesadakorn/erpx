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

#[tokio::test]
async fn quality_lineage_uses_actual_inventory_posted_count_witness() {
    use erpx_server_inventory::public::quality_disposition_count::{CurrentQualityDispositionCount, QualityDispositionCountRequest, inspect_quality_disposition_count, lock_quality_disposition_count_sources};
    use erpx_server_quality::public::purchase_lineage::{CountLineageRequest, QualityLineageCountPort, QualityLineageRequest, inspect_quality_purchase_lineage, lock_quality_purchase_lineage};
    use erpx_server_runtime::public::postgres::PgWorkFuture;
    struct InventoryCount;
    impl QualityLineageCountPort for InventoryCount {
        type Witness=CurrentQualityDispositionCount;
        fn inspect<'a>(&'a mut self,c:&'a mut sqlx::PgConnection,r:CountLineageRequest)->PgWorkFuture<'a,Option<Self::Witness>,BusinessError> {
            Box::pin(async move { inspect_quality_disposition_count(c,QualityDispositionCountRequest {company_id:r.company_id,branch_id:r.branch_id,warehouse_id:r.warehouse_id,count_id:r.count_id,hold_id:r.hold_id}).await })
        }
        fn require_current<'a>(&'a mut self,c:&'a mut sqlx::PgConnection,w:&'a Self::Witness)->PgWorkFuture<'a,(),BusinessError> {
            Box::pin(async move { w.require_current(c).await })
        }
    }
    fixture::with_database(2,|pool|async move {
        setup(&pool,"none").await;
        sqlx::raw_sql("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id) VALUES('joined-root','company','branch','warehouse','product','Review','2026-11-10','held','maker',0,0,'partial',3000,'goods_receipt','origin'); INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,quality_hold_id,created_by,prepared_by,created_at,updated_at) VALUES('joined-count','company','branch','warehouse','JOINED','2026-11-10','Scrap','posted','joined-root','maker','maker',0,0); UPDATE inventory_quality_holds SET status='disposed',last_result='fail',disposition_kind='scrap',disposition_reason='Scrap',disposition_requested_by='reviewer',disposition_count_id='joined-count',disposition_quantity_milli=1000,disposed_by='approver',disposed_at=1 WHERE id='joined-root'; INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES('joined-event','joined-root','joined-count','post','approver',1,'Scrap',1); INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id) VALUES('joined-root-remaining-joined-count','company','branch','warehouse','product','Remaining','2026-11-10','held','maker',1,1,'partial',2000,'goods_receipt','origin:remaining:joined-count')").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_purchase_lineage(&mut tx).await.unwrap();
        lock_quality_disposition_count_sources(&mut tx).await.unwrap();
        let mut counts=InventoryCount;
        let request=QualityLineageRequest {company_id:company(),branch_id:Some("branch".into()),hold_id:"joined-root-remaining-joined-count".into()};
        let witness=inspect_quality_purchase_lineage(&mut tx,&mut counts,request.clone()).await.unwrap();
        assert_eq!(witness.root()["id"],"joined-root");
        witness.require_current(&mut tx,&mut counts).await.unwrap();
        sqlx::query("UPDATE inventory_counts SET status='draft' WHERE id='joined-count'").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx,&mut counts).await.is_err());
        let current=inspect_quality_purchase_lineage(&mut tx,&mut counts,request).await.unwrap();
        assert_eq!(current.root()["id"],"joined-root-remaining-joined-count");
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn posted_quality_count_witness_retains_scope_reciprocal_hold_and_unit() {
    use erpx_server_inventory::public::quality_disposition_count::{QualityDispositionCountRequest, inspect_quality_disposition_count, lock_quality_disposition_count_sources};
    fixture::with_database(2, |pool| async move {
        setup(&pool,"none").await;
        sqlx::raw_sql("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at) VALUES('quality-hold','company','branch','warehouse','product','Quality','2026-01-10','maker',0,0); INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,quality_hold_id,created_by,prepared_by,created_at,updated_at) VALUES('quality-count','company','branch','warehouse','QC-1','2026-01-10','Quality','posted','quality-hold','maker','maker',0,0),('draft-count','company','branch','warehouse','QC-2','2026-01-10','Quality','draft','quality-hold','maker','maker',0,0)").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_disposition_count_sources(&mut tx).await.unwrap();
        let request=|| QualityDispositionCountRequest {company_id:company(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),count_id:"quality-count".into(),hold_id:"quality-hold".into()};
        let source=inspect_quality_disposition_count(&mut tx,request()).await.unwrap().unwrap();
        source.require_current(&mut tx).await.unwrap();
        for bad in [QualityDispositionCountRequest {count_id:"draft-count".into(),..request()},QualityDispositionCountRequest {hold_id:"other".into(),..request()},QualityDispositionCountRequest {branch_id:"other".into(),..request()},QualityDispositionCountRequest {warehouse_id:"other".into(),..request()},QualityDispositionCountRequest {company_id:erpx_server_shared::public::CompanyId::new("foreign").unwrap(),..request()}] {
            assert!(inspect_quality_disposition_count(&mut tx,bad).await.unwrap().is_none());
        }
        sqlx::query("UPDATE inventory_counts SET status='cancelled' WHERE id='quality-count'").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(source.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}
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

#[tokio::test]
async fn field_journal_consumption_marker_retains_global_routing_predicate() {
    use erpx_server_inventory::public::field_journal_consumption_marker::*;
    use erpx_server_inventory::public::field_history_consumption::*;
    use erpx_server_runtime::public::postgres::PgTransactionError;
    use sqlx::Acquire;
    async fn compare_1097(c:&mut sqlx::PgConnection,source:&CurrentFieldHistoryConsumptionJoin,journal:&str,company:Option<&str>) -> bool {
        source.require_current(&mut *c).await.unwrap();
        let sql=format!("WITH {} SELECT EXISTS(SELECT 1 FROM public.finance_journals j WHERE j.id=$1 AND ($2::text IS NULL OR j.company_id=$2) AND j.source_type='field_service_parts') OR EXISTS(SELECT 1 FROM field_global_journal_consumptions p WHERE p.journal_id=$1 AND ($2::text IS NULL OR p.company_id=$2))",source.global_journal_marker_cte_sql());
        let observed:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(journal).bind(company).fetch_one(&mut *c).await.unwrap();
        let installed:bool=sqlx::query_scalar("SELECT public.erpx_finance_field_journal_owned($1,$2)").bind(journal).bind(company).fetch_one(&mut *c).await.unwrap();
        assert_eq!(observed,installed,"journal={journal} company={company:?}");source.require_current(c).await.unwrap();observed
    }
    fixture::with_database(2, |pool| async move {
        setup(&pool, "none").await;
        let mut conn = pool.raw().acquire().await.unwrap();
        // A TEMP/search_path shadow of the installed Holding relation must not
        // satisfy the marker fence. The real Files caller has no such shadow.
        sqlx::query("CREATE TEMP TABLE operational_inventory_consumptions(journal_id text,company_id text,source_kind text)").execute(&mut *conn).await.unwrap();
        sqlx::query("INSERT INTO pg_temp.operational_inventory_consumptions(journal_id,company_id,source_kind) VALUES('shadow-journal','shadow-company','field_service')").execute(&mut *conn).await.unwrap();
        let mut shadow = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *shadow).await.unwrap();
        lock_field_journal_consumption_marker(&mut shadow).await.unwrap();
        let hidden = inspect_field_journal_consumption_marker(&mut shadow, "shadow-journal").await.unwrap();
        assert!(!hidden.present(), "TEMP/search_path shadow must not satisfy the public marker fence");
        hidden.require_current(&mut shadow).await.unwrap();
        shadow.rollback().await.unwrap();
        // Extract the exact retained Files target query, then execute it against
        // the same transaction with the public source visible. This is the actual
        // TS SQL boundary, not a reproduced predicate.
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);let query='';const db={prepare(sql){return {bind(){return {async first(){query=sql;return {marker:false}}}}}}};await m.fieldJournalFilesTargetInDatabase(db,'shadow-journal');if(!query)throw Error('JOURNAL_TARGET_QUERY_MISSING');process.stdout.write(query.replace(/\?/g,()=>'$1'));"#;
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path=root.join("packages/server-files/src/adapters/field-file-writer-owner.ts");
        let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(&root).arg("--conditions=react-server").arg("-e").arg(script).arg(path).output().unwrap();
        assert!(output.status.success(),"actual TS journal target query unavailable: {}",String::from_utf8_lossy(&output.stderr));
        let ts_sql=String::from_utf8(output.stdout).unwrap();
        let mut ts_tx = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *ts_tx).await.unwrap();
        // pg_temp is explicitly last so the installed TS function resolves the
        // public relation rather than the test-only shadow.
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *ts_tx).await.unwrap();
        let ts_shadow:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(ts_sql.as_str())).bind("shadow-journal").fetch_one(&mut *ts_tx).await.unwrap();
        assert!(!ts_shadow,"actual TS target query must not see the TEMP shadow");
        ts_tx.rollback().await.unwrap();
        sqlx::query("DROP TABLE pg_temp.operational_inventory_consumptions").execute(&mut *conn).await.unwrap();
        let mut unlocked = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(inspect_field_journal_consumption_marker(&mut unlocked, "journal").await.is_err());
        unlocked.rollback().await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        lock_field_journal_consumption_marker(&mut tx).await.unwrap();
        lock_field_history_consumption_join_sources(&mut tx).await.unwrap();
        let global=inspect_field_history_consumption_join(&mut tx).await.unwrap();
        assert!(!compare_1097(&mut tx,&global,"journal",None).await);
        let absent = inspect_field_journal_consumption_marker(&mut tx, "journal").await.unwrap();
        assert!(!absent.present());
        assert_eq!(absent.present(),sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(ts_sql.as_str())).bind("journal").fetch_one(&mut *tx).await.unwrap(),"actual TS absent marker");
        absent.require_current(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES('journal','company','chart','period','J-MARKER','2026-01-10','THB','maintenance_parts','source','marker','draft','maker',0,0)").execute(&mut *tx).await.unwrap();
        // The deferred source guard is intentionally not committed: these rows
        // exercise the routing predicate, not valid operational lineage.
        sqlx::query("INSERT INTO operational_inventory_consumptions(id,company_id,branch_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES('marker-maintenance','company','branch','maintenance','source',1,'warehouse','product',1,0,'THB','2026-01-10','journal','test','maker',0)").execute(&mut *tx).await.unwrap();
        absent.require_current(&mut tx).await.unwrap();
        assert!(!compare_1097(&mut tx,&global,"journal",None).await);
        sqlx::query("INSERT INTO operational_inventory_consumptions(id,company_id,branch_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES('marker-foreign','foreign-company','branch','field_service','source',1,'warehouse','product',1,0,'THB','2026-01-10','journal','test','maker',0)").execute(&mut *tx).await.unwrap();
        assert!(compare_1097(&mut tx,&global,"journal",None).await);
        assert!(compare_1097(&mut tx,&global,"journal",Some("foreign-company")).await);
        assert!(!compare_1097(&mut tx,&global,"journal",Some("company")).await);
        assert!(!compare_1097(&mut tx,&global,"other",None).await);
        // Source identity alone does not attest data or classification: the
        // complete joined result above must be recomputed after late effects.
        global.require_current(&mut tx).await.unwrap();
        assert!(absent.require_current(&mut tx).await.is_err());
        let present = inspect_field_journal_consumption_marker(&mut tx, "journal").await.unwrap();
        assert_eq!(present.journal_id(), "journal");
        assert!(present.present());
        assert_eq!(present.present(),sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(ts_sql.as_str())).bind("journal").fetch_one(&mut *tx).await.unwrap(),"actual TS present marker");
        present.require_current(&mut tx).await.unwrap();
        let other=inspect_field_journal_consumption_marker(&mut tx, "other").await.unwrap();
        assert!(!other.present());
        assert_eq!(other.present(),sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(ts_sql.as_str())).bind("other").fetch_one(&mut *tx).await.unwrap(),"actual TS missing marker");
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_field_journal_consumption_marker(&mut other).await.unwrap();
        assert!(present.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        let mut saved = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();
        sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();
        sqlx::query("SAVEPOINT before_marker_lock").execute(&mut *saved).await.unwrap();
        lock_field_journal_consumption_marker(&mut saved).await.unwrap();
        let retained = inspect_field_journal_consumption_marker(&mut saved, "journal").await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_marker_lock").execute(&mut *saved).await.unwrap();
        assert!(retained.require_current(&mut saved).await.is_err());
        saved.rollback().await.unwrap();
        drop(conn);
        let mut writer = pool.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE public.operational_inventory_consumptions IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
        let mut blocked = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();
        let conflict = lock_field_journal_consumption_marker(&mut blocked).await;
        assert!(matches!(conflict,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        blocked.rollback().await.unwrap();
        writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn field_history_consumption_join_preserves_sql_candidates_without_temp() {
    use erpx_server_inventory::public::field_history_consumption::*;
    use erpx_server_runtime::public::postgres::PgTransactionError;
    use sqlx::Acquire;
    fixture::with_database(2,|pool|async move {
        setup(&pool,"none").await;
        let mut conn=pool.raw().acquire().await.unwrap();sqlx::query("CREATE TEMP TABLE operational_inventory_consumptions(id text)").execute(&mut *conn).await.unwrap();
        let mut unlocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();assert!(inspect_field_history_consumption_join(&mut unlocked).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_history_consumption_join_sources(&mut tx).await.unwrap();let source=inspect_field_history_consumption_join(&mut tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        let before:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();
        let sql=format!("WITH {} SELECT count(*) FROM field_history_consumptions WHERE company_id=$1 AND journal_id=$2",source.cte_sql());
        assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(sql.as_str())).bind("company").bind("journal-history").fetch_one(&mut *tx).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap(),before);
        // Staged only: deferred Inventory source guards stay enabled and this
        // transaction rolls back; no committed consumption lineage is claimed.
        sqlx::query("INSERT INTO public.finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES('journal-history','company','chart','period','J-HISTORY','2026-01-10','THB','field_service_parts','source','fixture','draft','maker',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO public.operational_inventory_consumptions(id,company_id,branch_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES('history-positive','company','branch','field_service','source',1,'warehouse','product',1000,100,'THB','2026-01-10','journal-history','fixture','maker',0),('history-zero','company','branch','field_service','source',1,'warehouse','product',1000,0,'THB','2026-01-10','journal-history','fixture','maker',0),('history-maintenance','company','branch','maintenance','source',1,'warehouse','product',1000,100,'THB','2026-01-10','journal-history','fixture','maker',0),('history-foreign','foreign','branch','field_service','source',1,'warehouse','product',1000,100,'THB','2026-01-10','journal-history','fixture','maker',0),('history-large','company','branch','field_service','source',1,'warehouse','product',9007199254740992,100,'THB','2026-01-10','journal-history','fixture','maker',0)").execute(&mut *tx).await.unwrap();
        let observed:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind("company").bind("journal-history").fetch_one(&mut *tx).await.unwrap();
        let direct:i64=sqlx::query_scalar("SELECT count(*) FROM public.operational_inventory_consumptions WHERE source_kind='field_service' AND quantity_milli>0 AND value_minor>0 AND company_id=$1 AND journal_id=$2").bind("company").bind("journal-history").fetch_one(&mut *tx).await.unwrap();assert_eq!((observed,direct),(2,2));
        assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(sql.as_str())).bind("foreign").bind("journal-history").fetch_one(&mut *tx).await.unwrap(),1);
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT inventory_join_insert").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.operational_inventory_consumptions(id,company_id,branch_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) SELECT 'history-bulk-'||n,'company','branch','field_service','source',1,'warehouse','product',1,1,'THB','2026-01-10','journal-history','fixture','maker',0 FROM generate_series(1,257) n").execute(&mut *tx).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(sql.as_str())).bind("company").bind("journal-history").fetch_one(&mut *tx).await.unwrap(),259);source.require_current(&mut tx).await.unwrap();
        // The source fence is not a row or classification witness.
        sqlx::query("ROLLBACK TO SAVEPOINT inventory_join_insert").execute(&mut *tx).await.unwrap();assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(sql.as_str())).bind("company").bind("journal-history").fetch_one(&mut *tx).await.unwrap(),observed);
        tx.rollback().await.unwrap();sqlx::query("DROP TABLE pg_temp.operational_inventory_consumptions").execute(&mut *conn).await.unwrap();
        let mut other=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_field_history_consumption_join_sources(&mut other).await.unwrap();assert!(source.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT inventory_lock").execute(&mut *saved).await.unwrap();lock_field_history_consumption_join_sources(&mut saved).await.unwrap();let saved_source=inspect_field_history_consumption_join(&mut saved).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT inventory_lock").execute(&mut *saved).await.unwrap();assert!(saved_source.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE public.operational_inventory_consumptions IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();let mut blocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();let conflict=lock_field_history_consumption_join_sources(&mut blocked).await;assert!(matches!(conflict,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn initial_count_history_join_matches_actual_marker_sql() {
    use erpx_server_inventory::public::initial_count_history_source::*;
    use erpx_server_workflow::public::record_origin::*;
    use sqlx::Acquire;
    fixture::with_database(2, |pool| async move {
        setup(&pool,"none").await;
        let mut conn=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TEMP TABLE inventory_counts(id text); CREATE TEMP TABLE inventory_count_lines(id text); CREATE TEMP TABLE command_receipts(command_id text)").execute(&mut *conn).await.unwrap();
        let mut tx=conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_history_sources(&mut tx).await.unwrap();
        lock_initial_count_history_receipt_sources(&mut tx).await.unwrap();
        let inventory=inspect_initial_count_history_source(&mut tx).await.unwrap();
        let workflow=inspect_initial_count_history_receipt_join(&mut tx).await.unwrap();
        let projected=format!("WITH {}, {} SELECT count(*) FROM initial_count_history_counts c JOIN initial_count_history_receipts r ON r.record_id=c.id WHERE c.journal_id IS NOT NULL AND jsonb_exists(r.result_json::jsonb,$1) AND c.company_id=$2 AND c.journal_id=$3",inventory.cte_sql(),workflow.cte_sql());
        let temp_before:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(projected.as_str())).bind("_serviceInitialPhysicalStockCount").bind("company").bind("initial-marker-journal").fetch_one(&mut *tx).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap(),temp_before);
        // The legacy SQL is unqualified; dropping shadows in this rolled-back
        // transaction lets the exact source run against the same public rows.
        sqlx::raw_sql("DROP TABLE pg_temp.inventory_counts; DROP TABLE pg_temp.inventory_count_lines; DROP TABLE pg_temp.command_receipts").execute(&mut *tx).await.unwrap();
        let count_id=format!("count-{}","x".repeat(240));
        sqlx::query("INSERT INTO public.finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES('initial-marker-journal','company','chart','period','J-INITIAL','2026-01-10','THB','stock_count',$1,'fixture','draft','maker',0,0)").bind(&count_id).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,created_by,prepared_by,journal_id,created_at,updated_at) VALUES($1,'company','branch','warehouse','INIT-1','2026-01-10','fixture','draft','maker','maker','initial-marker-journal',0,0),('no-journal','company','branch','warehouse','INIT-2','2026-01-10','fixture','draft','maker','maker',NULL,0,0)").bind(&count_id).execute(&mut *tx).await.unwrap();
        let names=["_initialPhysicalStockCount","_serviceInitialPhysicalStockCount","_posInitialPhysicalStockCount","_manufacturingInitialPhysicalStockCount"];
        for (n,marker) in names.iter().enumerate(){let result=serde_json::json!({(*marker):null}).to_string();sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,'inventory.count.create','fixture','synthetic',$2,$3,'{\"companyId\":\"foreign\"}',0)").bind(format!("initial-marker-{n}")).bind(&count_id).bind(result).execute(&mut *tx).await.unwrap();}
        sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('initial-no-journal','inventory.count.create','fixture','synthetic','no-journal','{\"_serviceInitialPhysicalStockCount\":null}','{\"companyId\":\"company\"}',0)").execute(&mut *tx).await.unwrap();
        let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters");
        let script=r#"import fs from 'node:fs';if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const vars=['initialPhysicalStockMarkedRows','serviceInitialPhysicalStockMarkedRows','posInitialPhysicalStockMarkedRows','manufacturingInitialPhysicalStockMarkedRows'];const names=['initialPhysicalCountMarkedJournal','serviceInitialPhysicalCountMarkedJournal','posInitialPhysicalCountMarkedJournal','manufacturingInitialPhysicalCountMarkedJournal'];const text=fs.readFileSync(process.argv[1],'utf8');const result=[];for(let i=0;i<names.length;i++){const m=await import(process.argv[i+2]);const line=text.split('\n').find(x=>x.startsWith('const '+names[i]+'='));if(!line)throw Error('MISSING_SOURCE');result.push(new Function(vars[i],line+';return '+names[i])(m[vars[i]]))}process.stdout.write(JSON.stringify(result));"#;
        let output=std::process::Command::new(std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into())).arg("--conditions=react-server").arg("--input-type=module").arg("-e").arg(script).arg(dir.join("finance-record-sources.ts")).arg(dir.join("initial-physical-stock-count-source-sql.ts")).arg(dir.join("service-initial-physical-stock-count-source-sql.ts")).arg(dir.join("pos-initial-physical-stock-count-source-sql.ts")).arg(dir.join("manufacturing-initial-physical-stock-count-source-sql.ts")).output().unwrap();
        assert!(output.status.success(),"actual TS SQL: {}",String::from_utf8_lossy(&output.stderr));let actual:Vec<String>=serde_json::from_slice(&output.stdout).unwrap();
        for (n,marker) in names.iter().enumerate(){let legacy=format!("SELECT count(*) FROM ({}) marker WHERE marker.company_id=$1 AND marker.id=$2",actual[n]);let expected:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(legacy.as_str())).bind("company").bind("initial-marker-journal").fetch_one(&mut *tx).await.unwrap();let got:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind(marker).bind("company").bind("initial-marker-journal").fetch_one(&mut *tx).await.unwrap();assert_eq!((got,expected),(1,1),"{marker}");assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(projected.as_str())).bind(marker).bind("foreign").bind("initial-marker-journal").fetch_one(&mut *tx).await.unwrap(),0);}
        inventory.require_current(&mut tx).await.unwrap();workflow.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT late_marker").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('initial-extra','inventory.count.create','fixture','synthetic',$1,'{\"_serviceInitialPhysicalStockCount\":null}','{\"companyId\":\"other\"}',0)").bind(&count_id).execute(&mut *tx).await.unwrap();
        let legacy=format!("SELECT count(*) FROM ({}) marker WHERE marker.company_id=$1 AND marker.id=$2",actual[1]);
        assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(legacy.as_str())).bind("company").bind("initial-marker-journal").fetch_one(&mut *tx).await.unwrap(),2);
        assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(projected.as_str())).bind(names[1]).bind("company").bind("initial-marker-journal").fetch_one(&mut *tx).await.unwrap(),2);
        inventory.require_current(&mut tx).await.unwrap();workflow.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT late_marker").execute(&mut *tx).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(projected.as_str())).bind(names[1]).bind("company").bind("initial-marker-journal").fetch_one(&mut *tx).await.unwrap(),1);
        // Synthetic rows never commit; deferred guards remain enabled.
        tx.rollback().await.unwrap();sqlx::raw_sql("DROP TABLE pg_temp.inventory_counts; DROP TABLE pg_temp.inventory_count_lines; DROP TABLE pg_temp.command_receipts").execute(&mut *conn).await.unwrap();
        let mut other=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_initial_count_history_sources(&mut other).await.unwrap();assert!(inventory.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn initial_count_history_inventory_predicates_match_actual_service_sql() {
    use erpx_server_inventory::public::initial_count_history_source::*;
    fixture::with_database(2,|pool|async move {
        setup(&pool,"none").await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_history_sources(&mut tx).await.unwrap();
        let source=inspect_initial_count_history_source(&mut tx).await.unwrap();
        let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters");
        // Extract the actual Inventory clauses, including SQL SUM and NULL
        // semantics, without reproducing their text in this oracle.
        let script=r#"import fs from 'node:fs';if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const a=fs.readFileSync(process.argv[1],'utf8');const b=fs.readFileSync(process.argv[2],'utf8');const start=a.indexOf(' AND line.snapshot_quantity_milli=0');const end=a.indexOf('`;',start);const sum=b.match(/c\.net_value_minor>0 AND c\.net_value_minor=\(SELECT sum\(line\.counted_value_minor-line\.snapshot_value_minor\) FROM inventory_count_lines line WHERE line\.count_id=c\.id\)/);if(start<0||end<0||!sum)throw Error('SOURCE_CLAUSE_MISSING');process.stdout.write(JSON.stringify([a.slice(start,end),sum[0]]));"#;
        let output=std::process::Command::new(std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into())).arg("--input-type=module").arg("-e").arg(script).arg(dir.join("service-initial-physical-stock-count-source-sql.ts")).arg(dir.join("finance-record-sources.ts")).output().unwrap();
        assert!(output.status.success(),"TS predicate extraction: {}",String::from_utf8_lossy(&output.stderr));let actual:Vec<String>=serde_json::from_slice(&output.stdout).unwrap();
        let originals=format!("SELECT EXISTS(SELECT 1 FROM public.inventory_counts c JOIN public.inventory_count_lines line ON line.count_id=c.id WHERE c.id=$1 {}), EXISTS(SELECT 1 FROM public.inventory_counts c WHERE c.id=$1 AND {})",actual[0],actual[1]);
        let projected=format!("WITH {} SELECT EXISTS(SELECT 1 FROM initial_count_history_counts c JOIN initial_count_history_lines line ON line.count_id=c.id WHERE c.id=$1 {}), EXISTS(SELECT 1 FROM initial_count_history_counts c WHERE c.id=$1 AND {})",source.cte_sql(),actual[0].replace("inventory_count_lines actual","initial_count_history_lines actual"),actual[1].replace("inventory_count_lines line","initial_count_history_lines line"));
        let long_id=format!("count-{}","x".repeat(240));
        sqlx::query("INSERT INTO public.inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,net_value_minor,created_by,prepared_by,created_at,updated_at) VALUES($1,'company','branch','warehouse','PREDICATE','2026-01-10','fixture','posted',200,'maker','maker',0,0)").bind(&long_id).execute(&mut *tx).await.unwrap();
        // A count without lines yields NULL SUM, not zero; all synthetic rows
        // are rolled back with deferred constraints still enabled.
        async fn compare(c:&mut sqlx::PgConnection,id:&str,original:&str,candidate:&str)->(bool,bool){let a=sqlx::query_as(sqlx::AssertSqlSafe(original)).bind(id).fetch_one(&mut *c).await.unwrap();let b=sqlx::query_as(sqlx::AssertSqlSafe(candidate)).bind(id).fetch_one(&mut *c).await.unwrap();assert_eq!(a,b);a}
        assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(false,false));
        sqlx::query("INSERT INTO public.inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES('predicate-line',$1,'product',1,'fixture','each',0,0,0,1000,200,1)").bind(&long_id).execute(&mut *tx).await.unwrap();
        assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(true,true));source.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT nonzero_snapshot").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.inventory_count_lines SET snapshot_quantity_milli=1 WHERE id='predicate-line'").execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(false,true));sqlx::query("ROLLBACK TO SAVEPOINT nonzero_snapshot").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT mismatched_sum").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.inventory_count_lines SET counted_value_minor=199 WHERE id='predicate-line'").execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(true,false));sqlx::query("ROLLBACK TO SAVEPOINT mismatched_sum").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT zero_net").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.inventory_counts SET net_value_minor=0 WHERE id=$1").bind(&long_id).execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(true,false));sqlx::query("ROLLBACK TO SAVEPOINT zero_net").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT nonzero_version").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.inventory_count_lines SET snapshot_version=1 WHERE id='predicate-line'").execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(false,true));sqlx::query("ROLLBACK TO SAVEPOINT nonzero_version").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT held_count").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at) VALUES('predicate-hold','company','branch','warehouse','product','fixture','2026-01-10','maker',0,0)").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.inventory_counts SET quality_hold_id='predicate-hold' WHERE id=$1").bind(&long_id).execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(false,true));sqlx::query("ROLLBACK TO SAVEPOINT held_count").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT extra_line").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('predicate-product','company','PREDICATE-P','fixture',100,'none')").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES('predicate-extra',$1,'predicate-product',1,'fixture','each',0,0,0,1000,0,2)").bind(&long_id).execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(false,true));sqlx::query("ROLLBACK TO SAVEPOINT extra_line").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&originals,&projected).await,(true,true));tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn count_terminal_decision_source_matches_actual_inventory_ts_branch() {
    use erpx_server_inventory::public::{count_terminal_history_source::*,initial_count_history_source::*};
    use erpx_server_workflow::public::record_origin::*;
    fixture::with_database(2,|pool|async move {
        setup(&pool,"none").await;
        let mut unlocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();assert!(inspect_count_terminal_history_source(&mut unlocked).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_history_sources(&mut tx).await.unwrap();lock_count_terminal_history_source(&mut tx).await.unwrap();lock_initial_count_history_receipt_sources(&mut tx).await.unwrap();
        let counts=inspect_initial_count_history_source(&mut tx).await.unwrap();let decisions=inspect_count_terminal_history_source(&mut tx).await.unwrap();let workflow=inspect_initial_count_history_receipt_join(&mut tx).await.unwrap();
        for (n,status,action,actor,version) in [(1,"posted","post","user",2),(2,"cancelled","cancel","user",2),(3,"rejected","reject","user",2),(4,"posted","post","wrong",2),(5,"posted","post","user",3),(6,"posted","post","user",2),(7,"posted","post","user",2),(8,"posted","post","user",2),(9,"posted","cancel","user",2)] {
            let id=format!("terminal-count-{n}-{}","x".repeat(210));let cmd=format!("terminal-command-{n}");
            sqlx::query("INSERT INTO public.inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,created_by,prepared_by,created_at,updated_at) VALUES($1,'company','branch','warehouse',$2,'2026-01-10','fixture',$3,$4,'maker','maker',0,0)").bind(&id).bind(format!("TERM-{n}")).bind(status).bind(version as i64).execute(&mut *tx).await.unwrap();
            let result=serde_json::json!({"status":status,"version":if n==8 {3} else {2}}).to_string();let authority=serde_json::json!({"companyId":if n==6 {"foreign"}else{"company"}}).to_string();let record=if n==7 {"other"} else {id.as_str()};
            sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,$2,$3,'fixture',$4,$5,$6,0)").bind(&cmd).bind(format!("inventory.count.{action}")).bind(actor).bind(record).bind(result).bind(authority).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO public.inventory_count_decisions(id,count_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,'user',1,'fixture',0)").bind(format!("terminal-decision-{n}")).bind(&id).bind(&cmd).bind(action).execute(&mut *tx).await.unwrap();
        }
        let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/supply-terminal-receipts.ts");let script=r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const m=await import(process.argv[1]);const s=m.supplyInventoryTerminalReceipts;const start=s.indexOf(" SELECT 'count'::text"),end=s.indexOf(" UNION ALL SELECT 'landed-cost'",start);if(start<0||end<0)throw Error('COUNT_BRANCH_MISSING');process.stdout.write(JSON.stringify(s.slice(start,end)));"#;
        let output=std::process::Command::new(std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into())).arg("--conditions=react-server").arg("--input-type=module").arg("-e").arg(script).arg(dir).output().unwrap();assert!(output.status.success(),"actual TS count branch: {}",String::from_utf8_lossy(&output.stderr));let actual:String=serde_json::from_slice(&output.stdout).unwrap();
        let replaced=actual.replace("FROM inventory_counts count","FROM initial_count_history_counts count").replace("JOIN inventory_count_decisions event","JOIN count_history_decisions event").replace("JOIN command_receipts command","JOIN initial_count_history_support_receipts command");assert_ne!(actual,replaced);
        let raw=format!("SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.id),'[]'::jsonb)::text FROM ({actual}) t WHERE t.company_id=$1");let projected=format!("WITH {}, {}, {} SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.id),'[]'::jsonb)::text FROM ({replaced}) t WHERE t.company_id=$1",counts.cte_sql(),decisions.cte_sql(),workflow.cte_sql());
        async fn compare(c:&mut sqlx::PgConnection,a:&str,b:&str,company:&str)->serde_json::Value{let original:String=sqlx::query_scalar(sqlx::AssertSqlSafe(a)).bind(company).fetch_one(&mut *c).await.unwrap();let candidate:String=sqlx::query_scalar(sqlx::AssertSqlSafe(b)).bind(company).fetch_one(&mut *c).await.unwrap();let original:serde_json::Value=serde_json::from_str(&original).unwrap();let candidate:serde_json::Value=serde_json::from_str(&candidate).unwrap();assert_eq!(original,candidate);candidate}
        // This is the actual Count terminal branch before Files recursive steps;
        // the later version case remains a candidate, not an accepted terminal.
        assert_eq!(compare(&mut tx,&raw,&projected,"company").await.as_array().unwrap().len(),4);assert_eq!(compare(&mut tx,&raw,&projected,"foreign").await.as_array().unwrap().len(),0);
        counts.require_current(&mut tx).await.unwrap();decisions.require_current(&mut tx).await.unwrap();workflow.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT bad_terminal_result").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.command_receipts SET result_json='{}' WHERE command_id='terminal-command-1'").execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&raw,&projected,"company").await.as_array().unwrap().len(),3);decisions.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT bad_terminal_result").execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&raw,&projected,"company").await.as_array().unwrap().len(),4);
        // Synthetic rows stay uncommitted; no guard is disabled.
        tx.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT terminal_lock").execute(&mut *saved).await.unwrap();lock_count_terminal_history_source(&mut saved).await.unwrap();let retained=inspect_count_terminal_history_source(&mut saved).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT terminal_lock").execute(&mut *saved).await.unwrap();assert!(retained.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
    }).await;
}
