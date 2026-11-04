//! Native Procurement supplier promise command on migrated PostgreSQL.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_procurement::public::supplier_promise_command::{
    PgSupplierPromiseCommand, RecordSupplierPromiseCommand, SupplierPromiseCommandActor,
    SupplierPromiseCommandRepository, SupplierPromiseLineCommand, SupplierPromiseRecordReceipt,
    record_supplier_promise,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(company: &str, branch: &str, user: &str) -> SupplierPromiseCommandActor {
    SupplierPromiseCommandActor {
        user_id: user.into(),
        assignment_id: format!("assignment-{user}"),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: Some(RecordId::new(branch).unwrap()),
        department_id: None,
        revision: 1,
        assignment_version: 1,
        permissions: vec!["commerce.read".into(), "commerce.purchase".into()],
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
         ('order-line-f','order-foreign','product-foreign','Foreign part',8000,0,100,700,8000,560,0);",
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

fn input() -> RecordSupplierPromiseCommand {
    RecordSupplierPromiseCommand {
        expected_version: 1,
        kind: "confirmed".into(),
        confirmation_date: "2026-10-25".into(),
        supplier_reference: "SUP-REF-1".into(),
        evidence: "Supplier confirmation evidence".into(),
        attachment_set_ids: Vec::new(),
        lines: vec![SupplierPromiseLineCommand {
            order_line_id: "order-line-a".into(),
            quantity: "4.000".into(),
            promised_date: "2026-10-26".into(),
        }],
    }
}

async fn run_command(
    pool: &DatabasePool,
    actor: SupplierPromiseCommandActor,
    order_id: &str,
    input: RecordSupplierPromiseCommand,
    key: &str,
) -> Result<SupplierPromiseRecordReceipt, BusinessError> {
    PgUnitOfWork::new(pool.clone())
        .run::<SupplierPromiseRecordReceipt, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let input = input.clone();
            let key = key.to_owned();
            let order_id = order_id.to_owned();
            Box::pin(async move {
                let mut repo = PgSupplierPromiseCommand::new(connection);
                record_supplier_promise(&actor, &mut repo, &order_id, &input, &key, NOW).await
            })
        })
        .await
        .map_err(business)
}

#[tokio::test]
async fn records_supplier_promise_and_replays_idempotently() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", "branch", "owner");
        let receipt = run_command(&pool, owner.clone(), "order-owned", input(), "command-1").await.unwrap();
        assert_eq!(receipt.id, "order-owned");
        assert_eq!(receipt.version, 2);
        assert_eq!(receipt.promise_id, "supplier-promise-command-1");

        // counts
        let promise_count: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises WHERE order_id='order-owned'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(promise_count, 1);
        let line_count: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promise_lines")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(line_count, 1);
        let receipt_count: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='command-1'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(receipt_count, 1);
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE command_id='command-1'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(audit_count, 1);
        let outbox_count: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE command_id='command-1'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(outbox_count, 1);

        // order version advanced
        let version: i64 = sqlx::query_scalar("SELECT version FROM commerce_orders WHERE id='order-owned'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(version, 2);

        // replay
        let replay = run_command(&pool, owner.clone(), "order-owned", input(), "command-1").await.unwrap();
        assert_eq!(replay, receipt);

        let promise_count2: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises WHERE order_id='order-owned'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(promise_count2, 1);
        let receipt_count2: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='command-1'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(receipt_count2, 1);
    })
    .await;
}

#[tokio::test]
async fn idempotency_conflict_for_different_request() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", "branch", "owner");
        run_command(&pool, owner.clone(), "order-owned", input(), "command-2").await.unwrap();

        let mut different = input();
        different.supplier_reference = "DIFFERENT-REF".into();
        let err = run_command(&pool, owner.clone(), "order-owned", different, "command-2").await.unwrap_err();
        assert_eq!(err.code, "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn cas_conflict_when_order_version_changed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", "branch", "owner");
        // advance version manually
        sqlx::query("UPDATE commerce_orders SET version=2 WHERE id='order-owned'")
            .execute(pool.raw()).await.unwrap();

        let err = run_command(&pool, owner.clone(), "order-owned", input(), "command-3").await.unwrap_err();
        assert_eq!(err.code, "PURCHASE_ORDER_CHANGED");

        let promise_count: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises WHERE order_id='order-owned'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(promise_count, 0);
    })
    .await;
}

#[tokio::test]
async fn company_isolation_blocks_foreign_access() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let foreign = actor("foreign", "branch-foreign", "foreign-user");
        let err = run_command(&pool, foreign.clone(), "order-owned", input(), "command-4").await.unwrap_err();
        assert_eq!(err.code, "PURCHASE_ORDER_NOT_FOUND");

        let owner = actor("company", "branch", "owner");
        let err = run_command(&pool, owner.clone(), "order-foreign", input(), "command-5").await.unwrap_err();
        assert_eq!(err.code, "PURCHASE_ORDER_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn serializable_guard_fails_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let mut outside = pool.raw().acquire().await.unwrap();
        let err = PgSupplierPromiseCommand::new(&mut outside)
            .receipt("key")
            .await
            .unwrap_err();
        assert!(matches!(err, PgTransactionError::Persistence(ref value) if value.code == "PROCUREMENT_PROMISE_SERIALIZABLE_REQUIRED"));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let err = PgSupplierPromiseCommand::new(&mut default_isolation)
            .receipt("key")
            .await
            .unwrap_err();
        assert!(matches!(err, PgTransactionError::Persistence(ref value) if value.code == "PROCUREMENT_PROMISE_SERIALIZABLE_REQUIRED"));
        default_isolation.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn readback_and_audit_outbox_persisted() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", "branch", "owner");
        let receipt = run_command(&pool, owner.clone(), "order-owned", input(), "command-6").await.unwrap();

        let promise_id = receipt.promise_id.clone();
        // readback row
        let (id, order_id, version, kind, recorded_by, line_count): (String,String,i64,String,String,i64) =
            sqlx::query_as("SELECT id,order_id,order_version,kind,recorded_by,(SELECT count(*)::bigint FROM procurement_delivery_promise_lines l WHERE l.promise_id=procurement_delivery_promises.id) FROM procurement_delivery_promises WHERE id=$1")
            .bind(&promise_id)
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(id, promise_id);
        assert_eq!(order_id, "order-owned");
        assert_eq!(version, 1);
        assert_eq!(kind, "confirmed");
        assert_eq!(recorded_by, "owner");
        assert_eq!(line_count, 1);

        // audit assignment_id
        let assignment: String = sqlx::query_scalar("SELECT assignment_id FROM audit_events WHERE command_id='command-6'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(assignment, "assignment-owner");

        // outbox payload contains company
        let payload: String = sqlx::query_scalar("SELECT payload_json::text FROM outbox_events WHERE command_id='command-6'")
            .fetch_one(pool.raw()).await.unwrap();
        assert!(payload.contains("company"));
    })
    .await;
}