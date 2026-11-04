//! Actual Inventory sales-delivery reads and effects over leased PostgreSQL.
//! Commerce fulfillment rows are synthetic source facts; these cases do not
//! prove IAM, purchase, complete Commerce command composition or HTTP behavior.
#![allow(dead_code)]
#[path = "../../support/inventory/stock-issue-facts.rs"]
mod facts;
#[path = "../../support/inventory/reservation-source-fence.rs"]
mod fence;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::stock_issue::*;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;
use sqlx::AssertSqlSafe;

type Error = PgTransactionError<BusinessError>;

fn allocation(lot_id: &str, quantity_milli: i64) -> StockIssueAllocationInput {
    StockIssueAllocationInput {
        lot_id: lot_id.into(),
        quantity_milli,
    }
}

fn input(quantity_milli: i64, allocations: Vec<StockIssueAllocationInput>) -> StockIssueInput {
    StockIssueInput {
        expected_order_version: 4,
        date: "2026-11-04".into(),
        lines: vec![StockIssueLineInput {
            line_id: "line-1".into(),
            quantity_milli,
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
async fn issue_reads_current_sales_source_and_persists_lots_and_reservations() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", Some("branch"), None, "order-1")
                            .await?;
                    assert!(source.can_issue());
                    assert_eq!(source.order.number, "SO-1");
                    assert_eq!(source.lines.len(), 1);
                    assert_eq!(source.lines[0].id, "line-1");
                    assert_eq!(source.lines[0].remaining_milli, 4_000);
                    let stock = source.lines[0].stock.as_ref().unwrap();
                    assert_eq!(stock.quantity_milli, 4_000);
                    assert_eq!(stock.value_minor, 2_000);
                    assert_eq!(stock.version, 2);
                    assert_eq!(stock.reserved_milli, 2_000);
                    assert_eq!(stock.quarantined_milli, 0);
                    assert_eq!(source.lines[0].lots.len(), 2);
                    assert_eq!(source.lines[0].reservations.len(), 1);
                    assert!(source.lines[0].reservations[0].active);
                    assert_eq!(source.lines[0].reservations[0].remaining_milli(), 2_000);
                    let plan = prepare_stock_issue(
                        &source,
                        &input(
                            2_000,
                            vec![allocation("lot-a", 1_000), allocation("lot-b", 1_000)],
                        ),
                    )?;
                    let mut plan = plan;
                    plan.lines[0].allocations[0].lot_code = Some("FORGED".into());
                    assert_eq!(plan.total_value_minor, 1_000);
                    assert_eq!(plan.lines[0].expected_stock_version, 2);
                    facts::align_fulfillment(c, 2_000, 1_000).await;
                    let applied =
                        apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                            .await?;
                    assert!(!applied.replayed);
                    assert_eq!(applied.lines.len(), 1);
                    assert_eq!(applied.lines[0].quantity_milli, 2_000);
                    assert_eq!(applied.lines[0].value_minor, 1_000);
                    assert_eq!(applied.lines[0].reservations_consumed_milli, 2_000);
                    assert_eq!(applied.lines[0].allocations.len(), 2);
                    assert_eq!(
                        applied.lines[0].allocations[0].lot_code.as_deref(),
                        Some("L-A")
                    );
                    assert_eq!(applied.lines[0].stock_version, 4);
                    let replay =
                        apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                            .await?;
                    assert!(replay.replayed);
                    assert_eq!(replay.lines[0].quantity_milli, 2_000);
                    assert_eq!(replay.lines[0].value_minor, 1_000);
                    assert_eq!(replay.lines[0].allocations.len(), 2);
                    let stock: (i64, i64, i64) = sqlx::query_as(
                        "SELECT quantity_milli,value_minor,version FROM commerce_stock \
                         WHERE warehouse_id='warehouse' AND product_id='product'",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(stock, (2_000, 1_000, 4));
                    let reservation: (i64, i64) = sqlx::query_as(
                        "SELECT consumed_milli,released_milli FROM inventory_order_reservations \
                         WHERE id='reservation-1'",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(reservation, (2_000, 0));
                    let lots: Vec<(String, i64, i64, String, i64)> = sqlx::query_as(
                        "SELECT l.lot_code,b.quantity_milli,b.value_minor,l.status,b.version \
                         FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id \
                         WHERE l.company_id='company' AND l.product_id='product' \
                         ORDER BY l.lot_code",
                    )
                    .fetch_all(&mut *c)
                    .await?;
                    assert_eq!(
                        lots,
                        vec![
                            ("L-A".into(), 0, 0, "depleted".into(), 2),
                            ("L-B".into(), 2_000, 1_000, "active".into(), 2),
                        ]
                    );
                    let evidence: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT \
                         (SELECT count(*) FROM inventory_lot_moves WHERE source_type='delivery'),\
                         (SELECT count(*) FROM inventory_fulfillment_lot_allocations),\
                         (SELECT count(*) FROM commerce_stock_moves WHERE id='delivery-key-0'),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT sum(quantity_milli)::bigint FROM inventory_lot_moves),\
                         (SELECT sum(value_minor)::bigint FROM inventory_lot_moves)",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (2, 2, 1, 1, -2_000, -1_000));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn issue_delivers_untracked_stock_without_lot_evidence() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source = read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_issue(&source, &input(2_000, vec![]))?;
                    assert_eq!(plan.total_value_minor, 1_000);
                    assert!(plan.lines[0].allocations.is_empty());
                    facts::align_fulfillment(c, 2_000, 1_000).await;
                    let applied = apply_stock_issue(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "issuer",
                        100,
                    )
                    .await?;
                    assert!(!applied.replayed);
                    assert_eq!(applied.lines[0].reservations_consumed_milli, 2_000);
                    assert!(applied.lines[0].allocations.is_empty());
                    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT \
                         (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT count(*) FROM inventory_lot_moves),\
                         (SELECT count(*) FROM inventory_fulfillment_lot_allocations)",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (2_000, 1_000, 0, 0));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn issue_delivers_whole_serials_and_depletes_them() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "serial").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_issue(
                        &source,
                        &input(
                            2_000,
                            vec![allocation("serial-a", 1_000), allocation("serial-b", 1_000)],
                        ),
                    )?;
                    facts::align_fulfillment(c, 2_000, 1_000).await;
                    let applied =
                        apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                            .await?;
                    assert_eq!(applied.lines[0].allocations.len(), 2);
                    assert_eq!(
                        applied.lines[0].allocations[0].serial_number.as_deref(),
                        Some("SN-A")
                    );
                    let lots: Vec<(String, i64, i64, String)> = sqlx::query_as(
                        "SELECT l.serial_number,b.quantity_milli,b.value_minor,l.status \
                         FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id \
                         WHERE l.company_id='company' AND l.product_id='product' \
                         ORDER BY l.serial_number",
                    )
                    .fetch_all(&mut *c)
                    .await?;
                    assert_eq!(
                        lots,
                        vec![
                            ("SN-A".into(), 0, 0, "depleted".into()),
                            ("SN-B".into(), 0, 0, "depleted".into()),
                            ("SN-C".into(), 1_000, 500, "active".into()),
                            ("SN-D".into(), 1_000, 500, "active".into()),
                        ]
                    );
                    let stock: (i64, i64) = sqlx::query_as(
                        "SELECT quantity_milli,value_minor FROM commerce_stock \
                         WHERE warehouse_id='warehouse' AND product_id='product'",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(stock, (2_000, 1_000));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn issue_consumes_only_its_own_active_sales_reservation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    sqlx::raw_sql(
                        "INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at)VALUES\
                         ('order-2','company','branch','sales','SO-2','customer','warehouse','THB','2026-11-01','2026-11-10',2500,0,'confirmed',1,'maker',0,0);\
                         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type)VALUES\
                         ('line-2','order-2','product','Other delivery',1000,0,2500,0,2500,0,1,'stock');\
                         INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)VALUES\
                         ('reservation-2','line-2','warehouse','product',1000,0,0,9007199254740991,'maker','Other reserve',0);",
                    )
                    .execute(&mut *c)
                    .await?;
                    let source = read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    assert_eq!(source.lines[0].stock.as_ref().unwrap().version, 3);
                    assert_eq!(source.lines[0].stock.as_ref().unwrap().reserved_milli, 3_000);
                    let plan =
                        prepare_stock_issue(&source, &input(3_000, vec![]))?;
                    assert_eq!(plan.total_value_minor, 1_500);
                    facts::align_fulfillment(c, 3_000, 1_500).await;
                    let applied = apply_stock_issue(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "issuer",
                        100,
                    )
                    .await?;
                    assert_eq!(applied.lines[0].reservations_consumed_milli, 2_000);
                    let reservations: Vec<(String, i64, i64)> = sqlx::query_as(
                        "SELECT id,consumed_milli,released_milli FROM inventory_order_reservations \
                         ORDER BY id",
                    )
                    .fetch_all(&mut *c)
                    .await?;
                    assert_eq!(
                        reservations,
                        vec![
                            ("reservation-1".into(), 2_000, 0),
                            ("reservation-2".into(), 0, 0),
                        ]
                    );
                    let stock: (i64, i64) = sqlx::query_as(
                        "SELECT quantity_milli,value_minor FROM commerce_stock \
                         WHERE warehouse_id='warehouse' AND product_id='product'",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(stock, (1_000, 500));
                    Ok(())
                })
            })
            .await
            .unwrap();
        let stock: (i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,value_minor FROM commerce_stock \
             WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stock, (1_000, 500));
    })
    .await;
}

#[tokio::test]
async fn issue_rejects_a_stale_stock_or_reservation_source() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_issue(&source, &input(1_000, vec![]))?;
                    facts::align_fulfillment(c, 1_000, 500).await;
                    sqlx::query(
                        "UPDATE commerce_stock SET version=version+1 \
                         WHERE warehouse_id='warehouse' AND product_id='product'",
                    )
                    .execute(&mut *c)
                    .await?;
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100).await
                })
            })
            .await;
        denied(result, "STOCK_ISSUE_SOURCE_CHANGED");

        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let mut plan = prepare_stock_issue(&source, &input(1_000, vec![]))?;
                    facts::align_fulfillment(c, 1_000, 500).await;
                    plan.lines[0].reservations[0].quantity_milli += 100;
                    plan.lines[0].reservations[0].remaining_milli += 100;
                    assert!(validate_stock_issue_plan(&plan).is_ok());
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100).await
                })
            })
            .await;
        denied(result, "STOCK_ISSUE_SOURCE_CHANGED");
        let stock: (i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,value_minor FROM commerce_stock \
             WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stock, (4_000, 2_000));
    })
    .await;
}

#[tokio::test]
async fn issue_rejects_a_changed_replay_without_duplicating_evidence() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_issue(&source, &input(2_000, vec![]))?;
                    facts::align_fulfillment(c, 2_000, 1_000).await;
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                        .await?;
                    let changed = prepare_stock_issue(&source, &input(1_000, vec![]))?;
                    assert_eq!(changed.total_value_minor, 500);
                    facts::align_fulfillment(c, 1_000, 500).await;
                    denied(
                        apply_stock_issue(
                            c,
                            &changed,
                            "delivery-key",
                            "command-key-2",
                            "issuer",
                            100,
                        )
                        .await,
                        "STOCK_ISSUE_REPLAY_MISMATCH",
                    );
                    let evidence: (i64, i64, i64) = sqlx::query_as(
                        "SELECT (SELECT quantity_milli FROM commerce_stock \
                           WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT consumed_milli FROM inventory_order_reservations \
                           WHERE id='reservation-1')",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (2_000, 1, 2_000));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn issue_replays_after_the_commerce_order_update() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_issue(&source, &input(2_000, vec![]))?;
                    facts::align_fulfillment(c, 2_000, 1_000).await;
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                        .await?;
                    sqlx::raw_sql(
                        "UPDATE commerce_order_lines SET fulfilled_milli=2000 WHERE id='line-1';\
                         UPDATE commerce_orders SET state='partially_fulfilled',version=5 \
                         WHERE id='order-1';",
                    )
                    .execute(&mut *c)
                    .await?;
                    let replay =
                        apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                            .await?;
                    assert!(replay.replayed);
                    assert_eq!(replay.lines[0].quantity_milli, 2_000);
                    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT (SELECT quantity_milli FROM commerce_stock \
                           WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT fulfilled_milli FROM commerce_order_lines WHERE id='line-1'),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT consumed_milli FROM inventory_order_reservations \
                           WHERE id='reservation-1')",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (2_000, 2_000, 1, 2_000));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn issue_rejects_over_fulfillment_and_a_changed_order_version() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    prepare_stock_issue(&source, &input(4_001, vec![]))?;
                    Ok(())
                })
            })
            .await;
        denied(result, "OVER_FULFILLMENT");

        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_issue(&source, &input(1_000, vec![]))?;
                    facts::align_fulfillment(c, 1_000, 500).await;
                    sqlx::query("UPDATE commerce_orders SET version=5 WHERE id='order-1'")
                        .execute(&mut *c)
                        .await?;
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100).await
                })
            })
            .await;
        denied(result, "STOCK_ISSUE_SOURCE_CHANGED");

        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_issue(&source, &input(1_000, vec![]))?;
                    facts::align_fulfillment(c, 1_000, 500).await;
                    sqlx::query(
                        "UPDATE commerce_order_lines SET fulfilled_milli=3000 WHERE id='line-1'",
                    )
                    .execute(&mut *c)
                    .await?;
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100).await
                })
            })
            .await;
        denied(result, "STOCK_ISSUE_SOURCE_CHANGED");
    })
    .await;
}

#[tokio::test]
async fn issue_rejects_a_tampered_value_at_the_write_boundary() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot").await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let mut plan = prepare_stock_issue(
                        &source,
                        &input(
                            2_000,
                            vec![allocation("lot-a", 1_000), allocation("lot-b", 1_000)],
                        ),
                    )?;
                    plan.lines[0].allocations[0].value_minor = 400;
                    plan.lines[0].stock_value_minor = 900;
                    plan.total_value_minor = 900;
                    assert!(validate_stock_issue_plan(&plan).is_ok());
                    facts::align_fulfillment(c, 2_000, 900).await;
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100).await
                })
            })
            .await;
        denied(result, "STOCK_ISSUE_VALUE_INVALID");
    })
    .await;

    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let mut plan = prepare_stock_issue(&source, &input(2_000, vec![]))?;
                    plan.lines[0].stock_value_minor = 900;
                    plan.total_value_minor = 900;
                    assert!(validate_stock_issue_plan(&plan).is_ok());
                    facts::align_fulfillment(c, 2_000, 900).await;
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100).await
                })
            })
            .await;
        denied(result, "STOCK_ISSUE_VALUE_INVALID");
    })
    .await;
}

#[tokio::test]
async fn issue_preserves_company_and_branch_isolation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    denied(
                        read_stock_issue_source(c, "other-company", None, None, "order-1").await,
                        "ORDER_NOT_FOUND",
                    );
                    denied(
                        read_stock_issue_source(c, "company", Some("branch-two"), None, "order-1")
                            .await,
                        "ORDER_NOT_FOUND",
                    );
                    denied(
                        read_stock_issue_source(c, "company", None, Some("department"), "order-1")
                            .await,
                        "ORDER_NOT_FOUND",
                    );
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let mut plan = prepare_stock_issue(&source, &input(1_000, vec![]))?;
                    facts::align_fulfillment(c, 1_000, 500).await;
                    plan.company_id = "other-company".into();
                    denied(
                        apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                            .await,
                        "ORDER_NOT_FOUND",
                    );
                    plan.company_id = "company".into();
                    plan.branch_id = "branch-two".into();
                    denied(
                        apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                            .await,
                        "STOCK_ISSUE_SCOPE_CHANGED",
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn issue_refuses_whole_quarantined_stock() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    sqlx::query(
                        "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,\
                         product_id,reason,due_date,status,created_by,created_at,updated_at,\
                         held_quantity_milli,source_type,source_id,quantity_scope) \
                         VALUES('whole-hold','company','branch','warehouse','product','Hold',\
                         '2026-11-10','held','inspector',0,0,NULL,'manual','hold-1','whole')",
                    )
                    .execute(&mut *c)
                    .await?;
                    let source =
                        read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    assert_eq!(
                        source.lines[0].stock.as_ref().unwrap().quarantined_milli,
                        4_000
                    );
                    prepare_stock_issue(&source, &input(1_000, vec![]))?;
                    Ok(())
                })
            })
            .await;
        denied(result, "INSUFFICIENT_STOCK");
    })
    .await;
}

#[tokio::test]
async fn issue_refuses_a_quarantined_lot() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot").await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    sqlx::query(
                        "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,\
                         product_id,reason,due_date,status,created_by,created_at,updated_at,\
                         lot_id,held_quantity_milli,source_type,source_id,quantity_scope) \
                         VALUES('lot-hold','company','branch','warehouse','product','Hold',\
                         '2026-11-10','held','inspector',0,0,'lot-a',1000,'manual','hold-2','partial')",
                    )
                    .execute(&mut *c)
                    .await?;
                    let source = read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    prepare_stock_issue(
                        &source,
                        &input(3_000, vec![allocation("lot-a", 1_000), allocation("lot-b", 2_000)]),
                    )?;
                    Ok(())
                })
            })
            .await;
        denied(result, "LOT_UNAVAILABLE");
    })
    .await;
}

#[tokio::test]
async fn issue_fences_every_reservation_source_and_stock_writers() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot").await;
        let mut reader = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *reader)
            .await
            .unwrap();
        lock_stock_issue_sources(&mut reader).await.unwrap();
        fence::assert_reservation_source_writes_wait(&pool).await;
        for statement in [
            "UPDATE commerce_stock SET quantity_milli=quantity_milli WHERE warehouse_id='warehouse' AND product_id='product'",
            "UPDATE inventory_lot_balances SET quantity_milli=quantity_milli WHERE lot_id='lot-a'",
        ] {
            let mut writer = pool.raw().begin().await.unwrap();
            sqlx::query("SET LOCAL lock_timeout='150ms'")
                .execute(&mut *writer)
                .await
                .unwrap();
            let error = sqlx::query(AssertSqlSafe(statement))
                .execute(&mut *writer)
                .await
                .expect_err("the Inventory issue fence must block stock and lot writers");
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|value| value.code())
                    .as_deref(),
                Some("55P03"),
            );
            writer.rollback().await.unwrap();
        }
        reader.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn issue_requires_a_serializable_owner_transaction() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        let mut c = pool.raw().acquire().await.unwrap();
        assert!(matches!(
            read_stock_issue_source(&mut c, "company", None, None, "order-1").await,
            Err(PgTransactionError::Persistence(_))
        ));
        drop(c);
        let mut c = pool.raw().begin().await.unwrap();
        assert!(matches!(
            lock_stock_issue_sources(&mut c).await,
            Err(PgTransactionError::Persistence(_))
        ));
        c.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn issue_rolls_back_with_the_enclosing_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot").await;
        let result: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_issue_sources(c).await?;
                    let source = read_stock_issue_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_issue(
                        &source,
                        &input(
                            2_000,
                            vec![allocation("lot-a", 1_000), allocation("lot-b", 1_000)],
                        ),
                    )?;
                    facts::align_fulfillment(c, 2_000, 1_000).await;
                    apply_stock_issue(c, &plan, "delivery-key", "command-key", "issuer", 100)
                        .await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "FIXTURE_ABORT",
                        "Rollback after verifying the delivery effects.",
                    )
                    .into())
                })
            })
            .await;
        denied(result, "FIXTURE_ABORT");
        let evidence: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
             (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
             (SELECT consumed_milli FROM inventory_order_reservations WHERE id='reservation-1'),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM inventory_lot_moves)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (4_000, 0, 0, 0));
    })
    .await;
}
