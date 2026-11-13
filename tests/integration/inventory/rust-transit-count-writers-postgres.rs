//! Actual native Transit dispatch/receive/cancel and generic Count
//! create/revision effects over leased PostgreSQL. These cases prove owner
//! reads and writes only; they do not prove IAM, purchase admission, Workflow
//! receipts, Finance journals or Files evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_inventory::public::counts::{
    CountAllocationInput, CountLineInput, CountSaveInput, apply_count_save, lock_count_sources,
    plan_count_save, read_count_save_source,
};
use erpx_server_inventory::public::transit::{
    TransitCancelInput, TransitDispatchInput, TransitLotAllocationInput, TransitReceiveInput,
    TransitReceiveLineInput, apply_transit_cancel, apply_transit_dispatch, apply_transit_receive,
    lock_transit_sources, plan_cancel, plan_dispatch, plan_receive, read_transit_source,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::BusinessError;


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
