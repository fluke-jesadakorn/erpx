//! Actual Inventory stock-register reads over leased PostgreSQL. These cases
//! prove source scope, lot/count visibility and reconciliation only; they do
//! not prove IAM, Commercial purchase or an HTTP session.
#[path = "../../support/inventory/stock-register-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::stock_register::{
    lock_stock_register_sources, read_stock_register,
};
use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;

type Error = erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>;

#[tokio::test]
async fn register_reads_company_scope_lots_counts_and_reconciles_tracked_stock() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "company", None).await
                })
            })
            .await
            .unwrap();
        assert_eq!(result.currency, "THB");
        assert_eq!(result.stock.len(), 3);
        assert_eq!(result.lots.len(), 3);
        assert_eq!(result.counts.len(), 3);
        let main = result
            .stock
            .iter()
            .find(|row| row.warehouse_id == "warehouse" && row.product_id == "product")
            .unwrap();
        assert_eq!(main.quantity_milli, 3_000);
        assert_eq!(main.reserved_milli, 500);
        assert_eq!(main.quarantined_milli, 1_000);
        assert_eq!(main.available_milli, 2_000);
        assert_eq!(main.tracking_mode, "lot");
        assert_eq!(main.version, 3);
        let untracked = result
            .stock
            .iter()
            .find(|row| row.product_id == "untracked")
            .unwrap();
        assert_eq!(untracked.available_milli, 7_000);
        let held = result.lots.iter().find(|lot| lot.id == "lot-b").unwrap();
        assert_eq!(held.quality_hold_milli, 1_000);
        assert_eq!(held.quality_status.as_deref(), Some("held"));
        assert_eq!(held.quantity_milli, 1_000);
        assert_eq!(held.version, 1);
        let initial = result
            .counts
            .iter()
            .find(|count| count.id == "count-initial-command")
            .unwrap();
        assert!(initial.initial_physical_source);
        assert_eq!(initial.status, "posted");
        assert_eq!(initial.version, 4);
        let draft = result
            .counts
            .iter()
            .find(|count| count.id == "count-draft")
            .unwrap();
        assert!(!draft.initial_physical_source);
        assert_eq!(draft.version, 2);
        assert_eq!(result.totals.tracked_quantity_milli, 7_000);
        assert_eq!(result.totals.tracked_value_minor, 7_000);
        assert_eq!(result.totals.lot_quantity_milli, 7_000);
        assert_eq!(result.totals.lot_value_minor, 7_000);
        assert!(result.totals.consistent);
    })
    .await;
}

#[tokio::test]
async fn register_preserves_branch_and_company_isolation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let scoped = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "company", Some("branch-two")).await
                })
            })
            .await
            .unwrap();
        assert_eq!(scoped.stock.len(), 1);
        assert_eq!(scoped.stock[0].warehouse_id, "warehouse-two");
        assert!(
            scoped
                .lots
                .iter()
                .all(|lot| lot.warehouse_id == "warehouse-two")
        );
        assert_eq!(scoped.counts.len(), 1);
        assert_eq!(scoped.counts[0].id, "count-two");
        let foreign = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "another-company", None).await
                })
            })
            .await
            .unwrap();
        assert!(foreign.stock.is_empty());
        assert!(foreign.lots.is_empty());
        assert!(foreign.counts.is_empty());
        assert_eq!(foreign.currency, "THB");
    })
    .await;
}

#[tokio::test]
async fn register_requires_a_serializable_owner_transaction() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let result = read_stock_register(&mut c, "company", None).await;
        assert!(matches!(
            result,
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_))
        ));
    })
    .await;
}

#[tokio::test]
async fn register_rolls_back_with_the_enclosing_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let result: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    let register = read_stock_register(c, "company", None).await?;
                    assert_eq!(register.stock.len(), 3);
                    sqlx::query("UPDATE commerce_stock SET quantity_milli=quantity_milli+1 WHERE warehouse_id='warehouse' AND product_id='product'")
                        .execute(&mut *c)
                        .await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "FIXTURE_ABORT",
                        "Rollback after verifying the register read.",
                    )
                    .into())
                })
            })
            .await;
        assert!(matches!(
            result,
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(_))
        ));
        let quantity: i64 = sqlx::query_scalar(
            "SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(quantity, 3_000);
    })
    .await;
}
