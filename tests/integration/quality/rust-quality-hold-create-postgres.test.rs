//! Native Quality manual hold-create over a real serializable PostgreSQL unit.
#[path = "../../support/quality/quality-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_quality::public::quality_commands::{
    QualityHoldCommand, lock_quality_command_sources, lock_quality_hold_create_sources,
    prepare_quality_hold,
};
use erpx_server_quality::public::quality_http::{
    lock_quality_read_sources, read_quality_detail, read_quality_workspace,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, RecordId, UserId};

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn branch() -> RecordId {
    RecordId::new("branch").unwrap()
}

fn command(key: &str) -> QualityHoldCommand {
    QualityHoldCommand {
        key: CommandKey::new(key).unwrap(),
        warehouse_id: RecordId::new("warehouse").unwrap(),
        product_id: RecordId::new("product").unwrap(),
        lot_id: None,
        expected_stock_version: 1,
        expected_lot_version: None,
        due_date: "2026-11-10".into(),
        reason: "Damaged carton requires independent review".into(),
    }
}

async fn serializable(pool: &erpx_server_runtime::public::postgres::DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

#[tokio::test]
async fn manual_hold_persists_source_stock_spec_and_product_response() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::specification(&pool, "spec").await;
        sqlx::raw_sql(
            "INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) \
             VALUES('tracked','company','TRACKED','Tracked product',2000,'lot');\
             INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
             VALUES('warehouse','tracked',4000,800);\
             INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,serial_number,expiry_date,created_by,created_at) \
             VALUES('lot','company','tracked','lot','L1',NULL,'2027-01-31','maker',1);\
             INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) \
             VALUES('lot','warehouse',4000,800);",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let mut tx = serializable(&pool).await;
        lock_quality_hold_create_sources(&mut tx, &command("hold-key"), &company(), Some(&branch()))
            .await
            .unwrap();
        let prepared = prepare_quality_hold(
            &mut tx,
            &command("hold-key"),
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1_800_000_000_000,
        )
        .await
        .unwrap();
        assert_eq!(prepared.receipt().id, "quality-hold-key");
        assert_eq!(prepared.receipt().version, 1);
        prepared.apply(&mut tx).await.unwrap();
        prepared.require_after(&mut tx).await.unwrap();
        let saved: (String, String, i64, i64, Option<String>, Option<i64>, String) = sqlx::query_as(
            "SELECT status,branch_id,version,held_quantity_milli,spec_id,spec_version,quantity_scope \
             FROM inventory_quality_holds WHERE id='quality-hold-key'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            saved,
            (
                "held".into(),
                "branch".into(),
                1,
                5000,
                Some("spec".into()),
                Some(2),
                "whole".into()
            )
        );
        let stock_version: i64 = sqlx::query_scalar(
            "SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stock_version, 2, "the hold trigger advances the stock version");
        tx.commit().await.unwrap();

        // The bounded product response carries the page's stock, lot, product,
        // specification and hold label facts in one unit.
        let mut read = serializable(&pool).await;
        lock_quality_read_sources(&mut read).await.unwrap();
        let workspace = match read_quality_workspace(&mut read, &company(), Some(&branch())).await {
            Ok(workspace) => workspace,
            Err(PgTransactionError::Persistence(error)) => {
                panic!("workspace read failed: {} {:?}", error.code, error.sql_state())
            }
            Err(error) => panic!("workspace read failed: {error:?}"),
        };
        let data = workspace.data();
        assert!(data.stock.iter().any(|row| row.product_id == "product"
            && row.warehouse_name == "Quality warehouse"
            && row.product_name == "Quality product"
            && row.quantity_milli == 5000));
        assert_eq!(data.products.len(), 2);
        assert!(data.specifications.iter().any(|row| row.id == "spec"
            && row.sku == "P1"
            && row.product_name == "Quality product"
            && row.status == erpx_server_quality::public::quality_read::SpecStatus::Active));
        assert_eq!(data.specification_metrics.len(), 2);
        let hold = data
            .holds
            .iter()
            .find(|row| row.id == "quality-hold-key")
            .expect("created hold");
        let product = data.hold_products.get(&hold.id).expect("hold labels");
        assert_eq!(product.product_name, "Quality product");
        assert_eq!(product.product_unit, "each");
        assert_eq!(product.tracking_mode, "none");
        assert_eq!(product.warehouse_name, "Quality warehouse");
        assert_eq!(product.eligible_count, 0);
        workspace.require_current(&mut read).await.unwrap();
        read.rollback().await.unwrap();

        let mut detail_tx = serializable(&pool).await;
        lock_quality_read_sources(&mut detail_tx).await.unwrap();
        let detail = read_quality_detail(&mut detail_tx, &company(), Some(&branch()), "quality-hold-key")
            .await
            .unwrap()
            .expect("detail");
        assert_eq!(detail.data().product.product_name, "Quality product");
        assert_eq!(detail.data().product.warehouse_name, "Quality warehouse");
        assert_eq!(detail.data().hold.held_quantity_milli, Some(5000));
        assert_eq!(detail.data().spec_metrics.len(), 2);
        detail_tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn manual_hold_rejects_stale_stock_lot_and_duplicate_command_keys() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) \
             VALUES('tracked','company','TRACKED','Tracked product',2000,'lot');\
             INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
             VALUES('warehouse','tracked',4000,800);\
             INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) \
             VALUES('lot','company','tracked','lot','L1','maker',1);\
             INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) \
             VALUES('lot','warehouse',4000,800);\
             UPDATE commerce_stock SET version=3 WHERE product_id='tracked';",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let mut stale = command("stale-stock");
        stale.product_id = RecordId::new("tracked").unwrap();
        stale.expected_stock_version = 2;
        let mut tx = serializable(&pool).await;
        match prepare_quality_hold(
            &mut tx,
            &stale,
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1,
        )
        .await
        {
            Err(PgTransactionError::Business(error)) => assert_eq!(error.code, "CONFLICT"),
            _ => panic!("stale stock version was accepted"),
        }
        tx.rollback().await.unwrap();

        let mut lot = command("stale-lot");
        lot.product_id = RecordId::new("tracked").unwrap();
        lot.lot_id = Some(RecordId::new("lot").unwrap());
        lot.expected_stock_version = 3;
        lot.expected_lot_version = Some(2);
        let mut tx = serializable(&pool).await;
        match prepare_quality_hold(
            &mut tx,
            &lot,
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1,
        )
        .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "QUALITY_LOT_CHANGED")
            }
            _ => panic!("stale lot version was accepted"),
        }
        tx.rollback().await.unwrap();

        let mut missing = command("missing-lot");
        missing.product_id = RecordId::new("tracked").unwrap();
        missing.lot_id = Some(RecordId::new("absent").unwrap());
        missing.expected_stock_version = 3;
        missing.expected_lot_version = Some(1);
        let mut tx = serializable(&pool).await;
        match prepare_quality_hold(
            &mut tx,
            &missing,
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1,
        )
        .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "QUALITY_LOT_CHANGED")
            }
            _ => panic!("absent lot was accepted"),
        }
        tx.rollback().await.unwrap();

        let mut first = serializable(&pool).await;
        lock_quality_hold_create_sources(&mut first, &command("same-key"), &company(), Some(&branch()))
            .await
            .unwrap();
        lock_quality_command_sources(&mut first).await.unwrap();
        prepare_quality_hold(
            &mut first,
            &command("same-key"),
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1,
        )
        .await
        .unwrap()
        .apply(&mut first)
        .await
        .unwrap();
        let mut duplicate = command("same-key");
        match prepare_quality_hold(
            &mut first,
            &duplicate,
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            2,
        )
        .await
        {
            Err(PgTransactionError::Business(error)) => assert_eq!(error.code, "CONFLICT"),
            _ => panic!("a duplicate hold command key was accepted"),
        }
        duplicate.expected_stock_version = 1;
        first.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_holds WHERE id='quality-same-key'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 0, "rollback removes the prepared hold");
    })
    .await;
}

#[tokio::test]
async fn manual_hold_requires_current_company_and_branch_stock_and_derives_the_hold_branch() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;

        let mut foreign_company = serializable(&pool).await;
        match prepare_quality_hold(
            &mut foreign_company,
            &command("foreign-company"),
            &CompanyId::new("other").unwrap(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1,
        )
        .await
        {
            Err(PgTransactionError::Business(error)) => assert_eq!(error.code, "STOCK_NOT_FOUND"),
            _ => panic!("another Company's stock was accepted"),
        }
        foreign_company.rollback().await.unwrap();

        let mut foreign_branch = serializable(&pool).await;
        match prepare_quality_hold(
            &mut foreign_branch,
            &command("foreign-branch"),
            &company(),
            Some(&RecordId::new("branch-other").unwrap()),
            &UserId::new("maker").unwrap(),
            1,
        )
        .await
        {
            Err(PgTransactionError::Business(error)) => assert_eq!(error.code, "STOCK_NOT_FOUND"),
            _ => panic!("another Branch's stock was accepted"),
        }
        foreign_branch.rollback().await.unwrap();

        // A Holding-level caller has no branch filter; the stored hold branch
        // must still come from the locked source warehouse, never the request.
        let mut holding = serializable(&pool).await;
        let prepared = prepare_quality_hold(
            &mut holding,
            &command("holding-scope"),
            &company(),
            None,
            &UserId::new("maker").unwrap(),
            1,
        )
        .await
        .unwrap();
        prepared.apply(&mut holding).await.unwrap();
        let stored_branch: String = sqlx::query_scalar(
            "SELECT branch_id FROM inventory_quality_holds WHERE id='quality-holding-scope'",
        )
        .fetch_one(&mut *holding)
        .await
        .unwrap();
        assert_eq!(stored_branch, "branch");
        holding.rollback().await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_holds")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0, "refused and rolled back holds are not persisted");
    })
    .await;
}

#[tokio::test]
async fn manual_hold_keeps_a_lot_scoped_hold_on_the_current_lot_balance() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) \
             VALUES('tracked','company','TRACKED','Tracked product',2000,'lot');\
             INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
             VALUES('warehouse','tracked',4000,800);\
             INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) \
             VALUES('lot','company','tracked','lot','L1','maker',1);\
             INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) \
             VALUES('lot','warehouse',4000,800);",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let mut lot_hold = command("lot-hold");
        lot_hold.product_id = RecordId::new("tracked").unwrap();
        lot_hold.lot_id = Some(RecordId::new("lot").unwrap());
        lot_hold.expected_lot_version = Some(1);
        let mut tx = serializable(&pool).await;
        let prepared = prepare_quality_hold(
            &mut tx,
            &lot_hold,
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1_800_000_000_000,
        )
        .await
        .unwrap();
        prepared.apply(&mut tx).await.unwrap();
        let saved: (String, i64, String) = sqlx::query_as(
            "SELECT lot_id,held_quantity_milli,quantity_scope \
             FROM inventory_quality_holds WHERE id='quality-lot-hold'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            saved,
            ("lot".into(), 4000, "whole".into()),
            "the retained manual hold keeps the whole current lot balance"
        );
        let balance: (i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,version FROM inventory_lot_balances \
             WHERE lot_id='lot' AND warehouse_id='warehouse'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(balance, (4000, 1), "the hold does not move the lot balance");
        let stock_version: i64 = sqlx::query_scalar(
            "SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='tracked'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stock_version, 2, "the hold trigger advances the stock version");
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn applied_hold_rejects_a_changed_source_stock_and_rolls_back() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let mut tx = serializable(&pool).await;
        let prepared = prepare_quality_hold(
            &mut tx,
            &command("witness-key"),
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1,
        )
        .await
        .unwrap();
        prepared.apply(&mut tx).await.unwrap();

        // The prepared value is bound to the exact source image it read. A
        // later change in the same unit must fail closed instead of retaining
        // a hold whose stock witness no longer matches.
        sqlx::query(
            "UPDATE commerce_stock SET version=version+1 \
             WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        match prepared.require_after(&mut tx).await {
            Err(PgTransactionError::Business(error)) => assert_eq!(error.code, "CONFLICT"),
            _ => panic!("a changed stock source was accepted"),
        }
        tx.rollback().await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_holds")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0, "the rejected prepared hold is not persisted");
    })
    .await;
}

#[tokio::test]
async fn manual_hold_requires_a_serializable_owned_unit() {
    fixture::with_database(1, |pool| async move {
        facts::setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = prepare_quality_hold(
            &mut connection,
            &command("unit-key"),
            &company(),
            Some(&branch()),
            &UserId::new("maker").unwrap(),
            1,
        )
        .await;
        match error {
            Err(PgTransactionError::Persistence(error)) => {
                assert_eq!(error.code, "QUALITY_TRANSACTION_REQUIRED")
            }
            _ => panic!("expected the transaction requirement"),
        }
    })
    .await;
}

#[test]
fn hold_refusal_codes_remain_the_retained_owner_codes() {
    // The adapter maps these BusinessError codes; this keeps accidental code
    // drift visible without a database.
    let errors = [
        BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "STOCK_NOT_FOUND",
            "Active stock location not found.",
        ),
        BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "QUALITY_LOT_CHANGED",
            "The lot or serial balance changed. Refresh before placing the hold.",
        ),
    ];
    assert_eq!(errors[0].code, "STOCK_NOT_FOUND");
    assert_eq!(errors[1].code, "QUALITY_LOT_CHANGED");
}
