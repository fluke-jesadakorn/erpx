//! Actual Inventory goods-receipt reads and effects over leased PostgreSQL.
//! Commerce fulfillment rows are synthetic source facts; Quality is a test port
//! implementing the owner boundary. These cases do not prove IAM, purchase,
//! complete Commerce command composition or HTTP behavior.
#![allow(dead_code)]
#[path = "../../support/inventory/stock-receipt-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::stock_receipt::*;
use erpx_server_inventory::public::stock_register::read_stock_register;
use erpx_server_runtime::public::postgres::{
    PgTransactionError, PgUnitOfWork, PgWorkFuture, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

struct TestQuality;

impl StockReceiptQualityPort for TestQuality {
    fn create_quality_hold<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: StockReceiptQualityRequest,
    ) -> PgWorkFuture<'a, String, BusinessError> {
        Box::pin(async move {
            let id = format!("quality-{}", request.allocation_id);
            sqlx::query(
                "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,\
                 product_id,reason,due_date,status,created_by,created_at,updated_at,lot_id,\
                 held_quantity_milli,source_type,source_id,quantity_scope) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,'held','inspector',0,0,$8,$9,'goods_receipt',$10,'partial')",
            )
            .bind(&id)
            .bind(&request.company_id)
            .bind(&request.branch_id)
            .bind(&request.warehouse_id)
            .bind(&request.product_id)
            .bind(&request.reason)
            .bind(&request.due_date)
            .bind(&request.lot_id)
            .bind(request.quantity_milli)
            .bind(&request.allocation_id)
            .execute(&mut *c)
            .await?;
            Ok(id)
        })
    }
}

fn allocation(
    lot: Option<&str>,
    serial: Option<&str>,
    quantity: i64,
    disposition: ReceiptDisposition,
) -> StockReceiptAllocationInput {
    StockReceiptAllocationInput {
        lot_code: lot.map(Into::into),
        serial_number: serial.map(Into::into),
        quantity_milli: quantity,
        disposition,
        quarantine_reason: (disposition == ReceiptDisposition::Quarantine)
            .then(|| "Packaging inspection".into()),
        quarantine_due_date: (disposition == ReceiptDisposition::Quarantine)
            .then(|| "2026-11-10".into()),
        manufactured_date: None,
        expiry_date: None,
    }
}

fn input(
    quantity: i64,
    value: i64,
    allocations: Vec<StockReceiptAllocationInput>,
) -> StockReceiptInput {
    StockReceiptInput {
        expected_order_version: 3,
        date: "2026-11-04".into(),
        lines: vec![StockReceiptLineInput {
            line_id: "line-1".into(),
            quantity_milli: quantity,
            stock_value_minor: value,
            allocations,
        }],
    }
}

fn denied<T: std::fmt::Debug>(result: Result<T, Error>, expected: &str) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, expected),
        other => panic!("expected {expected}, got {other:?}"),
    }
}

#[tokio::test]
async fn receipt_reads_current_purchase_source_and_persists_lots_and_quality() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source = read_stock_receipt_source(
                        c,
                        "company",
                        Some("branch"),
                        None,
                        "order-1",
                    )
                    .await?;
                    assert!(source.can_receive());
                    assert_eq!(source.order.number, "PO-1");
                    assert_eq!(source.lines.len(), 1);
                    assert_eq!(source.lines[0].remaining_milli, 5_000);
                    assert!(source.lines[0].stock.is_none());
                    assert!(source.lines[0].lots.is_empty());
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            2_000,
                            5_000,
                            vec![
                                allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                                allocation(Some("L-B"), None, 1_000, ReceiptDisposition::Quarantine),
                            ],
                        ),
                    )?;
                    facts::align_fulfillment(c, 2_000, 5_000).await;
                    let mut quality = TestQuality;
                    let applied = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert!(!applied.replayed);
                    assert_eq!(applied.lines.len(), 1);
                    assert_eq!(applied.lines[0].allocations.len(), 2);
                    assert_eq!(applied.lines[0].stock_version, 1);
                    assert!(applied.lines[0].allocations[0].quality_hold_id.is_none());
                    assert!(applied.lines[0].allocations[1].quality_hold_id.is_some());
                    let replay = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert!(replay.replayed);
                    assert_eq!(replay.lines[0].stock_version, 2);
                    let stock: (i64, i64) = sqlx::query_as(
                        "SELECT quantity_milli,value_minor FROM commerce_stock \
                         WHERE warehouse_id='warehouse' AND product_id='product'",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(stock, (2_000, 5_000));
                    let lots: Vec<(String, i64)> = sqlx::query_as(
                        "SELECT l.lot_code,b.quantity_milli FROM inventory_lots l \
                         JOIN inventory_lot_balances b ON b.lot_id=l.id \
                         WHERE l.company_id='company' AND l.product_id='product' ORDER BY l.lot_code",
                    )
                    .fetch_all(&mut *c)
                    .await?;
                    assert_eq!(
                        lots,
                        vec![("L-A".into(), 1_000), ("L-B".into(), 1_000)]
                    );
                    let evidence: (i64, i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT \
                         (SELECT count(*) FROM inventory_lot_moves WHERE source_type='goods_receipt'),\
                         (SELECT count(*) FROM inventory_fulfillment_lot_allocations),\
                         (SELECT count(*) FROM inventory_quality_holds WHERE source_type='goods_receipt'),\
                         (SELECT count(*) FROM commerce_stock_moves WHERE id='delivery-key-0'),\
                         (SELECT count(*) FROM commerce_stock_moves)",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (2, 2, 1, 1, 1));
                    let register = read_stock_register(c, "company", None).await?;
                    assert_eq!(register.currency, "THB");
                    let row = register
                        .stock
                        .iter()
                        .find(|row| {
                            row.warehouse_id == "warehouse" && row.product_id == "product"
                        })
                        .unwrap();
                    assert_eq!(row.quantity_milli, 2_000);
                    assert_eq!(row.value_minor, 5_000);
                    assert_eq!(row.quarantined_milli, 1_000);
                    assert_eq!(row.available_milli, 1_000);
                    assert_eq!(register.lots.len(), 2);
                    assert_eq!(register.totals.tracked_quantity_milli, 2_000);
                    assert_eq!(register.totals.tracked_value_minor, 5_000);
                    assert_eq!(register.totals.lot_quantity_milli, 2_000);
                    assert_eq!(register.totals.lot_value_minor, 5_000);
                    assert!(register.totals.consistent);
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_refuses_stale_stock_and_a_quarantined_available_allocation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", true).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    assert_eq!(source.lines[0].stock.as_ref().unwrap().version, 1);
                    assert_eq!(source.lines[0].lots.len(), 1);
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            1_000,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 1_000).await;
                    sqlx::query("UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'")
                        .execute(&mut *c)
                        .await?;
                    let mut quality = TestQuality;
                    let stale = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await;
                    denied(stale, "STOCK_RECEIPT_SOURCE_CHANGED");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_rechecks_a_quality_hold_created_after_preparation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", true).await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let mut plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            1_000,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    sqlx::query(
                        "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,\
                         product_id,reason,due_date,status,created_by,created_at,updated_at,lot_id,\
                         held_quantity_milli,source_type,source_id,quantity_scope) \
                         VALUES('existing-hold','company','branch','warehouse','product','Hold',\
                         '2026-11-10','held','inspector',0,0,'lot-existing',1000,'manual','hold-1','partial')",
                    )
                    .execute(&mut *c)
                    .await?;
                    let version: i64 = sqlx::query_scalar(
                        "SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    plan.lines[0].expected_stock_version = Some(version);
                    facts::align_fulfillment(c, 1_000, 1_000).await;
                    let mut quality = TestQuality;
                    let refused = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await;
                    refused.map(|_| ())
                })
            })
            .await;
        denied(result, "LOT_UNAVAILABLE");
        let evidence: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
             (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
             (SELECT count(*) FROM inventory_lot_moves),\
             (SELECT count(*) FROM inventory_fulfillment_lot_allocations),\
             (SELECT count(*) FROM inventory_quality_holds WHERE id='existing-hold')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (1_000, 0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn receipt_persists_serials_and_rejects_a_duplicate_serial() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "serial", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            2_000,
                            5_000,
                            vec![
                                allocation(None, Some("SN-1"), 1_000, ReceiptDisposition::Available),
                                allocation(None, Some("SN-2"), 1_000, ReceiptDisposition::Available),
                            ],
                        ),
                    )?;
                    facts::align_fulfillment(c, 2_000, 5_000).await;
                    let mut quality = TestQuality;
                    let applied = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert_eq!(applied.lines[0].allocations.len(), 2);
                    let serials: Vec<(String, i64)> = sqlx::query_as(
                        "SELECT l.serial_number,b.quantity_milli FROM inventory_lots l \
                         JOIN inventory_lot_balances b ON b.lot_id=l.id \
                         WHERE l.company_id='company' AND l.product_id='product' ORDER BY l.serial_number",
                    )
                    .fetch_all(&mut *c)
                    .await?;
                    assert_eq!(
                        serials,
                        vec![("SN-1".into(), 1_000), ("SN-2".into(), 1_000)]
                    );
                    let refreshed =
                        read_stock_receipt_source(c, "company", None, None, "order-1")
                            .await?;
                    let duplicate = prepare_stock_receipt(
                        &refreshed,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                None,
                                Some("SN-1"),
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    );
                    match duplicate {
                        Err(error) => assert_eq!(error.code, "SERIAL_ALREADY_RECEIVED"),
                        other => panic!("expected SERIAL_ALREADY_RECEIVED, got {other:?}"),
                    }
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_persists_untracked_stock_without_lot_evidence() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan =
                        prepare_stock_receipt(&source, &input(2_000, 5_000, vec![]))?;
                    facts::align_fulfillment(c, 2_000, 5_000).await;
                    let mut quality = TestQuality;
                    let applied = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert_eq!(applied.lines[0].allocations.len(), 0);
                    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT \
                         (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT count(*) FROM inventory_lots WHERE product_id='product'),\
                         (SELECT count(*) FROM inventory_lot_moves),\
                         (SELECT count(*) FROM commerce_stock_moves WHERE id='delivery-key-0')",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (2_000, 0, 0, 1));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_rolls_back_with_the_enclosing_commerce_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        let result: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Quarantine,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;
                    apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "FIXTURE_ABORT",
                        "Roll back after verifying Inventory effects.",
                    )
                    .into())
                })
            })
            .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(_))
        ));
        let evidence: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
             (SELECT count(*) FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
             (SELECT count(*) FROM inventory_lots WHERE product_id='product'),\
             (SELECT count(*) FROM inventory_lot_balances WHERE warehouse_id='warehouse'),\
             (SELECT count(*) FROM inventory_lot_moves),\
             (SELECT count(*) FROM inventory_fulfillment_lot_allocations),\
             (SELECT count(*) FROM commerce_stock_moves)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (0, 0, 0, 0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn receipt_source_excludes_service_lines_and_rejects_tampered_scope() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", Some("branch"), None, "order-1")
                            .await?;
                    assert!(source.can_receive());
                    assert_eq!(source.lines.len(), 1);
                    assert_eq!(source.lines[0].id, "line-1");
                    assert!(source.lines.iter().all(|line| line.id != "line-service"));

                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;

                    let mut branch = plan.clone();
                    branch.branch_id = "branch-two".into();
                    denied(
                        apply_stock_receipt(
                            c,
                            &branch,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "STOCK_RECEIPT_SCOPE_CHANGED",
                    );

                    let mut warehouse = plan.clone();
                    warehouse.warehouse_id = "warehouse-two".into();
                    denied(
                        apply_stock_receipt(
                            c,
                            &warehouse,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "STOCK_RECEIPT_SCOPE_CHANGED",
                    );

                    let mut company = plan;
                    company.company_id = "other-company".into();
                    denied(
                        apply_stock_receipt(
                            c,
                            &company,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "ORDER_NOT_FOUND",
                    );

                    let evidence: (i64, i64, i64) = sqlx::query_as(
                        "SELECT (SELECT count(*) FROM commerce_stock),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT count(*) FROM inventory_lot_moves)",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (0, 0, 0));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_rejects_over_receipt_against_the_current_order_line() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;
                    apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;

                    sqlx::query(
                        "UPDATE commerce_order_lines SET fulfilled_milli=4000 WHERE id='line-1'",
                    )
                    .execute(&mut *c)
                    .await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let mut over = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    over.lines[0].quantity_milli = 2_000;
                    over.lines[0].stock_value_minor = 5_000;
                    over.lines[0].allocations[0].quantity_milli = 2_000;
                    over.lines[0].allocations[0].value_minor = 5_000;
                    over.total_value_minor = 5_000;
                    sqlx::raw_sql(
                        "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version)VALUES\
                         ('delivery-over','order-1','GR-OVER','2026-11-04','maker',0,1);\
                         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor)VALUES\
                         ('delivery-over-0','delivery-over','line-1',2000,5000,5000,0);",
                    )
                    .execute(&mut *c)
                    .await?;
                    denied(
                        apply_stock_receipt(
                            c,
                            &over,
                            "delivery-over",
                            "command-key-over",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "OVER_FULFILLMENT",
                    );
                    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT fulfilled_milli FROM commerce_order_lines WHERE id='line-1'),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT count(*) FROM commerce_stock_moves WHERE id='delivery-over-0')",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (1_000, 4_000, 1, 0));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_rejects_a_changed_replay_without_duplicating_evidence() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;
                    let applied = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert!(!applied.replayed);

                    let mut changed = plan.clone();
                    changed.lines[0].stock_value_minor = 3_000;
                    changed.lines[0].allocations[0].value_minor = 3_000;
                    changed.total_value_minor = 3_000;
                    denied(
                        apply_stock_receipt(
                            c,
                            &changed,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "STOCK_RECEIPT_REPLAY_MISMATCH",
                    );
                    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT count(*) FROM inventory_fulfillment_lot_allocations)",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (1_000, 2_500, 1, 1));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_refuses_an_inactive_existing_lot_and_rolls_back_stock() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", true).await;
        sqlx::query("UPDATE inventory_lots SET status='scrapped' WHERE id='lot-existing'")
            .execute(pool.raw())
            .await
            .unwrap();
        let result: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;
                    denied(
                        apply_stock_receipt(
                            c,
                            &plan,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "LOT_UNAVAILABLE",
                    );
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "FIXTURE_ABORT",
                        "Roll back after verifying the inactive lot refusal.",
                    )
                    .into())
                })
            })
            .await;
        assert!(matches!(result, Err(PgTransactionError::Business(_))));
        let evidence: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM inventory_lot_moves),\
             (SELECT (CASE WHEN status='scrapped' THEN 1 ELSE 0 END)::bigint FROM inventory_lots WHERE id='lot-existing')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (1_000, 0, 0, 1));
    })
    .await;
}

#[tokio::test]
async fn receipt_source_is_company_and_branch_scoped() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    denied(
                        read_stock_receipt_source(c, "other-company", None, None, "order-1").await,
                        "ORDER_NOT_FOUND",
                    );
                    denied(
                        read_stock_receipt_source(
                            c,
                            "company",
                            Some("branch-two"),
                            None,
                            "order-1",
                        )
                        .await,
                        "ORDER_NOT_FOUND",
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
