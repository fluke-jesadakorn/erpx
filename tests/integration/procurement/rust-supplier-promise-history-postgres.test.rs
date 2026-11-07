//! Native Procurement supplier promise history reads on migrated PostgreSQL.
//! These tests prove owner SQL, Company scope, the open-delivery projection,
//! transaction/serializable guards, rollback and persisted readback; they do
//! not prove a command, purchase admission, RFQ, bill match or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_procurement::public::supplier_promise_history::{
    PgSupplierPromiseHistoryRead, SupplierPromiseReadActor, supplier_promise_history,
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
        permissions: vec!["commerce.read".into()],
        modules: vec!["procurement".into()],
        expires_at: NOW.get() + 60_000,
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
async fn owner_reads_company_scoped_history_and_open_deliveries() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company");
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let result =
                        supplier_promise_history(&current, &mut reader, "order-owned", NOW).await?;
                    assert_eq!(result.order_id, "order-owned");
                    assert_eq!(result.history.len(), 2);
                    assert_eq!(result.history[0].id, "promise-owned-2");
                    assert_eq!(result.history[0].kind, "confirmed");
                    assert_eq!(result.history[1].id, "promise-owned-1");
                    assert_eq!(result.history[1].kind, "withdrawn");
                    assert_eq!(result.lines.len(), 3);
                    assert_eq!(result.open.len(), 3);
                    assert_eq!(result.open[0].id, "promise-line-a1");
                    assert_eq!(result.open[0].order_number, "PO-OWNED");
                    assert_eq!(result.open[0].remaining_milli, 4_000);
                    assert_eq!(result.open[1].id, "promise-line-a2");
                    assert_eq!(result.open[1].prior_quantity, 4_000);
                    assert_eq!(result.open[1].remaining_milli, 2_000);
                    assert_eq!(result.open[2].id, "promise-line-b1");
                    assert_eq!(result.open[2].remaining_milli, 5_000);
                    assert_eq!(result.open[2].description, "Second part");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(persisted, 3, "reads must not create or delete promise rows");
    })
    .await;
}

#[tokio::test]
async fn company_isolation_returns_only_the_current_company_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let company = actor("company");
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = company.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let foreign =
                        supplier_promise_history(&current, &mut reader, "order-foreign", NOW)
                            .await?;
                    assert!(foreign.history.is_empty());
                    assert!(foreign.lines.is_empty());
                    assert!(foreign.open.is_empty());
                    Ok(())
                })
            })
            .await
            .unwrap();

        let foreign = actor("foreign");
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let result =
                        supplier_promise_history(&current, &mut reader, "order-foreign", NOW)
                            .await?;
                    assert_eq!(result.history.len(), 1);
                    assert_eq!(result.history[0].company_id, "foreign");
                    assert_eq!(result.open.len(), 1);
                    assert_eq!(result.open[0].order_number, "PO-FOREIGN");
                    let owned =
                        supplier_promise_history(&current, &mut reader, "order-owned", NOW).await?;
                    assert!(owned.history.is_empty());
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn transaction_guards_rollback_and_persisted_readback() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company");

        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgSupplierPromiseHistoryRead::new(&mut outside)
            .history(&CompanyId::new("company").unwrap(), "order-owned")
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
            .history(&CompanyId::new("company").unwrap(), "order-owned")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "SUPPLIER_PROMISE_HISTORY_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();

        let before: i64 =
            sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let read_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_owner.clone();
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let result =
                        supplier_promise_history(&current, &mut reader, "order-owned", NOW).await?;
                    assert_eq!(result.history.len(), 2);
                    Ok(())
                })
            })
            .await
            .unwrap();
        let after: i64 =
            sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(before, after);

        let rollback = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO procurement_delivery_promises(id,order_id,company_id,order_version,supplier_id,currency,required_date,kind,confirmation_date,supplier_reference,evidence,recorded_by,created_at) VALUES \
                         ('promise-rollback','order-owned','company',3,'supplier','THB','2026-11-10','withdrawn','2026-11-02','SUP-REF-3','Rollback supplier evidence','owner',30);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let result =
                        supplier_promise_history(&current, &mut reader, "order-owned", NOW).await?;
                    assert_eq!(result.history.len(), 3);
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await;
        assert!(rollback.is_err());
        let disappeared: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM procurement_delivery_promises WHERE id='promise-rollback'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(disappeared, 0);
    })
    .await;
}

#[tokio::test]
async fn policy_fails_before_source_reads() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut reader = PgSupplierPromiseHistoryRead::new(connection);
                    let denied = SupplierPromiseReadActor {
                        permissions: Vec::new(),
                        ..actor("company")
                    };
                    let error = business(
                        supplier_promise_history(&denied, &mut reader, "order-owned", NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.kind, FailureKind::Forbidden);
                    assert_eq!(error.code, "PERMISSION_REQUIRED");

                    let module = SupplierPromiseReadActor {
                        modules: vec!["finance".into()],
                        ..actor("company")
                    };
                    let error = business(
                        supplier_promise_history(&module, &mut reader, "order-owned", NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.code, "MODULE_FORBIDDEN");

                    let mut expired = actor("company");
                    expired.expires_at = NOW.get();
                    let error = business(
                        supplier_promise_history(&expired, &mut reader, "order-owned", NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.kind, FailureKind::Conflict);
                    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");

                    let invalid = supplier_promise_history(&actor("company"), &mut reader, "", NOW)
                        .await
                        .unwrap_err();
                    assert_eq!(business(invalid).code, "PURCHASE_ORDER_NOT_FOUND");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
