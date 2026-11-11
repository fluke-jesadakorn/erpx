//! Native Procurement supplier promise detail reads on migrated PostgreSQL.
//! These prove the retained detail shape, scoped purchase-order SQL, the
//! outstanding/unconfirmed projection, transaction/serializable guards and
//! non-mutation; they do not prove purchase admission or a browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_procurement::public::supplier_promise_history::{
    PgSupplierPromiseHistoryRead, SupplierPromisePurchaseAccess, SupplierPromiseReadActor,
    supplier_promise_detail,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(company: &str) -> SupplierPromiseReadActor {
    SupplierPromiseReadActor {
        user_id: "owner".into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec!["commerce.read".into(), "commerce.purchase".into()],
        modules: vec!["procurement".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn overlay() -> SupplierPromisePurchaseAccess {
    SupplierPromisePurchaseAccess {
        new_work: true,
        settle_existing: true,
        history: true,
        revision: "sha256:7".into(),
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company','company','active',1),\
         ('branch','branch','branch','Branch','company','company','branch','active',1),\
         ('foreign','company','foreign','Foreign company',NULL,'foreign','foreign','active',1),\
         ('branch-foreign','branch','branch-foreign','Foreign branch','foreign','foreign','branch-foreign','active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('supplier','company','supplier','SUP-1','Bounded supplier','THB',10,10),\
         ('supplier-foreign','foreign','supplier','SUP-F','Foreign supplier','THB',10,10);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES \
         ('warehouse','company','branch','WH-1','Main warehouse'),\
         ('warehouse-foreign','foreign','branch-foreign','WH-F','Foreign warehouse');\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES \
         ('product-a','company','SKU-A','Bounded part','each',100,700),\
         ('product-b','company','SKU-B','Second part','each',200,700),\
         ('product-foreign','foreign','SKU-F','Foreign part','each',100,700);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('order-owned','company','branch',NULL,'purchase','PO-OWNED','supplier','warehouse','THB','2026-10-20','2026-11-10','confirmed',10000,700,'owner',10,10),\
         ('order-foreign','foreign','branch-foreign',NULL,'purchase','PO-FOREIGN','supplier-foreign','warehouse-foreign','THB','2026-10-20','2026-11-10','confirmed',10000,700,'owner',10,10);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES \
         ('order-line-a','order-owned','product-a','Bounded part',10000,4000,100,700,10000,700,0),\
         ('order-line-b','order-owned','product-b','Second part',5000,0,200,700,5000,350,1),\
         ('order-line-f','order-foreign','product-foreign','Foreign part',8000,0,100,700,8000,560,0);\
         INSERT INTO procurement_delivery_promises(id,order_id,company_id,order_version,supplier_id,currency,required_date,kind,confirmation_date,supplier_reference,evidence,recorded_by,created_at) VALUES \
         ('promise-owned-1','order-owned','company',1,'supplier','THB','2026-11-10','withdrawn','2026-10-30','SUP-REF-0','Earlier supplier withdrawal', 'owner',10),\
         ('promise-owned-2','order-owned','company',2,'supplier','THB','2026-11-10','confirmed','2026-11-01','SUP-REF-1','Current supplier confirmation','owner',20),\
         ('promise-foreign-1','order-foreign','foreign',1,'supplier-foreign','THB','2026-11-10','confirmed','2026-11-01','SUP-F-1','Foreign supplier confirmation','owner',10);\
         INSERT INTO procurement_delivery_promise_lines(id,promise_id,order_line_id,ordinal,quantity_milli,fulfilled_snapshot_milli,promised_date) VALUES \
         ('promise-line-a1','promise-owned-2','order-line-a',0,4000,4000,'2026-11-08'),\
         ('promise-line-a2','promise-owned-2','order-line-a',1,2000,4000,'2026-11-09'),\
         ('promise-line-b1','promise-owned-2','order-line-b',2,5000,0,'2026-11-09'),\
         ('promise-line-f1','promise-foreign-1','order-line-f',0,8000,0,'2026-11-08');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

#[tokio::test]
async fn owner_detail_returns_the_retained_shape_and_quantities() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company");
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let detail = supplier_promise_detail(
                        &current,
                        &mut reader,
                        "order-owned",
                        Some(overlay()),
                        NOW,
                    )
                    .await?;

                    assert_eq!(detail.order.id, "order-owned");
                    assert_eq!(detail.order.number, "PO-OWNED");
                    assert_eq!(detail.order.kind, "purchase");
                    assert_eq!(detail.order.state, "confirmed");
                    assert_eq!(detail.order.version, 1);
                    assert_eq!(detail.order.branch_id.as_deref(), Some("branch"));
                    assert_eq!(detail.order.warehouse_id.as_deref(), Some("warehouse"));
                    assert_eq!(detail.order.supplier_name, "Bounded supplier");
                    assert_eq!(detail.order.subtotal_minor, 10_000);
                    assert_eq!(detail.order.tax_minor, 700);
                    assert_eq!(detail.order.discount_approval_required, 0);

                    assert_eq!(detail.lines.len(), 2);
                    let line_a = detail
                        .lines
                        .iter()
                        .find(|line| line.id == "order-line-a")
                        .unwrap();
                    assert_eq!(line_a.sku, "SKU-A");
                    assert_eq!(line_a.description, "Bounded part");
                    assert_eq!(line_a.quantity_milli, 10_000);
                    assert_eq!(line_a.fulfilled_milli, 4_000);
                    assert_eq!(line_a.price_minor, 100);
                    assert_eq!(line_a.tax_bps, 700);
                    assert_eq!(line_a.item_type, "stock");
                    assert_eq!(line_a.ordinal, 0);
                    assert_eq!(line_a.outstanding_milli, 6_000);
                    // Both open deliveries on this line total the outstanding
                    // quantity, so nothing is left unconfirmed.
                    assert_eq!(line_a.unconfirmed_milli, 0);
                    let line_b = detail
                        .lines
                        .iter()
                        .find(|line| line.id == "order-line-b")
                        .unwrap();
                    assert_eq!(line_b.sku, "SKU-B");
                    assert_eq!(line_b.outstanding_milli, 5_000);
                    assert_eq!(line_b.unconfirmed_milli, 0);

                    assert_eq!(detail.history.len(), 2);
                    assert_eq!(detail.history[0].id, "promise-owned-2");
                    assert_eq!(detail.schedule.len(), 3);
                    assert_eq!(detail.open.len(), 3);
                    assert_eq!(detail.open[0].order_number, "PO-OWNED");
                    assert!(detail.can_record);
                    let purchase = detail.purchase.as_ref().unwrap();
                    assert!(purchase.new_work && purchase.settle_existing && purchase.history);
                    assert_eq!(purchase.revision, "sha256:7");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let persisted: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(persisted, 3, "reads must not create or delete promise rows");
    })
    .await;
}

#[tokio::test]
async fn unconfirmed_quantity_subtracts_only_open_supplier_promises() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        // Immutable promise rows cannot be edited; move the *actual* receipt
        // quantity after the snapshot so the open promise allocation no longer
        // covers the whole outstanding line.
        sqlx::raw_sql(
            "UPDATE commerce_order_lines SET fulfilled_milli=2000 WHERE id='order-line-a';",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let owner = actor("company");
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let detail = supplier_promise_detail(
                        &current,
                        &mut reader,
                        "order-owned",
                        None,
                        NOW,
                    )
                    .await?;
                    let line_a = detail
                        .lines
                        .iter()
                        .find(|line| line.id == "order-line-a")
                        .unwrap();
                    assert_eq!(line_a.outstanding_milli, 8_000);
                    assert_eq!(line_a.unconfirmed_milli, 2_000);
                    assert!(detail.can_record);
                    assert!(detail.purchase.is_none());
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn order_scope_and_company_isolation_return_the_retained_not_found() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company");
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let foreign = business(
                        supplier_promise_detail(&current, &mut reader, "order-foreign", None, NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(foreign.kind, FailureKind::NotFound);
                    assert_eq!(foreign.code, "PURCHASE_ORDER_NOT_FOUND");

                    let wrong_branch = SupplierPromiseReadActor {
                        branch_id: Some(RecordId::new("other-branch").unwrap()),
                        ..current.clone()
                    };
                    let hidden = business(
                        supplier_promise_detail(&wrong_branch, &mut reader, "order-owned", None, NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(hidden.code, "PURCHASE_ORDER_NOT_FOUND");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let foreign = SupplierPromiseReadActor {
            branch_id: Some(RecordId::new("branch-foreign").unwrap()),
            ..actor("foreign")
        };
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let detail = supplier_promise_detail(
                        &current,
                        &mut reader,
                        "order-foreign",
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(detail.order.company_id, "foreign");
                    assert_eq!(detail.lines.len(), 1);
                    assert_eq!(detail.lines[0].sku, "SKU-F");
                    let owned = business(
                        supplier_promise_detail(&current, &mut reader, "order-owned", None, NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(owned.code, "PURCHASE_ORDER_NOT_FOUND");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn owner_iam_without_commerce_purchase_cannot_record() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = SupplierPromiseReadActor {
            permissions: vec!["commerce.read".into()],
            ..actor("company")
        };
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let detail = supplier_promise_detail(
                        &current,
                        &mut reader,
                        "order-owned",
                        Some(overlay()),
                        NOW,
                    )
                    .await?;
                    assert!(!detail.can_record);
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn detail_sql_requires_the_serializable_owner_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company");

        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgSupplierPromiseHistoryRead::new(&mut outside)
            .order(&owner, "order-owned")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "SUPPLIER_PROMISE_HISTORY_TRANSACTION_REQUIRED"
        ));
        let error = PgSupplierPromiseHistoryRead::new(&mut outside)
            .source_lines(&CompanyId::new("company").unwrap(), "order-owned")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "SUPPLIER_PROMISE_HISTORY_TRANSACTION_REQUIRED"
        ));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let error = PgSupplierPromiseHistoryRead::new(&mut default_isolation)
            .order(&owner, "order-owned")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "SUPPLIER_PROMISE_HISTORY_SERIALIZABLE_REQUIRED"
        ));
        let error = PgSupplierPromiseHistoryRead::new(&mut default_isolation)
            .source_lines(&CompanyId::new("company").unwrap(), "order-owned")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "SUPPLIER_PROMISE_HISTORY_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();
    })
    .await;
}
