//! Actual service join over native-seeded Inventory/Quality owner rows and the
//! original Workflow receipt. This source does not grant IAM or purchases.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_inventory::public::counts::{
    lock_count_sources, lock_count_transition_guard_sources,
};
use erpx_server_quality::public::quality_commands::lock_quality_count_disposition_sources;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{CompanyId, RecordId};
use erpx_server_workflow::public::quality_return_receipts::lock_quality_scrap_request_receipts;
use erpx_service::quality_count_origin::inspect_quality_count_origin;
use sqlx::PgConnection;

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('company','company','company','Company','company');
        INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES('branch','branch','branch','Branch','company','company','branch'),('other-branch','branch','other-branch','Other','company','company','other-branch');
        INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('warehouse','company','branch','W1','Warehouse')").execute(pool.raw()).await.unwrap();
}
async fn seed(pool: &DatabasePool, key: &str, variant: &str) {
    let product = format!("product-{key}");
    let hold = format!("hold-{key}");
    let count = format!("quality-scrap-{key}");
    sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES($1,'company',$1,'Product',100,'none')").bind(&product).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES('warehouse',$1,10000,5000)").bind(&product).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES($1,'company','branch','warehouse',$2,'2026-11-10','scrap','draft',1,-2000,'maker','maker','reviewer',0,0)").bind(&count).bind(format!("QC-{key}")).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,held_quantity_milli,quantity_scope) VALUES($1,'company','branch','warehouse',$2,'Failed','2026-11-10','held','maker','fail',1,0,0,10000,'partial')").bind(&hold).bind(&product).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='maker',disposition_count_id=$1,disposition_quantity_milli=4000 WHERE id=$2").bind(&count).bind(&hold).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE inventory_counts SET quality_hold_id=$1 WHERE id=$2")
        .bind(&hold)
        .bind(&count)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES($1,$2,$3,1,'Product','pcs',10000,5000,1,6000,3000,0)").bind(format!("line-{key}")).bind(&count).bind(&product).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,'request','maker',1,'Failed',0)").bind(format!("quality-disposition-{key}")).bind(&hold).bind(&count).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE inventory_counts SET status='approved' WHERE id=$1")
        .bind(&count)
        .execute(pool.raw())
        .await
        .unwrap();
    if variant != "missing" {
        let command = if variant == "command" {
            format!("forged-{key}")
        } else {
            key.into()
        };
        let actor = if variant == "actor" { "other" } else { "maker" };
        let record = if variant == "hold" {
            "forged-hold"
        } else {
            &hold
        };
        let company = if variant == "company" {
            "other"
        } else {
            "company"
        };
        let result_count = if variant == "count" {
            "forged-count"
        } else {
            &count
        };
        let authority = serde_json::json!({"companyId":company}).to_string();
        let result = serde_json::json!({"countId":result_count}).to_string();
        let operation = if variant == "operation" {
            "other.operation"
        } else {
            "quality.hold.disposition.scrap"
        };
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,$2,$3,'hash',$4,$5,$6,1)")
            .bind(command).bind(operation).bind(actor).bind(record).bind(result).bind(authority).execute(pool.raw()).await.unwrap();
    }
}
async fn lock_all(c: &mut PgConnection) {
    lock_count_sources(c).await.unwrap();
    lock_count_transition_guard_sources(c).await.unwrap();
    lock_quality_count_disposition_sources(c).await.unwrap();
    lock_quality_scrap_request_receipts(c).await.unwrap();
}
#[tokio::test]
async fn actual_quality_origin_receipt_join_retains_scoped_owner_evidence() {
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        for (key,variant) in [("good","good"),("missing","missing"),("company","company"),("actor","actor"),("hold","hold"),("count","count"),("command","command"),("operation","operation")] {seed(&pool,key,variant).await;}
        let company=CompanyId::new("company").unwrap();let branch=RecordId::new("branch").unwrap();let other_branch=RecordId::new("other-branch").unwrap();let count=RecordId::new("quality-scrap-good").unwrap();
        let mut unlocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(inspect_quality_count_origin(&mut unlocked,&company,Some(&branch),&count).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_all(&mut tx).await;
        for key in ["missing","company","actor","hold","count","command","operation"] {
            let id=RecordId::new(format!("quality-scrap-{key}")).unwrap();assert!(inspect_quality_count_origin(&mut tx,&company,Some(&branch),&id).await.unwrap().is_none(),"{key}");
        }
        assert!(inspect_quality_count_origin(&mut tx,&company,Some(&other_branch),&count).await.unwrap().is_none());
        let other_company=CompanyId::new("other-company").unwrap();assert!(inspect_quality_count_origin(&mut tx,&other_company,Some(&branch),&count).await.unwrap().is_none());
        let origin=inspect_quality_count_origin(&mut tx,&company,Some(&branch),&count).await.unwrap().expect("actual qualified owner and receipt");
        assert_eq!((origin.inventory().hold_id(),origin.inventory().source_command_id(),origin.inventory().creator()),("hold-good","good","maker"));
        origin.require_current(&mut tx).await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(origin.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        sqlx::query("SAVEPOINT origin_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE command_receipts SET request_hash='drift' WHERE command_id='good'").execute(&mut *tx).await.unwrap();assert!(origin.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT origin_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_count_lines SET description='drift' WHERE id='line-good'").execute(&mut *tx).await.unwrap();assert!(origin.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT origin_drift").execute(&mut *tx).await.unwrap();origin.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE inventory_counts SET status='posted',version=version+1 WHERE id='quality-scrap-good'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_quality_holds SET status='disposed',disposed_by='reviewer',disposed_at=100,version=version+1 WHERE id='hold-good'").execute(&mut *tx).await.unwrap();origin.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT origin_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_counts SET status='rejected',review_reason='reject',version=version+1 WHERE id='quality-scrap-good'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_quality_holds SET status='held',disposition_kind=NULL,disposition_reason=NULL,disposition_requested_by=NULL,disposition_count_id=NULL,disposition_quantity_milli=NULL,version=version+1 WHERE id='hold-good'").execute(&mut *tx).await.unwrap();origin.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        // All fences are held in this new unit: rejection therefore proves
        // transaction identity, rather than just a missing lock.
        let mut fully_fenced=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fully_fenced).await.unwrap();lock_all(&mut fully_fenced).await;
        assert!(origin.require_current(&mut fully_fenced).await.is_err());fully_fenced.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_source_locks").execute(&mut *saved).await.unwrap();lock_all(&mut saved).await;
        let retained=inspect_quality_count_origin(&mut saved,&company,Some(&branch),&count).await.unwrap().unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_source_locks").execute(&mut *saved).await.unwrap();assert!(retained.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
    }).await;
}

pub(crate) async fn seed_customer_return(pool: &DatabasePool) {
    sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('chart','company','Chart','THB','maker',0,0);
        INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('product','company','P1','Lot',1000,'lot');
        INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES('warehouse','product',1000,1000);
        INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES('lot-existing','company','product','lot','LOT','maker',0);
        INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES('lot-existing','warehouse',1000,1000)").execute(pool.raw()).await.unwrap();
    sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('return-customer','company','customer','C1','Customer','THB',0,0); INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES('return-debit','chart','company','100','Debit','asset',0,0),('return-credit','chart','company','400','Credit','revenue',0,0); INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,created_by,created_at,updated_at) VALUES('return-invoice','company','return-customer','receivable','INV-RETURN','2026-11-01','2026-11-02','THB',2000,0,2000,2000,'return-debit','return-credit','maker',0,0); INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('plain-product','company','P2','Plain',1000,'none'); INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES('warehouse','plain-product',1000,1000,1); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) VALUES('return-order','company','branch','sales','SO-RET','return-customer','warehouse','THB','2026-11-01','2026-11-10',2000,0,'confirmed',3,'maker',0,0); INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES('plain-order-line','return-order','plain-product','Plain',1000,1000,1000,0,1000,0,0,'stock'),('lot-order-line','return-order','product','Lot',1000,1000,1000,0,1000,0,1,'stock'); INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version,invoice_id) VALUES('return-fulfillment','return-order','FUL-RET','2026-11-01','maker',0,1,'return-invoice'); INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES('plain-fulfilled','return-fulfillment','plain-order-line',1000,1000,1000,0),('lot-fulfilled','return-fulfillment','lot-order-line',1000,1000,1000,0); INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition) VALUES('sold-allocation','lot-fulfilled','lot-existing',1000,1000,'available'); INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,status,subtotal_minor,tax_minor,total_minor,version,requested_by,approved_by,created_at,updated_at) VALUES('return-1','company','branch','return-order','return-fulfillment','return-invoice','customer_return','RET-1','2026-11-01','Return','approved',2000,0,2000,2,'maker','reviewer',0,0); INSERT INTO commerce_return_lines(id,return_id,fulfillment_line_id,product_id,quantity_milli,value_minor,subtotal_minor,tax_minor,ordinal) VALUES('plain-return-line','return-1','plain-fulfilled','plain-product',1000,1000,1000,0,0),('lot-return-line','return-1','lot-fulfilled','product',1000,1000,1000,0,1); INSERT INTO commerce_return_lot_allocations(id,return_line_id,fulfillment_allocation_id,lot_id,quantity_milli,value_minor,snapshot_quantity_milli,snapshot_value_minor,snapshot_version) VALUES('return-allocation','lot-return-line','sold-allocation','lot-existing',1000,1000,1000,1000,1); INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('approval-key','commerce.return.approve','reviewer','legacy-nonhex','return-1','{}',1,'{\"companyId\":\"company\"}'); INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id,lot_id) VALUES('quality-return-approval-key-0','company','branch','warehouse','plain-product','Returned','2026-11-10','held','reviewer',1,1,'partial',1000,'customer_return','plain-return-line',NULL),('quality-return-approval-key-1-0','company','branch','warehouse','product','Returned','2026-11-10','held','reviewer',1,1,'partial',1000,'customer_return','return-allocation','lot-existing')").execute(pool.raw()).await.unwrap();
    sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,quality_hold_id,created_by,prepared_by,created_at,updated_at) VALUES('ancestor','company','branch','warehouse','ANCESTOR','2026-11-09','Scrap','posted','quality-return-approval-key-1-0','reviewer','reviewer',0,0);
       UPDATE inventory_quality_holds SET status='disposed',last_result='fail',disposition_kind='scrap',disposition_reason='Scrap',disposition_requested_by='reviewer',disposition_count_id='ancestor',disposition_quantity_milli=200,disposed_by='reviewer',disposed_at=1 WHERE id='quality-return-approval-key-1-0';
       INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES('ancestor-post','quality-return-approval-key-1-0','ancestor','post','reviewer',1,'Scrap',1);
       INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id,lot_id) VALUES('quality-return-approval-key-1-0-remaining-ancestor','company','branch','warehouse','product','Remaining','2026-11-10','held','reviewer','fail',1,1,'partial',800,'customer_return','return-allocation:remaining:ancestor','lot-existing')").execute(pool.raw()).await.unwrap();
}
pub(crate) async fn seed_return_count(
    pool: &DatabasePool,
    key: &str,
    hold: &str,
    product: &str,
    lot: Option<&str>,
) {
    let count = format!("quality-scrap-{key}");
    sqlx::query("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES($1,'company','branch','warehouse',$2,'2026-11-10','scrap','draft',1,-500,'maker','maker','reviewer',0,0)").bind(&count).bind(format!("QC-{key}")).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE inventory_quality_holds SET last_result='fail',status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='maker',disposition_count_id=$1,disposition_quantity_milli=500 WHERE id=$2").bind(&count).bind(hold).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE inventory_counts SET quality_hold_id=$1 WHERE id=$2")
        .bind(hold)
        .bind(&count)
        .execute(pool.raw())
        .await
        .unwrap();
    let line = format!("line-{key}");
    sqlx::query("INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT $1,$2,$3,1,'Product','pcs',quantity_milli,value_minor,version,quantity_milli-500,value_minor-500,0 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id=$3").bind(&line).bind(&count).bind(product).execute(pool.raw()).await.unwrap();
    if let Some(lot) = lot {
        sqlx::query("INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT $1,$2,$3,$3,'lot','LOT',quantity_milli,value_minor,version,quantity_milli-500,value_minor-500 FROM inventory_lot_balances WHERE lot_id=$3 AND warehouse_id='warehouse'").bind(format!("lot-line-{key}")).bind(&line).bind(lot).execute(pool.raw()).await.unwrap();
    }
    sqlx::query("INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,'request','maker',1,'Failed',0)").bind(format!("quality-disposition-{key}")).bind(hold).bind(&count).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE inventory_counts SET status='approved' WHERE id=$1")
        .bind(&count)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,'quality.hold.disposition.scrap','maker','legacy-nonhex',$2,$3,'{\"companyId\":\"company\"}',1)").bind(key).bind(hold).bind(serde_json::json!({"countId":count}).to_string()).execute(pool.raw()).await.unwrap();
}

async fn lock_return_sources(c: &mut PgConnection) {
    erpx_server_commerce::public::quality_return_origin::lock_quality_return_commerce(c)
        .await
        .unwrap();
    erpx_server_inventory::public::quality_fulfillment_allocation::lock_quality_fulfillment_allocations(c).await.unwrap();
    lock_all(c).await;
}
async fn finalized_return_count(
    c: &mut PgConnection,
    count: &str,
    hold: &str,
    product: &str,
    lot: Option<&str>,
    post: bool,
) -> erpx_server_quality::public::quality_commands::RetainedQualityCountDisposition<
    erpx_server_inventory::public::counts::CurrentInventoryQualityCount,
> {
    use erpx_server_inventory::public::counts::{
        apply_quality_count_decision, apply_quality_count_effects, inspect_count_transition_guard,
        plan_count_transition, plan_count_transition_effects, prepare_inventory_quality_count,
    };
    use erpx_server_inventory::public::service_initial_physical_count::{
        CountAction, CountAuthority,
    };
    use erpx_server_quality::public::quality_commands::{
        QualityCountDispositionAction, QualityCountDispositionFacts,
        prepare_quality_count_disposition,
    };
    let action = if post {
        CountAction::Post
    } else {
        CountAction::Reject
    };
    let guard = inspect_count_transition_guard(c, "company", Some("branch"), count, action)
        .await
        .unwrap();
    let authority = CountAuthority {
        same_as_creator: false,
        same_as_preparer: false,
        company_scope: true,
        department_scope: false,
        can_prepare: false,
        can_approve: true,
    };
    let plan = plan_count_transition(
        &guard.record().status,
        guard.record().version,
        1,
        action,
        &authority,
        Some("Decision"),
        0,
    )
    .unwrap();
    let effects = plan_count_transition_effects(&plan, guard.lines()).unwrap();
    let source = std::sync::Arc::new(
        prepare_inventory_quality_count(
            c,
            guard,
            QualityCountDispositionFacts {
                company_id: "company".into(),
                branch_id: "branch".into(),
                warehouse_id: "warehouse".into(),
                product_id: product.into(),
                lot_id: lot.map(str::to_owned),
                hold_id: hold.into(),
                count_id: count.into(),
                command_key: format!("decision-{count}"),
                actor_id: "reviewer".into(),
                reason: "Decision".into(),
                action: if post {
                    QualityCountDispositionAction::Post
                } else {
                    QualityCountDispositionAction::Reject
                },
                now: 100,
            },
            effects,
            None,
        )
        .await
        .unwrap(),
    );
    let staged = prepare_quality_count_disposition(c, source)
        .await
        .unwrap()
        .apply_before_count(c)
        .await
        .unwrap();
    apply_quality_count_effects(c, 1, &staged).await.unwrap();
    let staged = staged.stage_remaining(c).await.unwrap();
    apply_quality_count_decision(c, 1, &staged).await.unwrap();
    staged.finish_after_count(c).await.unwrap()
}

#[tokio::test]
async fn actual_customer_return_origins_and_finalized_count_actions() {
    use erpx_service::quality_count_origin::inspect_quality_count_customer_return;
    fixture::with_database(3,|pool|async move {
        setup(&pool).await; seed_customer_return(&pool).await;
        seed_return_count(&pool,"plain","quality-return-approval-key-0","plain-product",None).await;
        seed_return_count(&pool,"lot","quality-return-approval-key-1-0-remaining-ancestor","product",Some("lot-existing")).await;
        let company=CompanyId::new("company").unwrap();let branch=RecordId::new("branch").unwrap();
        let plain=RecordId::new("quality-scrap-plain").unwrap();let tracked=RecordId::new("quality-scrap-lot").unwrap();
        // The service must not inspect a return without Commerce's complete source fence.
        let mut missing=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *missing).await.unwrap();lock_all(&mut missing).await;
        let direct=inspect_quality_count_origin(&mut missing,&company,Some(&branch),&plain).await.unwrap().unwrap();
        assert!(inspect_quality_count_customer_return(&mut missing,direct,&company,Some(&branch)).await.is_err());missing.rollback().await.unwrap();
        // Tracked origin additionally needs the actual Inventory allocation fence.
        let mut missing_allocation=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *missing_allocation).await.unwrap();
        erpx_server_commerce::public::quality_return_origin::lock_quality_return_commerce(&mut missing_allocation).await.unwrap();lock_all(&mut missing_allocation).await;
        let direct=inspect_quality_count_origin(&mut missing_allocation,&company,Some(&branch),&tracked).await.unwrap().unwrap();
        assert!(inspect_quality_count_customer_return(&mut missing_allocation,direct,&company,Some(&branch)).await.is_err());missing_allocation.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_return_sources(&mut tx).await;
        let direct=inspect_quality_count_origin(&mut tx,&company,Some(&branch),&plain).await.unwrap().unwrap();
        let untracked=inspect_quality_count_customer_return(&mut tx,direct,&company,Some(&branch)).await.unwrap().expect("actual untracked return");
        let direct=inspect_quality_count_origin(&mut tx,&company,Some(&branch),&tracked).await.unwrap().unwrap();
        let tracked_source=inspect_quality_count_customer_return(&mut tx,direct,&company,Some(&branch)).await.unwrap().expect("actual tracked return with ancestor");
        untracked.require_current(&mut tx).await.unwrap();tracked_source.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT return_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE command_receipts SET request_hash='still-nonhex' WHERE command_id='approval-key'").execute(&mut *tx).await.unwrap();
        untracked.require_current(&mut tx).await.unwrap();tracked_source.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT return_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE commerce_orders SET number='forged' WHERE id='return-order'").execute(&mut *tx).await.unwrap();
        assert!(untracked.require_current(&mut tx).await.is_err());assert!(tracked_source.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT return_drift").execute(&mut *tx).await.unwrap();
        assert!(sqlx::query("UPDATE inventory_fulfillment_lot_allocations SET quantity_milli=quantity_milli+1 WHERE id='sold-allocation'").execute(&mut *tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT return_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_quality_holds SET source_id='forged' WHERE id='quality-return-approval-key-0'").execute(&mut *tx).await.unwrap();
        assert!(untracked.require_current(&mut tx).await.is_err());
        let forged_direct=inspect_quality_count_origin(&mut tx,&company,Some(&branch),&plain).await.unwrap().unwrap();
        assert!(inspect_quality_count_customer_return(&mut tx,forged_direct,&company,Some(&branch)).await.unwrap().is_none());
        sqlx::query("ROLLBACK TO SAVEPOINT return_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE command_receipts SET actor_id='forged' WHERE command_id='approval-key'").execute(&mut *tx).await.unwrap();
        assert!(untracked.require_current(&mut tx).await.is_err());
        let forged_direct=inspect_quality_count_origin(&mut tx,&company,Some(&branch),&plain).await.unwrap().unwrap();
        assert!(inspect_quality_count_customer_return(&mut tx,forged_direct,&company,Some(&branch)).await.unwrap().is_none());
        sqlx::query("ROLLBACK TO SAVEPOINT return_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_counts SET status='draft' WHERE id='ancestor'").execute(&mut *tx).await.unwrap();
        assert!(tracked_source.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT return_drift").execute(&mut *tx).await.unwrap();
        let post=finalized_return_count(&mut tx,"quality-scrap-plain","quality-return-approval-key-0","plain-product",None,true).await;
        untracked.require_after_count(&mut tx,&post).await.unwrap();
        let reject=finalized_return_count(&mut tx,"quality-scrap-lot","quality-return-approval-key-1-0-remaining-ancestor","product",Some("lot-existing"),false).await;
        tracked_source.require_after_count(&mut tx,&reject).await.unwrap();
        assert!(tracked_source.require_after_count(&mut tx,&post).await.is_err());
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_return_sources(&mut other).await;
        assert!(untracked.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut commerce_loss=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *commerce_loss).await.unwrap();
        erpx_server_inventory::public::quality_fulfillment_allocation::lock_quality_fulfillment_allocations(&mut commerce_loss).await.unwrap();lock_all(&mut commerce_loss).await;
        sqlx::query("SAVEPOINT commerce_fence").execute(&mut *commerce_loss).await.unwrap();erpx_server_commerce::public::quality_return_origin::lock_quality_return_commerce(&mut commerce_loss).await.unwrap();
        let direct=inspect_quality_count_origin(&mut commerce_loss,&company,Some(&branch),&plain).await.unwrap().unwrap();let proof=inspect_quality_count_customer_return(&mut commerce_loss,direct,&company,Some(&branch)).await.unwrap().unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT commerce_fence").execute(&mut *commerce_loss).await.unwrap();assert!(proof.require_current(&mut commerce_loss).await.is_err());commerce_loss.rollback().await.unwrap();
        let mut allocation_loss=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *allocation_loss).await.unwrap();
        erpx_server_commerce::public::quality_return_origin::lock_quality_return_commerce(&mut allocation_loss).await.unwrap();lock_all(&mut allocation_loss).await;
        sqlx::query("SAVEPOINT allocation_fence").execute(&mut *allocation_loss).await.unwrap();erpx_server_inventory::public::quality_fulfillment_allocation::lock_quality_fulfillment_allocations(&mut allocation_loss).await.unwrap();
        let direct=inspect_quality_count_origin(&mut allocation_loss,&company,Some(&branch),&tracked).await.unwrap().unwrap();let proof=inspect_quality_count_customer_return(&mut allocation_loss,direct,&company,Some(&branch)).await.unwrap().unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT allocation_fence").execute(&mut *allocation_loss).await.unwrap();assert!(proof.require_current(&mut allocation_loss).await.is_err());allocation_loss.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT return_fences").execute(&mut *saved).await.unwrap();lock_return_sources(&mut saved).await;
        let direct=inspect_quality_count_origin(&mut saved,&company,Some(&branch),&tracked).await.unwrap().unwrap();let retained=inspect_quality_count_customer_return(&mut saved,direct,&company,Some(&branch)).await.unwrap().unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT return_fences").execute(&mut *saved).await.unwrap();assert!(retained.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_return_fences_block_concurrent_allocation_phantoms() {
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;seed_customer_return(&pool).await;
        for mutation in [
            "INSERT INTO commerce_return_lot_allocations(id,return_line_id,fulfillment_allocation_id,lot_id,quantity_milli,value_minor,snapshot_quantity_milli,snapshot_value_minor,snapshot_version) VALUES('race-return','lot-return-line','sold-allocation','lot-existing',1,1,1000,1000,1)",
            "INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition) VALUES('race-stock','lot-fulfilled','lot-existing',1,1,'available')",
        ] {
            let mut owner=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *owner).await.unwrap();
            erpx_server_commerce::public::quality_return_origin::lock_quality_return_commerce(&mut owner).await.unwrap();
            erpx_server_inventory::public::quality_fulfillment_allocation::lock_quality_fulfillment_allocations(&mut owner).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *owner).await.unwrap();
            let other_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
            let worker=tokio::spawn(async move {let mut c=other_pool.raw().acquire().await.unwrap();let worker_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(worker_pid).unwrap();let _=sqlx::query(mutation).execute(&mut *c).await;});
            let worker_pid=receive.await.unwrap();let mut blocked=false;
            for _ in 0..200 {blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(pid).bind(worker_pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
            assert!(blocked,"allocation insert must wait on the owner's source fence");owner.rollback().await.unwrap();worker.await.unwrap();
        }
    }).await;
}

#[tokio::test]
async fn posted_quality_count_witness_requires_and_retains_all_owner_fences() {
    use erpx_server_inventory::public::quality_disposition_count::{
        QualityDispositionCountRequest, inspect_quality_disposition_count,
        lock_quality_disposition_count_sources,
    };
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool, "fenced", "good").await;
        sqlx::query("UPDATE inventory_counts SET status='posted' WHERE id='quality-scrap-fenced'")
            .execute(pool.raw())
            .await
            .unwrap();
        let request = || QualityDispositionCountRequest {
            company_id: CompanyId::new("company").unwrap(),
            branch_id: "branch".into(),
            warehouse_id: "warehouse".into(),
            count_id: "quality-scrap-fenced".into(),
            hold_id: "hold-fenced".into(),
        };
        let mut unlocked = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *unlocked)
            .await
            .unwrap();
        assert!(
            inspect_quality_disposition_count(&mut unlocked, request())
                .await
                .is_err()
        );
        unlocked.rollback().await.unwrap();
        let mut saved = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *saved)
            .await
            .unwrap();
        sqlx::query("SAVEPOINT before_fences")
            .execute(&mut *saved)
            .await
            .unwrap();
        lock_quality_disposition_count_sources(&mut saved)
            .await
            .unwrap();
        let witness = inspect_quality_disposition_count(&mut saved, request())
            .await
            .unwrap()
            .unwrap();
        witness.require_current(&mut saved).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_fences")
            .execute(&mut *saved)
            .await
            .unwrap();
        assert!(witness.require_current(&mut saved).await.is_err());
        saved.rollback().await.unwrap();
        // The existing Count writer fence is stronger and must qualify.
        let mut strong = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *strong)
            .await
            .unwrap();
        lock_count_sources(&mut strong).await.unwrap();
        let witness = inspect_quality_disposition_count(&mut strong, request())
            .await
            .unwrap()
            .unwrap();
        witness.require_current(&mut strong).await.unwrap();
        sqlx::query("SAVEPOINT shadow_count")
            .execute(&mut *strong)
            .await
            .unwrap();
        sqlx::query("CREATE TEMP TABLE inventory_count_lines(id text)")
            .execute(&mut *strong)
            .await
            .unwrap();
        assert!(witness.require_current(&mut strong).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT shadow_count")
            .execute(&mut *strong)
            .await
            .unwrap();
        sqlx::query("SAVEPOINT force_rls").execute(&mut *strong).await.unwrap();
        sqlx::raw_sql("ALTER TABLE public.inventory_counts ENABLE ROW LEVEL SECURITY; ALTER TABLE public.inventory_counts FORCE ROW LEVEL SECURITY").execute(&mut *strong).await.unwrap();
        let active:bool=sqlx::query_scalar("SELECT row_security_active('public.inventory_counts'::regclass)").fetch_one(&mut *strong).await.unwrap();assert!(active);
        assert!(witness.require_current(&mut strong).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT force_rls").execute(&mut *strong).await.unwrap();
        strong.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *other)
            .await
            .unwrap();
        lock_count_sources(&mut other).await.unwrap();
        assert!(witness.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn direct_quality_origin_retains_large_unicode_image_exactly() {
    use erpx_server_inventory::public::counts::inspect_direct_quality_count_origin;
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        seed(&pool, "large", "good").await;
        // One field alone exceeds the old synthetic 2 MiB cap; the multibyte
        // characters straddle chunk boundaries and the final byte can drift.
        sqlx::query("UPDATE inventory_count_lines SET description=repeat('x',2097151)||'é🙂tail' WHERE id='line-large'")
            .execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_all(&mut tx).await;
        let origin=inspect_direct_quality_count_origin(&mut tx,"company",Some("branch"),"quality-scrap-large").await.unwrap().expect("large native origin");
        origin.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT image_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_count_lines SET description=left(description,length(description)-1)||'X' WHERE id='line-large'").execute(&mut *tx).await.unwrap();
        assert!(origin.require_current(&mut tx).await.is_err(), "tail drift must fail exact comparison");
        sqlx::query("ROLLBACK TO SAVEPOINT image_drift").execute(&mut *tx).await.unwrap();
        origin.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_all(&mut other).await;
        assert!(origin.require_current(&mut other).await.is_err(), "a fully fenced other unit must fail");
        other.rollback().await.unwrap();
    }).await;
}
