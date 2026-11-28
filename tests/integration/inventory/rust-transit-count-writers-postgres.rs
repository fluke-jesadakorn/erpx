//! Actual native Transit dispatch/receive/cancel and generic Count
//! create/revision effects over leased PostgreSQL. These cases prove owner
//! reads and writes only; they do not prove IAM, purchase admission, Workflow
//! receipts, Finance journals or Files evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_inventory_server::public::counts::{
    CountAllocationInput, CountLineInput, CountSaveInput, apply_count_save, lock_count_sources,
    plan_count_save, read_count_save_source,
};
use erpx_inventory_server::public::transit::{
    TransitCancelInput, TransitDispatchInput, TransitLotAllocationInput, TransitReceiveInput,
    TransitReceiveLineInput, apply_transit_cancel, apply_transit_dispatch, apply_transit_receive,
    lock_transit_sources, plan_cancel, plan_dispatch, plan_receive, read_transit_source,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;


struct ActualLineageCounts;
impl erpx_quality_server::public::purchase_lineage::QualityLineageCountPort for ActualLineageCounts {
    type Witness = erpx_inventory_server::public::quality_disposition_count::CurrentQualityDispositionCount;
    fn inspect<'a>(&'a mut self,c:&'a mut sqlx::PgConnection,r:erpx_quality_server::public::purchase_lineage::CountLineageRequest)->erpx_platform_runtime_server::public::postgres::PgWorkFuture<'a,Option<Self::Witness>,BusinessError> {
        Box::pin(async move { erpx_inventory_server::public::quality_disposition_count::inspect_quality_disposition_count(c,erpx_inventory_server::public::quality_disposition_count::QualityDispositionCountRequest {company_id:r.company_id,branch_id:r.branch_id,warehouse_id:r.warehouse_id,count_id:r.count_id,hold_id:r.hold_id}).await })
    }
    fn require_current<'a>(&'a mut self,c:&'a mut sqlx::PgConnection,witness:&'a Self::Witness)->erpx_platform_runtime_server::public::postgres::PgWorkFuture<'a,(),BusinessError> {
        Box::pin(async move { witness.require_current(c).await })
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    let mut c = pool.raw().begin().await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company','company','company','Company','company');\
         INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES\
           ('branch','branch','branch','Branch','company','company','branch'),\
           ('branch-two','branch','branch-two','Second branch','company','company','branch-two');\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','maker',0,0);\
         INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at)VALUES\
           ('inventory-account','company','chart','100','Inventory','asset','THB',0,0);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES\
           ('warehouse','company','branch','W1','Main warehouse'),\
           ('warehouse-two','company','branch-two','W2','Second warehouse');\
         INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES\
           ('product','company','P1','Tracked product',2500,'lot');\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES\
           ('warehouse','product',10000,5000);\
         INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,manufactured_date,expiry_date,created_by,created_at)VALUES\
           ('lot-a','company','product','lot','L-A',NULL,'2027-01-01','maker',0);\
         INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES\
           ('lot-a','warehouse',10000,5000);\
         INSERT INTO inventory_transfers(id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,source_warehouse_version,target_warehouse_version,number,transfer_date,reason,status,version,total_value_minor,created_by,prepared_by,approved_by,submitted_at,posted_at,created_at,updated_at,delivery_mode,arrival_due_date)VALUES\
           ('transfer-1','company','warehouse','warehouse-two','branch','branch-two',1,1,'TR-1','2026-11-01','Transfer in transit','posted',1,5000,'maker','maker','approver',0,0,0,0,'transit','2026-11-10'),\
           ('transfer-2','company','warehouse','warehouse-two','branch','branch-two',1,1,'TR-2','2026-11-01','Cancelled transport','posted',1,5000,'maker','maker','approver',0,0,0,0,'transit','2026-11-10');\
         INSERT INTO inventory_transfer_lines(id,transfer_id,product_id,product_version,description,unit,quantity_milli,value_minor,source_quantity_milli,source_value_minor,source_version,ordinal)VALUES\
           ('source-line-1','transfer-1','product',1,'Tracked product','pcs',10000,5000,10000,5000,1,1),\
           ('source-line-2','transfer-2','product',1,'Tracked product','pcs',10000,5000,10000,5000,1,1);\
         INSERT INTO inventory_transit(id,transfer_id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,status,due_date,currency,chart_id,inventory_account_id,created_by,approved_by,version,created_at,updated_at)VALUES\
           ('transit-1','transfer-1','company','warehouse','warehouse-two','branch','branch-two','awaiting_dispatch','2026-11-10','THB','chart','inventory-account','maker','approver',1,0,0),\
           ('transit-2','transfer-2','company','warehouse','warehouse-two','branch','branch-two','awaiting_dispatch','2026-11-10','THB','chart','inventory-account','maker','approver',1,0,0);\
         INSERT INTO inventory_transit_lines(id,transit_id,source_line_id,product_id,quantity_milli,value_minor)VALUES\
           ('line-1','transit-1','source-line-1','product',10000,5000),\
           ('line-2','transit-2','source-line-2','product',10000,5000);\
         INSERT INTO inventory_transit_lots(id,transit_line_id,lot_id,quantity_milli,value_minor)VALUES\
           ('tl-a','line-1','lot-a',10000,5000),\
           ('tl-b','line-2','lot-a',10000,5000);",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    c.commit().await.unwrap();
}

fn dispatch(date: &str) -> TransitDispatchInput {
    TransitDispatchInput {
        expected_version: 1,
        expected_transfer_version: None,
        attachment_set_ids: vec![],
        date: date.into(),
        tracking_reference: "TRK-1".into(),
        evidence: "Loaded onto carrier".into(),
    }
}

#[tokio::test]
async fn dispatch_receive_and_cancel_apply_native_stock_and_audit_effects() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_transit_sources(c).await?;
                    let source = read_transit_source(c, "company", Some("branch"), "transit-1").await?;
                    let plan = plan_dispatch(&source, &dispatch("2026-11-02"))?;
                    apply_transit_dispatch(c, "transit-1", &plan, "dispatch-key", "dispatcher", 10).await?;
                    Ok(()) 
                })
            })
            .await
            .unwrap();
        let mut c = pool.raw().begin().await.unwrap();
        let stock: (i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(stock, (0, 0));
        let balance: (i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,value_minor FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(balance, (0, 0));
        let transit: (String, i64) =
            sqlx::query_as("SELECT status,version FROM inventory_transit WHERE id='transit-1'")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(transit, ("in_transit".into(), 2));
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_transit_events WHERE transit_id='transit-1' AND kind='dispatch'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(events, 1);
        let lot_events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_transit_lot_events",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(lot_events, 1);
        c.rollback().await.unwrap();

        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_transit_sources(c).await?;
                    let source = read_transit_source(c, "company", None, "transit-1").await?;
                    let input = TransitReceiveInput {
                        expected_version: 2,
                        expected_transfer_version: None,
                        attachment_set_ids: vec![],
                        date: "2026-11-03".into(),
                        evidence: "Received in full".into(),
                        lines: vec![TransitReceiveLineInput {
                            line_id: "line-1".into(),
                            quantity: "10.000".into(),
                            allocations: vec![TransitLotAllocationInput {
                                transit_lot_id: "tl-a".into(),
                                quantity: "10.000".into(),
                            }],
                        }],
                    };
                    let plan = plan_receive(&source, &input)?;
                    apply_transit_receive(c, "transit-1", &plan, "receive-key", "receiver", 20).await?;
                    Ok(()) 
                })
            })
            .await
            .unwrap();
        let mut c = pool.raw().begin().await.unwrap();
        let target: (i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse-two' AND product_id='product'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(target, (10_000, 5_000));
        let transit: (String, i64) =
            sqlx::query_as("SELECT status,version FROM inventory_transit WHERE id='transit-1'")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(transit, ("completed".into(), 3));
        let line: (i64, i64, i64) = sqlx::query_as(
            "SELECT received_milli,lost_milli,settled_value_minor FROM inventory_transit_lines WHERE id='line-1'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(line, (10_000, 0, 5_000));
        c.rollback().await.unwrap();

        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_transit_sources(c).await?;
                    let source = read_transit_source(c, "company", None, "transit-2").await?;
                    let plan = plan_cancel(
                        &source,
                        &TransitCancelInput {
                            expected_version: 1,
                            reason: "Supplier cancelled pickup".into(),
                        },
                    )?;
                    apply_transit_cancel(c, "transit-2", &plan, "cancel-key", "dispatcher", 30).await?;
                    Ok(()) 
                })
            })
            .await
            .unwrap();
        let mut c = pool.raw().begin().await.unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM inventory_transit WHERE id='transit-2'")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(status, "cancelled");
        let cancel_events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_transit_events WHERE transit_id='transit-2' AND kind='cancel'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(cancel_events, 1);
        c.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn count_save_creates_and_revises_a_draft_with_native_lot_snapshots() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let product_ids = vec!["product".to_string()];
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                let product_ids = product_ids.clone();
                Box::pin(async move {
                    lock_count_sources(c).await?;
                    let source = read_count_save_source(
                        c,
                        "company",
                        Some("branch"),
                        None,
                        "warehouse",
                        &product_ids,
                    )
                    .await?;
                    let input = CountSaveInput {
                        warehouse_id: "warehouse".into(),
                        count_date: "2026-11-10".into(),
                        reason: "Monthly physical count".into(),
                        expected_version: None,
                        lines: vec![CountLineInput {
                            product_id: "product".into(),
                            counted_quantity: "8.000".into(),
                            unit_cost: None,
                            allocations: vec![CountAllocationInput {
                                lot_id: Some("lot-a".into()),
                                lot_code: None,
                                serial_number: None,
                                manufactured_date: Some("".into()),
                                expiry_date: Some("".into()),
                                counted_quantity: "8.000".into(),
                                unit_cost: None,
                            }],
                        }],
                    };
                    let plan = plan_count_save(&source, &input, "count-test", "company")?;
                    apply_count_save(c, &plan, None, "counter", 100).await?;
                    Ok(()) 
                })
            })
            .await
            .unwrap();
        let mut c = pool.raw().begin().await.unwrap();
        let count: (String, i64, i64, String) = sqlx::query_as(
            "SELECT status,version,net_value_minor,number FROM inventory_counts WHERE id='count-test'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(count.0, "draft");
        assert_eq!(count.1, 1);
        assert_eq!(count.2, -1000);
        assert!(count.3.starts_with("COUNT-"));
        let line: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT snapshot_quantity_milli,snapshot_value_minor,counted_quantity_milli,counted_value_minor FROM inventory_count_lines WHERE id='count-test-0'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(line, (10_000, 5_000, 8_000, 4_000));
        let lot: (Option<String>, i64, i64) = sqlx::query_as(
            "SELECT source_lot_id,snapshot_version,counted_value_minor FROM inventory_count_lot_lines WHERE id='count-test-0-lot-0'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(lot, (Some("lot-a".into()), 1, 4_000));
        c.rollback().await.unwrap();

        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                let product_ids = product_ids.clone();
                Box::pin(async move {
                    lock_count_sources(c).await?;
                    let source = read_count_save_source(
                        c,
                        "company",
                        Some("branch"),
                        Some("count-test"),
                        "warehouse",
                        &product_ids,
                    )
                    .await?;
                    assert_eq!(source.current.as_ref().map(|row| row.version), Some(1));
                    let input = CountSaveInput {
                        warehouse_id: "warehouse".into(),
                        count_date: "2026-11-11".into(),
                        reason: "Corrected monthly count".into(),
                        expected_version: Some(1),
                        lines: vec![CountLineInput {
                            product_id: "product".into(),
                            counted_quantity: "9.000".into(),
                            unit_cost: None,
                            allocations: vec![CountAllocationInput {
                                lot_id: Some("lot-a".into()),
                                lot_code: None,
                                serial_number: None,
                                manufactured_date: Some("".into()),
                                expiry_date: Some("".into()),
                                counted_quantity: "9.000".into(),
                                unit_cost: None,
                            }],
                        }],
                    };
                    let plan = plan_count_save(&source, &input, "count-test", "company")?;
                    assert!(plan.revision);
                    apply_count_save(c, &plan, Some(1), "counter", 200).await?;
                    Ok(()) 
                })
            })
            .await
            .unwrap();
        let mut c = pool.raw().begin().await.unwrap();
        let count: (i64, i64, String) = sqlx::query_as(
            "SELECT version,net_value_minor,count_date FROM inventory_counts WHERE id='count-test'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(count, (2, -500, "2026-11-11".into()));
        let lines: i64 =
            sqlx::query_scalar("SELECT count(*) FROM inventory_count_lines WHERE count_id='count-test'")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(lines, 1);
        let counted: i64 = sqlx::query_scalar(
            "SELECT counted_quantity_milli FROM inventory_count_lines WHERE id='count-test-0'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(counted, 9_000);
        c.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn native_quality_count_checks_actual_snapshots_and_scraps_the_lot_not_the_hold_id() {
    use erpx_inventory_server::public::counts::{apply_quality_count_effects, apply_quality_count_decision, inspect_count_transition_guard, inspect_quality_count_decision_projection, lock_count_transition_guard_sources, plan_count_transition, plan_count_transition_effects, prepare_inventory_quality_count};
    use erpx_inventory_server::public::service_initial_physical_count::{CountAction, CountAuthority};
    use erpx_quality_server::public::quality_commands::{lock_quality_count_disposition_sources, prepare_quality_count_disposition, QualityCountDispositionAction, QualityCountDispositionFacts};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES('count-quality','company','branch','warehouse','QC-1','2026-11-10','scrap','draft',1,-5000,'maker','maker','reviewer',0,0);
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope) VALUES('hold-distinct','company','branch','warehouse','product','Failed inspection','2026-11-10','held','maker','fail',1,0,0,'lot-a',10000,'partial');
            UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='manager',disposition_count_id='count-quality',disposition_quantity_milli=10000 WHERE id='hold-distinct';
            UPDATE inventory_counts SET quality_hold_id='hold-distinct' WHERE id='count-quality';
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-quality','count-quality','product',1,'Tracked product','pcs',quantity_milli,value_minor,version,0,0,0 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot','line-quality','lot-a','lot-a','lot','L-A',quantity_milli,value_minor,version,0,0 FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse';
            INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('extra-product','company','PX','Extra',500,'none');
            INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('product-two','company','P2','Second product',500,'lot');
            INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES('warehouse','product-two',2000,1000);
            INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES('lot-b','company','product-two','lot','L-B','maker',0),('lot-c','company','product-two','lot','L-C','maker',0);
            INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES('lot-b','warehouse',2000,1000),('lot-c','warehouse',0,0);
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-two','count-quality','product-two',1,'สินค้า \"สอง\"','pcs',quantity_milli,value_minor,version,1000,500,1 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product-two';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot-b','line-two','lot-b','lot-b','lot','L-B',quantity_milli,value_minor,version,1000,500 FROM inventory_lot_balances WHERE lot_id='lot-b' AND warehouse_id='warehouse';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot-c','line-two','lot-c','lot-c','lot','L-C',quantity_milli,value_minor,version,0,0 FROM inventory_lot_balances WHERE lot_id='lot-c' AND warehouse_id='warehouse';
            INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('product-three','company','P3','Absent stock',500,'none');
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES('line-three','count-quality','product-three',1,'Absent stock','pcs',0,0,0,1000,500,2);
            UPDATE inventory_counts SET status='approved' WHERE id='count-quality'")
            .execute(pool.raw()).await.unwrap();
        // A single legacy-valid field exceeds the old synthetic image budget.
        sqlx::query("UPDATE inventory_count_lines SET description=repeat('x',2100000)||chr(10)||chr(34)||chr(92)||'ไทย🙂' WHERE id='line-quality'")
            .execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE commerce_products SET name=repeat('p',2100000) WHERE id='product'")
            .execute(pool.raw()).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run_once::<_, BusinessError, _>(TransactionOptions::default(), move |c| Box::pin(async move {
            lock_count_sources(c).await?;
            lock_count_transition_guard_sources(c).await?;
            lock_quality_count_disposition_sources(c).await?;
            let mut lineage_counts=ActualLineageCounts;
            let lineage=erpx_quality_server::public::purchase_lineage::inspect_quality_purchase_lineage(c,&mut lineage_counts,erpx_quality_server::public::purchase_lineage::QualityLineageRequest { company_id:erpx_platform_shared_server::public::CompanyId::new("company")?, branch_id:Some("branch".into()),hold_id:"hold-distinct".into() }).await?;
            let guard = inspect_count_transition_guard(c,"company",Some("branch"),"count-quality",CountAction::Post).await?;
            let facts = QualityCountDispositionFacts { company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),lot_id:Some("lot-a".into()),hold_id:"hold-distinct".into(),count_id:"count-quality".into(),command_key:"quality-key".into(),actor_id:"reviewer".into(),reason:"Approved scrap".into(),action:QualityCountDispositionAction::Post,now:100 };
            guard.require_current(c).await?;
            sqlx::query("SAVEPOINT snapshot_drift").execute(&mut *c).await?;
            for mutation in [
                "UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'",
                "UPDATE commerce_products SET version=version+1 WHERE id='product'",
                "UPDATE inventory_lot_balances SET version=version+1 WHERE lot_id='lot-a' AND warehouse_id='warehouse'",
                "INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES('late-drift','count-quality','warehouse','product',0,0,'2026-11-11',101)",
            ] {
                sqlx::query(mutation).execute(&mut *c).await?;
                assert!(guard.require_current(c).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT snapshot_drift").execute(&mut *c).await?;
            }
            sqlx::query("RELEASE SAVEPOINT snapshot_drift").execute(&mut *c).await?;
            let authority = CountAuthority { same_as_creator:false,same_as_preparer:false,company_scope:true,department_scope:false,can_prepare:false,can_approve:true };
            let plan = plan_count_transition(&guard.record().status,guard.record().version,1,CountAction::Post,&authority,Some("Approved scrap"),0)?;
            let effects = plan_count_transition_effects(&plan,guard.lines())?;
            let source = std::sync::Arc::new(prepare_inventory_quality_count(c,guard,facts,effects,None).await?);
            let projection = inspect_quality_count_decision_projection(c,source.clone()).await?;
            projection.require_before(c).await?;
            // Execute the legacy SQL using the pinned Bun/pg runtime against
            // this newly leased, native-seeded database. Preserve JS key order.
            let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&mut *c).await?;
            let bun=std::env::var_os("ERPX_BUN_BIN").expect("pinned Bun required");
            let version=std::process::Command::new(&bun).arg("--version").output().expect("Bun version");
            assert!(version.status.success());
            assert_eq!(String::from_utf8(version.stdout).unwrap().trim(),"1.4.3");
            let js=r#"import pg from 'pg'; import {createHash} from 'node:crypto';
await import(new URL('../server-runtime/src/adapters/runtime/postgres.ts','file://'+process.cwd()+'/').href);const {Client}=pg;
const u=new URL(process.env.ERPX_TEST_ADMIN_URL);u.pathname='/'+process.env.COUNT_TEST_DB;const c=new Client({connectionString:u.toString()});await c.connect();
const lines=(await c.query(`SELECT l.*,p.sku,p.tracking_mode,s.quantity_milli current_quantity_milli,s.value_minor current_value_minor,COALESCE(s.version,0) current_version FROM inventory_count_lines l JOIN commerce_products p ON p.id=l.product_id LEFT JOIN commerce_stock s ON s.product_id=l.product_id AND s.warehouse_id=$1 WHERE l.count_id=$2 ORDER BY l.ordinal`,['warehouse','count-quality'])).rows;
const lotLines=(await c.query(`SELECT cl.*,b.quantity_milli current_quantity_milli,b.value_minor current_value_minor,COALESCE(b.version,0) current_version FROM inventory_count_lot_lines cl JOIN inventory_count_lines l ON l.id=cl.count_line_id LEFT JOIN inventory_lot_balances b ON b.lot_id=cl.result_lot_id AND b.warehouse_id=$1 WHERE l.count_id=$2 ORDER BY l.ordinal,cl.id`,['warehouse','count-quality'])).rows;
const digest=createHash('sha256').update(JSON.stringify({reason:'Approved scrap',lines,lotLines})).digest('hex');process.stdout.write(JSON.stringify({lines:JSON.stringify(lines),lots:JSON.stringify(lotLines),digest}));await c.end();"#;
            let output=std::process::Command::new(bun).arg("-e").arg(js).env("COUNT_TEST_DB",db).current_dir(env!("CARGO_MANIFEST_DIR")).output().expect("pinned Bun reference");
            assert!(output.status.success(),"pinned Bun reference failed");
            let oracle:serde_json::Value=serde_json::from_slice(&output.stdout).expect("Bun reference JSON");
            assert_eq!(projection.lines_json(),oracle["lines"].as_str().unwrap());
            assert_eq!(projection.lot_lines_json(),oracle["lots"].as_str().unwrap());
            let digest=erpx_workflow_server::public::service_count_decision_evidence::service_count_decision_digest("Approved scrap",projection.lines_json(),projection.lot_lines_json())?;
            assert_eq!(digest,oracle["digest"].as_str().unwrap());
            sqlx::query("SAVEPOINT projection_large_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_count_lines SET description=left(description,length(description)-1)||'X' WHERE id='line-quality'").execute(&mut *c).await?;
            assert!(projection.require_before(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT projection_large_drift").execute(&mut *c).await?;
            projection.require_before(c).await?;
            sqlx::query("RELEASE SAVEPOINT projection_large_drift").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT projection_product_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE commerce_products SET sku='changed' WHERE id='product'").execute(&mut *c).await?;
            assert!(projection.require_before(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT projection_product_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT projection_product_drift").execute(&mut *c).await?;
            let prepared = prepare_quality_count_disposition(c,source.clone()).await?;
            let staged = prepared.apply_before_count(c).await?;
            let after_hold: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(after_hold,before);
            assert!(apply_quality_count_effects(c,2,&staged).await.is_err());
            let untouched: (String,i64) = sqlx::query_as("SELECT status,version FROM inventory_counts WHERE id='count-quality'").fetch_one(&mut *c).await?;
            assert_eq!(untouched,("approved".into(),1));
            apply_quality_count_effects(c,1,&staged).await?;
            let staged = staged.stage_remaining(c).await?;
            apply_quality_count_decision(c,1,&staged).await?;
            use erpx_quality_server::public::quality_commands::QualityCountDispositionSource;
            sqlx::query("SAVEPOINT after_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE commerce_stock SET value_minor=value_minor+1 WHERE warehouse_id='warehouse' AND product_id='product-two'").execute(&mut *c).await?;
            assert!(source.require_count_after(c,false).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT after_drift").execute(&mut *c).await?;
            sqlx::query("INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES('extra','count-quality','warehouse','extra-product',0,0,'2026-11-10',100)").execute(&mut *c).await?;
            assert!(source.require_count_after(c,false).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT after_drift").execute(&mut *c).await?;
            sqlx::query("INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES('extra-lot','lot-b','warehouse','stock_count','count-lot',0,0,'2026-11-10','reviewer',100)").execute(&mut *c).await?;
            assert!(source.require_count_after(c,false).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT after_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT after_drift").execute(&mut *c).await?;
            projection.require_after(c,false).await?;
            sqlx::query("SAVEPOINT projection_post_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE commerce_products SET sku='changed' WHERE id='product'").execute(&mut *c).await?;
            assert!(projection.require_after(c,false).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT projection_post_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT projection_post_drift").execute(&mut *c).await?;
            let retained = staged.finish_after_count(c).await?;
            let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM inventory_adjustment_moves WHERE count_id='count-quality' ORDER BY id").fetch_all(&mut *c).await?;
            assert_eq!(ids,vec!["count-quality-0","count-quality-1","count-quality-2"]);
            let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM inventory_lot_moves WHERE source_type='stock_count' ORDER BY id").fetch_all(&mut *c).await?;
            assert_eq!(ids,vec!["count-quality-0-0","count-quality-1-0"]);
            retained.require_current(c).await?;
            lineage.require_after_count(c,&mut lineage_counts,&retained).await?;
            let inserted: (i64,i64,i64) = sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product-three'").fetch_one(&mut *c).await?;
            assert_eq!(inserted,(1000,500,2));
            let lot: String = sqlx::query_scalar("SELECT status FROM inventory_lots WHERE id='lot-a'").fetch_one(&mut *c).await?;
            assert_eq!(lot,"scrapped");
            let stock: (i64,i64) = sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(stock,(0,before+1));
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn native_quality_count_checks_split_warehouse_lot_remains_depleted() {
    use erpx_inventory_server::public::counts::{apply_quality_count_effects, apply_quality_count_decision, inspect_count_transition_guard, lock_count_transition_guard_sources, plan_count_transition, plan_count_transition_effects, prepare_inventory_quality_count};
    use erpx_inventory_server::public::service_initial_physical_count::{CountAction, CountAuthority};
    use erpx_quality_server::public::quality_commands::{lock_quality_count_disposition_sources, prepare_quality_count_disposition, QualityCountDispositionAction, QualityCountDispositionFacts};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES('count-quality','company','branch','warehouse','QC-1','2026-11-10','scrap','draft',1,-5000,'maker','maker','reviewer',0,0);
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope) VALUES('hold-distinct','company','branch','warehouse','product','Failed inspection','2026-11-10','held','maker','fail',1,0,0,'lot-a',10000,'partial');
            UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='manager',disposition_count_id='count-quality',disposition_quantity_milli=10000 WHERE id='hold-distinct';
            UPDATE inventory_counts SET quality_hold_id='hold-distinct' WHERE id='count-quality';
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-quality','count-quality','product',1,'Tracked product','pcs',quantity_milli,value_minor,version,0,0,0 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot','line-quality','lot-a','lot-a','lot','L-A',quantity_milli,value_minor,version,0,0 FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse';
            INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('product-two','company','P2','Second product',500,'lot');
            INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES('warehouse','product-two',2000,1000);
            INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES('lot-b','company','product-two','lot','L-B','maker',0),('lot-c','company','product-two','lot','L-C','maker',0);
            INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES('lot-b','warehouse',2000,1000),('lot-c','warehouse',0,0);
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-two','count-quality','product-two',1,'Second product','pcs',quantity_milli,value_minor,version,1000,500,1 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product-two';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot-b','line-two','lot-b','lot-b','lot','L-B',quantity_milli,value_minor,version,1000,500 FROM inventory_lot_balances WHERE lot_id='lot-b' AND warehouse_id='warehouse';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot-c','line-two','lot-c','lot-c','lot','L-C',quantity_milli,value_minor,version,0,0 FROM inventory_lot_balances WHERE lot_id='lot-c' AND warehouse_id='warehouse';
            INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('product-three','company','P3','Absent stock',500,'none');
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES('line-three','count-quality','product-three',1,'Absent stock','pcs',0,0,0,1000,500,2);
            INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES('warehouse-two','product',1000,500);
            INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES('lot-a','warehouse-two',1000,500);
            UPDATE inventory_counts SET status='approved' WHERE id='count-quality'")
            .execute(pool.raw()).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run_once::<_, BusinessError, _>(TransactionOptions::default(), move |c| Box::pin(async move {
            lock_count_sources(c).await?;
            lock_count_transition_guard_sources(c).await?;
            lock_quality_count_disposition_sources(c).await?;
            let guard = inspect_count_transition_guard(c,"company",Some("branch"),"count-quality",CountAction::Post).await?;
            let facts = QualityCountDispositionFacts { company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),lot_id:Some("lot-a".into()),hold_id:"hold-distinct".into(),count_id:"count-quality".into(),command_key:"quality-key".into(),actor_id:"reviewer".into(),reason:"Approved scrap".into(),action:QualityCountDispositionAction::Post,now:100 };
            guard.require_current(c).await?;
            sqlx::query("SAVEPOINT snapshot_drift").execute(&mut *c).await?;
            for mutation in [
                "UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'",
                "UPDATE commerce_products SET version=version+1 WHERE id='product'",
                "UPDATE inventory_lot_balances SET version=version+1 WHERE lot_id='lot-a' AND warehouse_id='warehouse'",
                "INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES('late-drift','count-quality','warehouse','product',0,0,'2026-11-11',101)",
            ] {
                sqlx::query(mutation).execute(&mut *c).await?;
                assert!(guard.require_current(c).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT snapshot_drift").execute(&mut *c).await?;
            }
            sqlx::query("RELEASE SAVEPOINT snapshot_drift").execute(&mut *c).await?;
            let authority = CountAuthority { same_as_creator:false,same_as_preparer:false,company_scope:true,department_scope:false,can_prepare:false,can_approve:true };
            let plan = plan_count_transition(&guard.record().status,guard.record().version,1,CountAction::Post,&authority,Some("Approved scrap"),0)?;
            let effects = plan_count_transition_effects(&plan,guard.lines())?;
            let source = std::sync::Arc::new(prepare_inventory_quality_count(c,guard,facts,effects,None).await?);
            let prepared = prepare_quality_count_disposition(c,source.clone()).await?;
            let staged = prepared.apply_before_count(c).await?;
            let after_hold: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(after_hold,before);
            apply_quality_count_effects(c,1,&staged).await?;
            let staged = staged.stage_remaining(c).await?;
            apply_quality_count_decision(c,1,&staged).await?;
            use erpx_quality_server::public::quality_commands::QualityCountDispositionSource;
            sqlx::query("SAVEPOINT after_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE commerce_stock SET value_minor=value_minor+1 WHERE warehouse_id='warehouse' AND product_id='product-two'").execute(&mut *c).await?;
            assert!(source.require_count_after(c,false).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT after_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_lot_balances SET version=version+1 WHERE lot_id='lot-a' AND warehouse_id='warehouse-two'").execute(&mut *c).await?;
            assert!(source.require_count_after(c,false).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT after_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT after_drift").execute(&mut *c).await?;
            let retained = staged.finish_after_count(c).await?;
            let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM inventory_adjustment_moves WHERE count_id='count-quality' ORDER BY id").fetch_all(&mut *c).await?;
            assert_eq!(ids,vec!["count-quality-0","count-quality-1","count-quality-2"]);
            let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM inventory_lot_moves WHERE source_type='stock_count' ORDER BY id").fetch_all(&mut *c).await?;
            assert_eq!(ids,vec!["count-quality-0-0","count-quality-1-0"]);
            retained.require_current(c).await?;
            let inserted: (i64,i64,i64) = sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product-three'").fetch_one(&mut *c).await?;
            assert_eq!(inserted,(1000,500,2));
            let lot: String = sqlx::query_scalar("SELECT status FROM inventory_lots WHERE id='lot-a'").fetch_one(&mut *c).await?;
            assert_eq!(lot,"depleted");
            let stock: (i64,i64) = sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(stock,(0,before+1));
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn native_quality_count_checks_partial_scrap_and_remaining_trigger() {
    use erpx_inventory_server::public::counts::{apply_quality_count_effects, apply_quality_count_decision, inspect_count_transition_guard, lock_count_transition_guard_sources, plan_count_transition, plan_count_transition_effects, prepare_inventory_quality_count};
    use erpx_inventory_server::public::service_initial_physical_count::{CountAction, CountAuthority};
    use erpx_quality_server::public::quality_commands::{lock_quality_count_disposition_sources, prepare_quality_count_disposition, QualityCountDispositionAction, QualityCountDispositionFacts};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES('count-quality','company','branch','warehouse','QC-1','2026-11-10','scrap','draft',1,-2000,'maker','maker','reviewer',0,0);
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope) VALUES('hold-distinct','company','branch','warehouse','product','Failed inspection','2026-11-10','held','maker','fail',1,0,0,'lot-a',10000,'partial');
            UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='manager',disposition_count_id='count-quality',disposition_quantity_milli=4000 WHERE id='hold-distinct';
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,version,created_at,updated_at,quantity_scope) VALUES('hold-other','company','branch','warehouse','product','Other','2026-11-10','cancelled','maker',1,0,0,'whole');
            UPDATE inventory_counts SET quality_hold_id='hold-distinct' WHERE id='count-quality';
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-quality','count-quality','product',1,'Tracked product','pcs',quantity_milli,value_minor,version,6000,3000,0 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot','line-quality','lot-a','lot-a','lot','L-A',quantity_milli,value_minor,version,6000,3000 FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse';
            UPDATE inventory_counts SET status='approved' WHERE id='count-quality'")
            .execute(pool.raw()).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        let other_pool = pool.clone();
        PgUnitOfWork::new(pool.clone()).run_once::<_, BusinessError, _>(TransactionOptions::default(), move |c| Box::pin(async move {
            lock_count_sources(c).await?;
            lock_count_transition_guard_sources(c).await?;
            lock_quality_count_disposition_sources(c).await?;
            let mut lineage_counts=ActualLineageCounts;
            let lineage=erpx_quality_server::public::purchase_lineage::inspect_quality_purchase_lineage(c,&mut lineage_counts,erpx_quality_server::public::purchase_lineage::QualityLineageRequest { company_id:erpx_platform_shared_server::public::CompanyId::new("company")?, branch_id:Some("branch".into()),hold_id:"hold-distinct".into() }).await?;
            let decoy=erpx_quality_server::public::purchase_lineage::inspect_quality_purchase_lineage(c,&mut lineage_counts,erpx_quality_server::public::purchase_lineage::QualityLineageRequest { company_id:erpx_platform_shared_server::public::CompanyId::new("company")?, branch_id:Some("branch".into()),hold_id:"hold-other".into() }).await?;
            let guard = inspect_count_transition_guard(c,"company",Some("branch"),"count-quality",CountAction::Post).await?;
            let facts = QualityCountDispositionFacts { company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),lot_id:Some("lot-a".into()),hold_id:"hold-distinct".into(),count_id:"count-quality".into(),command_key:"quality-key".into(),actor_id:"reviewer".into(),reason:"Approved scrap".into(),action:QualityCountDispositionAction::Post,now:100 };
            guard.require_current(c).await?;
            let authority = CountAuthority { same_as_creator:false,same_as_preparer:false,company_scope:true,department_scope:false,can_prepare:false,can_approve:true };
            let plan = plan_count_transition(&guard.record().status,guard.record().version,1,CountAction::Post,&authority,Some("Approved scrap"),0)?;
            let effects = plan_count_transition_effects(&plan,guard.lines())?;
            let source = std::sync::Arc::new(prepare_inventory_quality_count(c,guard,facts,effects,None).await?);
            let prepared = prepare_quality_count_disposition(c,source.clone()).await?;
            let staged = prepared.apply_before_count(c).await?;
            let after_hold: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(after_hold,before);
            apply_quality_count_effects(c,1,&staged).await?;
            let staged = staged.stage_remaining(c).await?;
            assert_eq!(staged.remaining_hold_id(),Some("hold-distinct-remaining-count-quality"));
            let child_proof=staged.child_notice_proof(c).await?.expect("partial child proof");
            assert_eq!(child_proof.child_id(),"hold-distinct-remaining-count-quality");
            child_proof.require_current(c).await?;
            let mut other=other_pool.raw().begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;
            assert!(child_proof.require_current(&mut other).await.is_err());
            other.rollback().await?;
            sqlx::query("SAVEPOINT child_proof_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET reason='forged child' WHERE id='hold-distinct-remaining-count-quality'").execute(&mut *c).await?;
            assert!(child_proof.require_current(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT child_proof_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT child_proof_drift").execute(&mut *c).await?;
            let pending: (String,i64,i64) = sqlx::query_as("SELECT status,version,(SELECT count(*) FROM inventory_quality_disposition_events WHERE hold_id='hold-distinct') FROM inventory_counts WHERE id='count-quality'").fetch_one(&mut *c).await?;
            assert_eq!(pending,("approved".into(),1,0));
            sqlx::query("SAVEPOINT before_decision_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_counts SET created_at=created_at+1 WHERE id='count-quality'").execute(&mut *c).await?;
            assert!(apply_quality_count_decision(c,1,&staged).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT before_decision_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT before_decision_drift").execute(&mut *c).await?;
            apply_quality_count_decision(c,1,&staged).await?;
            let retained = std::sync::Arc::new(staged.finish_after_count(c).await?);
            drop(source);
            retained.require_current(c).await?;
            lineage.require_after_count(c,&mut lineage_counts,&retained).await?;
            assert!(decoy.require_after_count(c,&mut lineage_counts,&retained).await.is_err());
            sqlx::query("SAVEPOINT lineage_final_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET reason='forged child' WHERE id='hold-distinct-remaining-count-quality'").execute(&mut *c).await?;
            assert!(lineage.require_after_count(c,&mut lineage_counts,&retained).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT lineage_final_drift").execute(&mut *c).await?;
            assert!(sqlx::query("UPDATE inventory_quality_holds SET reason='forged root' WHERE id='hold-distinct'").execute(&mut *c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT lineage_final_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT lineage_final_drift").execute(&mut *c).await?;
            child_proof.require_current(c).await?;
            let requester=retained.requester_notice_proof().expect("posted requester proof");
            assert_eq!(requester.requester(),"manager");
            assert_eq!(requester.version(),2);
            assert_eq!(requester.facts().hold_id,"hold-distinct");
            requester.require_current(c).await?;
            sqlx::query("SAVEPOINT requester_proof_drift").execute(&mut *c).await?;
            assert!(sqlx::query("UPDATE inventory_quality_holds SET disposition_requested_by='forged' WHERE id='hold-distinct'").execute(&mut *c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT requester_proof_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_counts SET created_at=created_at+1 WHERE id='count-quality'").execute(&mut *c).await?;
            assert!(requester.require_current(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT requester_proof_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT requester_proof_drift").execute(&mut *c).await?;
            let lot: String = sqlx::query_scalar("SELECT status FROM inventory_lots WHERE id='lot-a'").fetch_one(&mut *c).await?;
            assert_eq!(lot,"active");
            assert_eq!(retained.remaining_hold_id(),Some("hold-distinct-remaining-count-quality"));
            let remaining: i64 = sqlx::query_scalar("SELECT held_quantity_milli FROM inventory_quality_holds WHERE id='hold-distinct-remaining-count-quality'").fetch_one(&mut *c).await?;
            assert_eq!(remaining,6000);
            let stock: (i64,i64) = sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(stock,(6000,before+2));
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn native_quality_count_checks_finalized_purchase_ancestry_real_owner() {
    use erpx_inventory_server::public::counts::{apply_quality_count_effects, apply_quality_count_decision, inspect_count_transition_guard, lock_count_transition_guard_sources, plan_count_transition, plan_count_transition_effects, prepare_inventory_quality_count};
    use erpx_inventory_server::public::service_initial_physical_count::{CountAction, CountAuthority};
    use erpx_quality_server::public::quality_commands::{lock_quality_count_disposition_sources, prepare_quality_count_disposition, QualityCountDispositionAction, QualityCountDispositionFacts};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope,source_type,source_id) VALUES('root','company','branch','warehouse','product','Failed','2026-11-10','held','maker','fail',1,0,0,'lot-a',10000,'partial','goods_receipt','origin');
            INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,quality_hold_id,created_by,prepared_by,created_at,updated_at) VALUES('prev','company','branch','warehouse','PREV','2026-11-09','Scrap','posted','root','maker','maker',0,0);
            UPDATE inventory_quality_holds SET status='disposed',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='manager',disposition_count_id='prev',disposition_quantity_milli=2000,disposed_by='reviewer',disposed_at=1 WHERE id='root';
            INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES('prev-post','root','prev','post','reviewer',1,'Scrap',1)").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES('count-quality','company','branch','warehouse','QC-1','2026-11-10','scrap','draft',1,-2000,'maker','maker','reviewer',0,0);
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope,source_type,source_id) VALUES('root-remaining-prev','company','branch','warehouse','product','Failed inspection','2026-11-10','held','maker','fail',1,0,0,'lot-a',8000,'partial','goods_receipt','origin:remaining:prev');
            UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='manager',disposition_count_id='count-quality',disposition_quantity_milli=4000 WHERE id='root-remaining-prev';
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,version,created_at,updated_at,quantity_scope) VALUES('hold-other','company','branch','warehouse','product','Other','2026-11-10','cancelled','maker',1,0,0,'whole');
            UPDATE inventory_counts SET quality_hold_id='root-remaining-prev' WHERE id='count-quality';
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-quality','count-quality','product',1,'Tracked product','pcs',quantity_milli,value_minor,version,6000,3000,0 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot','line-quality','lot-a','lot-a','lot','L-A',quantity_milli,value_minor,version,6000,3000 FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse';
            UPDATE inventory_counts SET status='approved' WHERE id='count-quality'")
            .execute(pool.raw()).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        let other_pool = pool.clone();
        let (prior_lineage,prior_retained)=PgUnitOfWork::new(pool.clone()).run_once::<_, BusinessError, _>(TransactionOptions::default(), move |c| Box::pin(async move {
            lock_count_sources(c).await?;
            lock_count_transition_guard_sources(c).await?;
            lock_quality_count_disposition_sources(c).await?;
            let mut lineage_counts=ActualLineageCounts;
            let lineage=erpx_quality_server::public::purchase_lineage::inspect_quality_purchase_lineage(c,&mut lineage_counts,erpx_quality_server::public::purchase_lineage::QualityLineageRequest { company_id:erpx_platform_shared_server::public::CompanyId::new("company")?, branch_id:Some("branch".into()),hold_id:"root-remaining-prev".into() }).await?;
            assert_eq!(lineage.root()["id"],"root");
            let decoy=erpx_quality_server::public::purchase_lineage::inspect_quality_purchase_lineage(c,&mut lineage_counts,erpx_quality_server::public::purchase_lineage::QualityLineageRequest { company_id:erpx_platform_shared_server::public::CompanyId::new("company")?, branch_id:Some("branch".into()),hold_id:"hold-other".into() }).await?;
            let guard = inspect_count_transition_guard(c,"company",Some("branch"),"count-quality",CountAction::Post).await?;
            let facts = QualityCountDispositionFacts { company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),lot_id:Some("lot-a".into()),hold_id:"root-remaining-prev".into(),count_id:"count-quality".into(),command_key:"quality-key".into(),actor_id:"reviewer".into(),reason:"Approved scrap".into(),action:QualityCountDispositionAction::Post,now:100 };
            guard.require_current(c).await?;
            let authority = CountAuthority { same_as_creator:false,same_as_preparer:false,company_scope:true,department_scope:false,can_prepare:false,can_approve:true };
            let plan = plan_count_transition(&guard.record().status,guard.record().version,1,CountAction::Post,&authority,Some("Approved scrap"),0)?;
            let effects = plan_count_transition_effects(&plan,guard.lines())?;
            let source = std::sync::Arc::new(prepare_inventory_quality_count(c,guard,facts,effects,None).await?);
            let prepared = prepare_quality_count_disposition(c,source.clone()).await?;
            let staged = prepared.apply_before_count(c).await?;
            let after_hold: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(after_hold,before);
            apply_quality_count_effects(c,1,&staged).await?;
            let staged = staged.stage_remaining(c).await?;
            assert_eq!(staged.remaining_hold_id(),Some("root-remaining-prev-remaining-count-quality"));
            let child_proof=staged.child_notice_proof(c).await?.expect("partial child proof");
            assert_eq!(child_proof.child_id(),"root-remaining-prev-remaining-count-quality");
            child_proof.require_current(c).await?;
            let mut other=other_pool.raw().begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;
            assert!(child_proof.require_current(&mut other).await.is_err());
            other.rollback().await?;
            sqlx::query("SAVEPOINT child_proof_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET reason='forged child' WHERE id='root-remaining-prev-remaining-count-quality'").execute(&mut *c).await?;
            assert!(child_proof.require_current(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT child_proof_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT child_proof_drift").execute(&mut *c).await?;
            let pending: (String,i64,i64) = sqlx::query_as("SELECT status,version,(SELECT count(*) FROM inventory_quality_disposition_events WHERE hold_id='root-remaining-prev') FROM inventory_counts WHERE id='count-quality'").fetch_one(&mut *c).await?;
            assert_eq!(pending,("approved".into(),1,0));
            sqlx::query("SAVEPOINT before_decision_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_counts SET created_at=created_at+1 WHERE id='count-quality'").execute(&mut *c).await?;
            assert!(apply_quality_count_decision(c,1,&staged).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT before_decision_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT before_decision_drift").execute(&mut *c).await?;
            apply_quality_count_decision(c,1,&staged).await?;
            let retained = std::sync::Arc::new(staged.finish_after_count(c).await?);
            drop(source);
            retained.require_current(c).await?;
            lineage.require_after_count(c,&mut lineage_counts,&retained).await?;
            sqlx::query("SAVEPOINT ancestor_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_counts SET status='draft' WHERE id='prev'").execute(&mut *c).await?;
            assert!(lineage.require_after_count(c,&mut lineage_counts,&retained).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT ancestor_drift").execute(&mut *c).await?;
            assert!(sqlx::query("UPDATE inventory_quality_holds SET reason='forged ancestor' WHERE id='root'").execute(&mut *c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT ancestor_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT ancestor_drift").execute(&mut *c).await?;
            assert!(decoy.require_after_count(c,&mut lineage_counts,&retained).await.is_err());
            sqlx::query("SAVEPOINT lineage_final_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET reason='forged child' WHERE id='root-remaining-prev-remaining-count-quality'").execute(&mut *c).await?;
            assert!(lineage.require_after_count(c,&mut lineage_counts,&retained).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT lineage_final_drift").execute(&mut *c).await?;
            assert!(sqlx::query("UPDATE inventory_quality_holds SET reason='forged root' WHERE id='root-remaining-prev'").execute(&mut *c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT lineage_final_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT lineage_final_drift").execute(&mut *c).await?;
            child_proof.require_current(c).await?;
            let requester=retained.requester_notice_proof().expect("posted requester proof");
            assert_eq!(requester.requester(),"manager");
            assert_eq!(requester.version(),2);
            assert_eq!(requester.facts().hold_id,"root-remaining-prev");
            requester.require_current(c).await?;
            sqlx::query("SAVEPOINT requester_proof_drift").execute(&mut *c).await?;
            assert!(sqlx::query("UPDATE inventory_quality_holds SET disposition_requested_by='forged' WHERE id='root-remaining-prev'").execute(&mut *c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT requester_proof_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_counts SET created_at=created_at+1 WHERE id='count-quality'").execute(&mut *c).await?;
            assert!(requester.require_current(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT requester_proof_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT requester_proof_drift").execute(&mut *c).await?;
            let lot: String = sqlx::query_scalar("SELECT status FROM inventory_lots WHERE id='lot-a'").fetch_one(&mut *c).await?;
            assert_eq!(lot,"active");
            assert_eq!(retained.remaining_hold_id(),Some("root-remaining-prev-remaining-count-quality"));
            let remaining: i64 = sqlx::query_scalar("SELECT held_quantity_milli FROM inventory_quality_holds WHERE id='root-remaining-prev-remaining-count-quality'").fetch_one(&mut *c).await?;
            assert_eq!(remaining,4000);
            let stock: (i64,i64) = sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(stock,(6000,before+2));
            Ok((lineage,retained))
        })).await.unwrap();
        let mut another=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *another).await.unwrap();
        lock_count_sources(&mut another).await.unwrap();lock_count_transition_guard_sources(&mut another).await.unwrap();lock_quality_count_disposition_sources(&mut another).await.unwrap();
        let mut other_counts=ActualLineageCounts;
        assert!(prior_lineage.require_after_count(&mut another,&mut other_counts,&prior_retained).await.is_err());
        another.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn native_quality_count_checks_reject_restores_hold_and_records_event() {
    use erpx_inventory_server::public::counts::{apply_quality_count_effects, apply_quality_count_decision, inspect_count_transition_guard, inspect_quality_count_decision_projection, lock_count_transition_guard_sources, plan_count_transition, plan_count_transition_effects, prepare_inventory_quality_count};
    use erpx_inventory_server::public::service_initial_physical_count::{CountAction, CountAuthority};
    use erpx_quality_server::public::quality_commands::{lock_quality_count_disposition_sources, prepare_quality_count_disposition, QualityCountDispositionAction, QualityCountDispositionFacts};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES('count-quality','company','branch','warehouse','QC-1','2026-11-10','scrap','draft',1,-5000,'maker','maker','reviewer',0,0);
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope) VALUES('hold-distinct','company','branch','warehouse','product','Failed inspection','2026-11-10','held','maker','fail',1,0,0,'lot-a',10000,'partial');
            UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='manager',disposition_count_id='count-quality',disposition_quantity_milli=10000 WHERE id='hold-distinct';
            UPDATE inventory_counts SET quality_hold_id='hold-distinct' WHERE id='count-quality';
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-quality','count-quality','product',1,'Tracked product','pcs',quantity_milli,value_minor,version,0,0,0 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot','line-quality','lot-a','lot-a','lot','L-A',quantity_milli,value_minor,version,0,0 FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse';
            UPDATE inventory_count_lines SET counted_quantity_milli=1000,counted_value_minor=9007199254740992 WHERE id='line-quality';
            UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product';
            UPDATE inventory_counts SET status='approved' WHERE id='count-quality'")
            .execute(pool.raw()).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run_once::<_, BusinessError, _>(TransactionOptions::default(), move |c| Box::pin(async move {
            lock_count_sources(c).await?;
            lock_count_transition_guard_sources(c).await?;
            lock_quality_count_disposition_sources(c).await?;
            let mut lineage_counts=ActualLineageCounts;
            let lineage=erpx_quality_server::public::purchase_lineage::inspect_quality_purchase_lineage(c,&mut lineage_counts,erpx_quality_server::public::purchase_lineage::QualityLineageRequest { company_id:erpx_platform_shared_server::public::CompanyId::new("company")?, branch_id:Some("branch".into()),hold_id:"hold-distinct".into() }).await?;
            let guard = inspect_count_transition_guard(c,"company",Some("branch"),"count-quality",CountAction::Reject).await?;
            let facts = QualityCountDispositionFacts { company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),lot_id:Some("lot-a".into()),hold_id:"hold-distinct".into(),count_id:"count-quality".into(),command_key:"quality-key".into(),actor_id:"reviewer".into(),reason:"Approved scrap".into(),action:QualityCountDispositionAction::Reject,now:100 };
            guard.require_current(c).await?;
            let authority = CountAuthority { same_as_creator:false,same_as_preparer:false,company_scope:true,department_scope:false,can_prepare:false,can_approve:true };
            let plan = plan_count_transition(&guard.record().status,guard.record().version,1,CountAction::Reject,&authority,Some("Approved scrap"),0)?;
            let effects = plan_count_transition_effects(&plan,guard.lines())?;
            let source = std::sync::Arc::new(prepare_inventory_quality_count(c,guard,facts,effects,None).await?);
            sqlx::query("SAVEPOINT stale_hold").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET status='held',disposition_kind=NULL,disposition_reason=NULL,disposition_requested_by=NULL,disposition_count_id=NULL,disposition_quantity_milli=NULL WHERE id='hold-distinct'").execute(&mut *c).await?;
            assert!(prepare_quality_count_disposition(c,source.clone()).await.is_err());
            let events: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_disposition_events WHERE hold_id='hold-distinct'").fetch_one(&mut *c).await?;
            assert_eq!(events,0);
            sqlx::query("ROLLBACK TO SAVEPOINT stale_hold").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT stale_hold").execute(&mut *c).await?;
            assert!(inspect_quality_count_decision_projection(c,source.clone()).await.is_err());
            let prepared = prepare_quality_count_disposition(c,source.clone()).await?;
            let staged = prepared.apply_before_count(c).await?;
            let after_hold: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(after_hold,before);
            apply_quality_count_effects(c,1,&staged).await?;
            let staged = staged.stage_remaining(c).await?;
            apply_quality_count_decision(c,1,&staged).await?;
            use erpx_quality_server::public::quality_commands::QualityCountDispositionSource;
            sqlx::query("SAVEPOINT reject_drift").execute(&mut *c).await?;
            for mutation in [
                "UPDATE inventory_counts SET number='QC-drift' WHERE id='count-quality'",
                "UPDATE inventory_count_lines SET description='changed' WHERE id='line-quality'",
                "UPDATE inventory_count_lot_lines SET lot_code='changed' WHERE id='count-lot'",
                "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES('other-source','lot-a','warehouse','stock_count','other-line',0,0,'2026-11-10','reviewer',100)",
                "UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'",
                "UPDATE inventory_lots SET status='depleted' WHERE id='lot-a'",
                "UPDATE inventory_lot_balances SET version=version+1 WHERE lot_id='lot-a' AND warehouse_id='warehouse'",
                "INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES('reject-extra','count-quality','warehouse','product',0,0,'2026-11-10',100)",
                "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES('reject-extra-lot','lot-a','warehouse','stock_count','count-lot',0,0,'2026-11-10','reviewer',100)"
            ] {
                sqlx::query(mutation).execute(&mut *c).await?;
                assert!(source.require_count_after(c,false).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT reject_drift").execute(&mut *c).await?;
            }
            sqlx::query("RELEASE SAVEPOINT reject_drift").execute(&mut *c).await?;
            let retained = staged.finish_after_count(c).await?;
            retained.require_current(c).await?;
            lineage.require_after_count(c,&mut lineage_counts,&retained).await?;
            let lot: String = sqlx::query_scalar("SELECT status FROM inventory_lots WHERE id='lot-a'").fetch_one(&mut *c).await?;
            assert_eq!(lot,"active");
            let hold: (String,Option<String>) = sqlx::query_as("SELECT status,disposition_count_id FROM inventory_quality_holds WHERE id='hold-distinct'").fetch_one(&mut *c).await?;
            assert_eq!(hold,("held".into(),None));
            let event: String = sqlx::query_scalar("SELECT action FROM inventory_quality_disposition_events WHERE id='quality-disposition-quality-key'").fetch_one(&mut *c).await?;
            assert_eq!(event,"reject");
            let stock: (i64,i64) = sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(stock,(10000,before));
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn native_quality_count_checks_projection_detects_omitted_row_drift_after_reject() {
    use erpx_inventory_server::public::counts::{apply_quality_count_effects, apply_quality_count_decision, inspect_count_transition_guard, inspect_quality_count_decision_projection, lock_count_transition_guard_sources, plan_count_transition, plan_count_transition_effects, prepare_inventory_quality_count};
    use erpx_inventory_server::public::service_initial_physical_count::{CountAction, CountAuthority};
    use erpx_quality_server::public::quality_commands::{lock_quality_count_disposition_sources, prepare_quality_count_disposition, QualityCountDispositionAction, QualityCountDispositionFacts};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES('count-quality','company','branch','warehouse','QC-1','2026-11-10','scrap','draft',1,-5000,'maker','maker','reviewer',0,0);
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope) VALUES('hold-distinct','company','branch','warehouse','product','Failed inspection','2026-11-10','held','maker','fail',1,0,0,'lot-a',10000,'partial');
            UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='manager',disposition_count_id='count-quality',disposition_quantity_milli=10000 WHERE id='hold-distinct';
            UPDATE inventory_counts SET quality_hold_id='hold-distinct' WHERE id='count-quality';
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-quality','count-quality','product',1,'Tracked product','pcs',quantity_milli,value_minor,version,0,0,0 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot','line-quality','lot-a','lot-a','lot','L-A',quantity_milli,value_minor,version,0,0 FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse';
            UPDATE inventory_count_lines SET counted_quantity_milli=1000,counted_value_minor=500 WHERE id='line-quality';
            UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product';
            UPDATE inventory_counts SET status='approved' WHERE id='count-quality'")
            .execute(pool.raw()).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run_once::<_, BusinessError, _>(TransactionOptions::default(), move |c| Box::pin(async move {
            lock_count_sources(c).await?;
            lock_count_transition_guard_sources(c).await?;
            lock_quality_count_disposition_sources(c).await?;
            let guard = inspect_count_transition_guard(c,"company",Some("branch"),"count-quality",CountAction::Reject).await?;
            let facts = QualityCountDispositionFacts { company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),lot_id:Some("lot-a".into()),hold_id:"hold-distinct".into(),count_id:"count-quality".into(),command_key:"quality-key".into(),actor_id:"reviewer".into(),reason:"Approved scrap".into(),action:QualityCountDispositionAction::Reject,now:100 };
            guard.require_current(c).await?;
            let authority = CountAuthority { same_as_creator:false,same_as_preparer:false,company_scope:true,department_scope:false,can_prepare:false,can_approve:true };
            let plan = plan_count_transition(&guard.record().status,guard.record().version,1,CountAction::Reject,&authority,Some("Approved scrap"),0)?;
            let effects = plan_count_transition_effects(&plan,guard.lines())?;
            let source = std::sync::Arc::new(prepare_inventory_quality_count(c,guard,facts,effects,None).await?);
            sqlx::query("SAVEPOINT stale_hold").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET status='held',disposition_kind=NULL,disposition_reason=NULL,disposition_requested_by=NULL,disposition_count_id=NULL,disposition_quantity_milli=NULL WHERE id='hold-distinct'").execute(&mut *c).await?;
            assert!(prepare_quality_count_disposition(c,source.clone()).await.is_err());
            let events: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_disposition_events WHERE hold_id='hold-distinct'").fetch_one(&mut *c).await?;
            assert_eq!(events,0);
            sqlx::query("ROLLBACK TO SAVEPOINT stale_hold").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT stale_hold").execute(&mut *c).await?;
            let projection=inspect_quality_count_decision_projection(c,source.clone()).await?;
            let prepared = prepare_quality_count_disposition(c,source.clone()).await?;
            let staged = prepared.apply_before_count(c).await?;
            let after_hold: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(after_hold,before);
            apply_quality_count_effects(c,1,&staged).await?;
            let staged = staged.stage_remaining(c).await?;
            apply_quality_count_decision(c,1,&staged).await?;
            projection.require_after(c,false).await?;
            sqlx::query("SAVEPOINT full_row_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_count_lines SET description='forged' WHERE id='line-quality'").execute(&mut *c).await?;
            assert!(projection.require_after(c,false).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT full_row_drift").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_count_lot_lines SET unit_cost_minor=7 WHERE id='count-lot'").execute(&mut *c).await?;
            assert!(projection.require_after(c,false).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT full_row_drift").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT full_row_drift").execute(&mut *c).await?;
            use erpx_quality_server::public::quality_commands::QualityCountDispositionSource;
            sqlx::query("SAVEPOINT reject_drift").execute(&mut *c).await?;
            for mutation in [
                "UPDATE inventory_counts SET number='QC-drift' WHERE id='count-quality'",
                "UPDATE inventory_count_lines SET description='changed' WHERE id='line-quality'",
                "UPDATE inventory_count_lot_lines SET lot_code='changed' WHERE id='count-lot'",
                "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES('other-source','lot-a','warehouse','stock_count','other-line',0,0,'2026-11-10','reviewer',100)",
                "UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'",
                "UPDATE inventory_lots SET status='depleted' WHERE id='lot-a'",
                "UPDATE inventory_lot_balances SET version=version+1 WHERE lot_id='lot-a' AND warehouse_id='warehouse'",
                "INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES('reject-extra','count-quality','warehouse','product',0,0,'2026-11-10',100)",
                "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES('reject-extra-lot','lot-a','warehouse','stock_count','count-lot',0,0,'2026-11-10','reviewer',100)"
            ] {
                sqlx::query(mutation).execute(&mut *c).await?;
                assert!(source.require_count_after(c,false).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT reject_drift").execute(&mut *c).await?;
            }
            sqlx::query("RELEASE SAVEPOINT reject_drift").execute(&mut *c).await?;
            let retained = staged.finish_after_count(c).await?;
            retained.require_current(c).await?;
            let lot: String = sqlx::query_scalar("SELECT status FROM inventory_lots WHERE id='lot-a'").fetch_one(&mut *c).await?;
            assert_eq!(lot,"active");
            let hold: (String,Option<String>) = sqlx::query_as("SELECT status,disposition_count_id FROM inventory_quality_holds WHERE id='hold-distinct'").fetch_one(&mut *c).await?;
            assert_eq!(hold,("held".into(),None));
            let event: String = sqlx::query_scalar("SELECT action FROM inventory_quality_disposition_events WHERE id='quality-disposition-quality-key'").fetch_one(&mut *c).await?;
            assert_eq!(event,"reject");
            let stock: (i64,i64) = sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(stock,(10000,before));
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn native_quality_count_checks_reject_captures_unlisted_hold_lot() {
    use erpx_inventory_server::public::counts::{apply_quality_count_effects, apply_quality_count_decision, inspect_count_transition_guard, lock_count_transition_guard_sources, plan_count_transition, plan_count_transition_effects, prepare_inventory_quality_count};
    use erpx_inventory_server::public::service_initial_physical_count::{CountAction, CountAuthority};
    use erpx_quality_server::public::quality_commands::{lock_quality_count_disposition_sources, prepare_quality_count_disposition, QualityCountDispositionAction, QualityCountDispositionFacts};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES('count-quality','company','branch','warehouse','QC-1','2026-11-10','scrap','draft',1,-5000,'maker','maker','reviewer',0,0);
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope) VALUES('hold-distinct','company','branch','warehouse','product','Failed inspection','2026-11-10','held','maker','fail',1,0,0,'lot-a',10000,'partial');
            UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='manager',disposition_count_id='count-quality',disposition_quantity_milli=10000 WHERE id='hold-distinct';
            UPDATE inventory_counts SET quality_hold_id='hold-distinct' WHERE id='count-quality';
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) SELECT 'line-quality','count-quality','product',1,'Tracked product','pcs',quantity_milli,value_minor,version,0,0,0 FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product';
            INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor) SELECT 'count-lot','line-quality','lot-a','lot-a','lot','L-A',quantity_milli,value_minor,version,0,0 FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse';
            DELETE FROM inventory_count_lot_lines WHERE count_line_id='line-quality';
            UPDATE inventory_count_lines SET counted_quantity_milli=1000 WHERE id='line-quality';
            UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product';
            UPDATE inventory_counts SET status='approved' WHERE id='count-quality'")
            .execute(pool.raw()).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run_once::<_, BusinessError, _>(TransactionOptions::default(), move |c| Box::pin(async move {
            lock_count_sources(c).await?;
            lock_count_transition_guard_sources(c).await?;
            lock_quality_count_disposition_sources(c).await?;
            let guard = inspect_count_transition_guard(c,"company",Some("branch"),"count-quality",CountAction::Reject).await?;
            let facts = QualityCountDispositionFacts { company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),lot_id:Some("lot-a".into()),hold_id:"hold-distinct".into(),count_id:"count-quality".into(),command_key:"quality-key".into(),actor_id:"reviewer".into(),reason:"Approved scrap".into(),action:QualityCountDispositionAction::Reject,now:100 };
            guard.require_current(c).await?;
            let authority = CountAuthority { same_as_creator:false,same_as_preparer:false,company_scope:true,department_scope:false,can_prepare:false,can_approve:true };
            let plan = plan_count_transition(&guard.record().status,guard.record().version,1,CountAction::Reject,&authority,Some("Approved scrap"),0)?;
            let effects = plan_count_transition_effects(&plan,guard.lines())?;
            let source = std::sync::Arc::new(prepare_inventory_quality_count(c,guard,facts,effects,None).await?);
            sqlx::query("SAVEPOINT stale_hold").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET status='held',disposition_kind=NULL,disposition_reason=NULL,disposition_requested_by=NULL,disposition_count_id=NULL,disposition_quantity_milli=NULL WHERE id='hold-distinct'").execute(&mut *c).await?;
            assert!(prepare_quality_count_disposition(c,source.clone()).await.is_err());
            let events: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_disposition_events WHERE hold_id='hold-distinct'").fetch_one(&mut *c).await?;
            assert_eq!(events,0);
            sqlx::query("ROLLBACK TO SAVEPOINT stale_hold").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT stale_hold").execute(&mut *c).await?;
            let prepared = prepare_quality_count_disposition(c,source.clone()).await?;
            let staged = prepared.apply_before_count(c).await?;
            let after_hold: i64 = sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(after_hold,before);
            apply_quality_count_effects(c,1,&staged).await?;
            let staged = staged.stage_remaining(c).await?;
            apply_quality_count_decision(c,1,&staged).await?;
            use erpx_quality_server::public::quality_commands::QualityCountDispositionSource;
            sqlx::query("SAVEPOINT reject_drift").execute(&mut *c).await?;
            for mutation in [
                "UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'",
                "UPDATE inventory_lots SET status='depleted' WHERE id='lot-a'",
                "UPDATE inventory_lot_balances SET version=version+1 WHERE lot_id='lot-a' AND warehouse_id='warehouse'",
                "INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES('reject-extra','count-quality','warehouse','product',0,0,'2026-11-10',100)"
            ] {
                sqlx::query(mutation).execute(&mut *c).await?;
                assert!(source.require_count_after(c,false).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT reject_drift").execute(&mut *c).await?;
            }
            sqlx::query("RELEASE SAVEPOINT reject_drift").execute(&mut *c).await?;
            let retained = staged.finish_after_count(c).await?;
            retained.require_current(c).await?;
            let lot: String = sqlx::query_scalar("SELECT status FROM inventory_lots WHERE id='lot-a'").fetch_one(&mut *c).await?;
            assert_eq!(lot,"active");
            let hold: (String,Option<String>) = sqlx::query_as("SELECT status,disposition_count_id FROM inventory_quality_holds WHERE id='hold-distinct'").fetch_one(&mut *c).await?;
            assert_eq!(hold,("held".into(),None));
            let event: String = sqlx::query_scalar("SELECT action FROM inventory_quality_disposition_events WHERE id='quality-disposition-quality-key'").fetch_one(&mut *c).await?;
            assert_eq!(event,"reject");
            let stock: (i64,i64) = sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(stock,(10000,before));
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn native_quality_count_checks_direct_request_origin_not_receipt_authority() {
    use erpx_inventory_server::public::counts::{inspect_direct_quality_count_origin,lock_count_transition_guard_sources};
    use erpx_quality_server::public::quality_commands::lock_quality_count_disposition_sources;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('other','company','OTHER','Other',100,'none')").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,created_at,updated_at) VALUES('quality-scrap-origin','company','branch','warehouse','QC-ORIGIN','2026-11-10','scrap','draft',1,-5000,'maker','maker','reviewer',0,0),('quality-scrap-forged','company','branch','warehouse','QC-FORGED','2026-11-10','scrap','draft',1,-5000,'maker','maker','reviewer',0,0);
            INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_result,version,created_at,updated_at,lot_id,held_quantity_milli,quantity_scope) VALUES('hold-origin','company','branch','warehouse','product','Failed','2026-11-10','held','maker','fail',1,0,0,'lot-a',10000,'partial');
            UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason='Failed',disposition_requested_by='maker',disposition_count_id='quality-scrap-origin',disposition_quantity_milli=10000 WHERE id='hold-origin';
            UPDATE inventory_counts SET quality_hold_id='hold-origin' WHERE id IN ('quality-scrap-origin','quality-scrap-forged');
            INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES('origin-line','quality-scrap-origin','product',1,'product','pcs',10000,5000,1,0,0,0),('forged-line','quality-scrap-forged','product',1,'product','pcs',10000,5000,1,0,0,0);
            INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES('quality-disposition-origin','hold-origin','quality-scrap-origin','request','maker',1,'request',0),('quality-disposition-not-forged','hold-origin','quality-scrap-forged','request','maker',1,'request',0);
            UPDATE inventory_counts SET status='approved' WHERE id='quality-scrap-origin'; UPDATE inventory_counts SET status='rejected' WHERE id='quality-scrap-forged'").execute(pool.raw()).await.unwrap();
        let other_pool=pool.clone();
        PgUnitOfWork::new(pool.clone()).run_once::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_count_sources(c).await?;lock_count_transition_guard_sources(c).await?;lock_quality_count_disposition_sources(c).await?;
            assert!(inspect_direct_quality_count_origin(c,"company",Some("branch-two"),"quality-scrap-origin").await?.is_none());
            assert!(inspect_direct_quality_count_origin(c,"company",Some("branch"),"quality-scrap-forged").await?.is_none());
            let origin=inspect_direct_quality_count_origin(c,"company",Some("branch"),"quality-scrap-origin").await?.expect("owner origin");
            assert_eq!((origin.hold_id(),origin.creator(),origin.source_command_id()),("hold-origin","maker","origin"));
            origin.require_current(c).await?;
            let mut other=other_pool.raw().begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;
            assert!(origin.require_current(&mut other).await.is_err());
            other.rollback().await?;
            sqlx::query("SAVEPOINT posted_origin").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_counts SET status='posted',version=version+1 WHERE id='quality-scrap-origin'").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET status='disposed',disposed_by='reviewer',disposed_at=100,version=version+1 WHERE id='hold-origin'").execute(&mut *c).await?;
            origin.require_current(c).await?;
            sqlx::query("ROLLBACK TO SAVEPOINT posted_origin").execute(&mut *c).await?;
            sqlx::query("RELEASE SAVEPOINT posted_origin").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT direct_origin_drift").execute(&mut *c).await?;
            for mutation in [
                "UPDATE inventory_counts SET prepared_by='other' WHERE id='quality-scrap-origin'",
                "UPDATE inventory_count_lines SET counted_quantity_milli=snapshot_quantity_milli WHERE id='origin-line'",
                "UPDATE inventory_count_lines SET description='forged' WHERE id='origin-line'",
                "UPDATE inventory_quality_holds SET reason='forged' WHERE id='hold-origin'",
                "INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES('other-line','quality-scrap-origin','other',1,'other','pcs',10,10,1,0,0,1)"
            ] {
                sqlx::query(mutation).execute(&mut *c).await?;
                assert!(origin.require_current(c).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT direct_origin_drift").execute(&mut *c).await?;
            }
            sqlx::query("UPDATE inventory_counts SET status='rejected',review_reason='reject',version=version+1 WHERE id='quality-scrap-origin'").execute(&mut *c).await?;
            sqlx::query("UPDATE inventory_quality_holds SET status='held',disposition_kind=NULL,disposition_reason=NULL,disposition_requested_by=NULL,disposition_count_id=NULL,disposition_quantity_milli=NULL,version=version+1 WHERE id='hold-origin'").execute(&mut *c).await?;
            origin.require_current(c).await?;
            assert!(inspect_direct_quality_count_origin(c,"company",Some("branch"),"quality-scrap-origin").await?.is_some());
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_count_pre_snapshot_inversions_retry_whole_unit_without_effects() {
    use erpx_commerce_server::public::quality_scrap_stock::lock_quality_scrap_stock_sources;
    use erpx_inventory_server::public::quality_scrap_count::lock_quality_scrap_count_sources;
    use erpx_quality_server::public::quality_commands::{lock_quality_command_sources,lock_quality_count_disposition_sources};
    use erpx_platform_runtime_server::public::postgres::PgTransactionError;
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        sqlx::query("CREATE TABLE quality_count_retry_probe(id text PRIMARY KEY)").execute(pool.raw()).await.unwrap();
        // Count-side partial Count-table acquisition versus Quality-side stock
        // and hold acquisition reproduces the real opposing lock order.
        let mut count=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *count).await.unwrap();
        sqlx::query("LOCK TABLE inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE ROW EXCLUSIVE MODE").execute(&mut *count).await.unwrap();
        let mut quality=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *quality).await.unwrap();
        lock_quality_scrap_stock_sources(&mut quality).await.unwrap();lock_quality_command_sources(&mut quality).await.unwrap();
        let conflict=lock_quality_scrap_count_sources(&mut quality).await;
        assert!(matches!(conflict,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        quality.rollback().await.unwrap();lock_count_sources(&mut count).await.unwrap();count.rollback().await.unwrap();
        // Same inversion from Count's side: rollback Count releases its partial
        // table locks so the Quality pre-snapshot unit can continue.
        let mut count=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *count).await.unwrap();
        sqlx::query("LOCK TABLE inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE ROW EXCLUSIVE MODE").execute(&mut *count).await.unwrap();
        let mut quality=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *quality).await.unwrap();
        lock_quality_scrap_stock_sources(&mut quality).await.unwrap();lock_quality_command_sources(&mut quality).await.unwrap();
        let conflict=lock_count_sources(&mut count).await;
        assert!(matches!(conflict,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        count.rollback().await.unwrap();lock_quality_scrap_count_sources(&mut quality).await.unwrap();quality.rollback().await.unwrap();
        // Count→Quality-hold versus Quality-hold→Count is the second possible
        // inversion when the controlled path is integrated.
        let mut count=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *count).await.unwrap();lock_count_sources(&mut count).await.unwrap();
        let mut quality=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *quality).await.unwrap();lock_quality_command_sources(&mut quality).await.unwrap();
        let conflict=lock_quality_count_disposition_sources(&mut count).await;
        assert!(matches!(conflict,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        count.rollback().await.unwrap();lock_quality_scrap_count_sources(&mut quality).await.unwrap();quality.rollback().await.unwrap();
        // Exercise the production UoW retry loop: the first failed pre-snapshot
        // attempt waits for the test to release the competing owner, then the
        // entire callback starts again and writes exactly one marker.
        let mut blocker=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocker).await.unwrap();lock_quality_scrap_stock_sources(&mut blocker).await.unwrap();
        let conflict_seen=std::sync::Arc::new(tokio::sync::Notify::new());let released=std::sync::Arc::new(tokio::sync::Notify::new());let attempts=std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker_pool=pool.clone();let seen=conflict_seen.clone();let gate=released.clone();let count_attempts=attempts.clone();
        let worker=tokio::spawn(async move {
            PgUnitOfWork::new(worker_pool).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{
                let seen=seen.clone();let gate=gate.clone();let count_attempts=count_attempts.clone();
                Box::pin(async move {
                    let attempt=count_attempts.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
                    if let Err(error)=lock_count_sources(c).await {
                        if attempt==0 {seen.notify_one();gate.notified().await;}
                        return Err(error);
                    }
                    lock_quality_count_disposition_sources(c).await?;
                    sqlx::query("INSERT INTO quality_count_retry_probe(id) VALUES('one')").execute(c).await?;
                    Ok(())
                })
            }).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5),conflict_seen.notified()).await.expect("first attempt hit pre-snapshot conflict");
        blocker.rollback().await.unwrap();released.notify_one();worker.await.unwrap().unwrap();
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst),2);
        let rows:i64=sqlx::query_scalar("SELECT count(*) FROM quality_count_retry_probe WHERE id='one'").fetch_one(pool.raw()).await.unwrap();assert_eq!(rows,1);
    }).await;
}


#[test]
fn large_internal_projection_matches_pinned_bun_without_changing_command_limit() {
        use std::io::Write;
        let bun = std::env::var_os("ERPX_BUN_BIN").expect("pinned Bun required");
        let version = std::process::Command::new(&bun).arg("--version").output().expect("Bun version");
        assert!(version.status.success());
        assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "1.4.3");
        for (first, second) in [(2_100_000, 1), (1_100_000, 1_100_000)] {
            let lines = format!("[{{\"id\":\"a\",\"description\":{},\"quantity\":9007199254740991,\"decimal\":1.25,\"negative_zero\":-0,\"keys\":{{\"10\":1,\"2\":2,\"a\":3}}}},{{\"id\":\"b\",\"description\":{},\"optional\":null}}]", serde_json::to_string(&format!("{}ไทย🙂\n\"\\", "x".repeat(first))).unwrap(), serde_json::to_string(&"y".repeat(second)).unwrap());
            let lots = r#"[{"id":"lot-b","version":0},{"id":"lot-a","version":12}]"#;
            let reason = "Review ✓\n\"\\";
            let input = serde_json::json!({"reason":reason,"lines":lines,"lots":lots}).to_string();
            let script = "import {createHash} from 'node:crypto';const x=JSON.parse(await Bun.stdin.text());const lines=JSON.parse(x.lines),lotLines=JSON.parse(x.lots);process.stdout.write(createHash('sha256').update(JSON.stringify({reason:x.reason,lines,lotLines})).digest('hex'));";
            let mut child = std::process::Command::new(&bun).args(["-e", script]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
            child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(result.status.success());
            assert_eq!(erpx_workflow_server::public::service_count_decision_evidence::service_count_decision_digest(reason, &lines, lots).unwrap(), String::from_utf8(result.stdout).unwrap());
            assert!(erpx_workflow_server::public::commands::CommandJson::parse(&lines).is_err(), "incoming command cap must remain enforced");
        }
    }
