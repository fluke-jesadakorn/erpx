//! Native Sales confirmed sales-order cancellation on migrated PostgreSQL.
//! Proves current IAM, Company/branch scope, confirmed-sales narrowing,
//! independent canceller, fulfillment reversal, version/CAS, idempotent replay,
//! Inventory reservation release, receipt/audit/outbox, rollback and readback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_sales::public::sales_order_cancellation::{
    PgSalesCancellation, SalesCancellationActor, SalesCancellationCommand,
    SalesCancellationReceipt, cancel_confirmed_sales_order,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use sqlx::Row;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(
    user: &str,
    role: &str,
    assignment: &str,
    branch: Option<&str>,
    permissions: &[&str],
) -> SalesCancellationActor {
    SalesCancellationActor {
        user_id: user.into(),
        person_user_ids: vec![],
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        assignment_id: assignment.into(),
        role: Some(role.into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["sales".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn reviewer() -> SalesCancellationActor {
    actor(
        "reviewer",
        "sales_manager",
        "assignment-reviewer",
        Some("branch"),
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn creator() -> SalesCancellationActor {
    actor(
        "creator",
        "sales_operator",
        "assignment-creator",
        Some("branch"),
        &["commerce.read", "commerce.sales"],
    )
}

fn foreign() -> SalesCancellationActor {
    SalesCancellationActor {
        company_id: CompanyId::new("foreign").unwrap(),
        branch_id: Some(RecordId::new("foreign-branch").unwrap()),
        user_id: "foreign-user".into(),
        assignment_id: "assignment-foreign".into(),
        role: Some("sales_manager".into()),
        ..reviewer()
    }
}

fn command(order: &str, expected: i64, key: &str) -> SalesCancellationCommand {
    SalesCancellationCommand {
        key: key.into(),
        order_id: order.into(),
        expected_version: expected,
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn run_cancel(
    pool: &DatabasePool,
    actor: SalesCancellationActor,
    command: SalesCancellationCommand,
) -> Result<SalesCancellationReceipt, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgSalesCancellation::new(connection);
                cancel_confirmed_sales_order(&actor, &command, &mut repository, NOW).await
            })
        })
        .await
}

async fn run_error(
    pool: &DatabasePool,
    actor: SalesCancellationActor,
    command: SalesCancellationCommand,
) -> BusinessError {
    run_cancel(pool, actor, command)
        .await
        .map_err(business)
        .unwrap_err()
}

async fn counts(pool: &DatabasePool, key: &str) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
         (SELECT count(*) FROM audit_events WHERE command_id=$1),\
         (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
    )
    .bind(key)
    .fetch_one(pool.raw())
    .await
    .unwrap()
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
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign company',NULL,'foreign',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('other-branch','branch','other-branch','Other branch',NULL,'company',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch',NULL,'foreign',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','creator','local:creator','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1),\
         ('company','revoked','local:revoked','active','local',1),\
         ('company','other-branch','local:other-branch','active','local',1),\
         ('foreign','foreign-user','local:foreign-user','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-creator','creator','sales_operator','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-reviewer','reviewer','sales_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-revoked','revoked','sales_manager','[]','company','branch',0,9000000000000,1,'branch','branch','manual'),\
         ('assignment-other-branch','other-branch','sales_manager','[]','company','other-branch',0,9000000000000,NULL,'branch','other-branch','manual'),\
         ('assignment-foreign','foreign-user','sales_manager','[]','foreign','foreign-branch',0,9000000000000,NULL,'branch','foreign-branch','manual');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES \
         ('party-1','company','customer','C1','Customer','THB',30,0,'active',1,1,1),\
         ('party-f','foreign','customer','CF','Foreign customer','THB',30,0,'active',1,1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1),\
         ('foreign-warehouse','foreign','foreign-branch','WHF','Foreign warehouse','active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) VALUES \
         ('stock-1','company','STK-1','Stock item','each',10000,700,'active',1,'none','stock'),\
         ('stock-f','foreign','STK-F','Foreign stock','each',10000,700,'active',1,'none','stock');\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES \
         ('warehouse-1','stock-1',10000,100000,1),\
         ('foreign-warehouse','stock-f',10000,100000,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES \
         ('sales-confirmed','company','branch',NULL,'sales','SO-1','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',10000,700,NULL,3,'creator','reviewer',100,300),\
         ('sales-confirmed-fulfilled','company','branch',NULL,'sales','SO-2','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',10000,700,NULL,3,'creator','reviewer',100,300),\
         ('sales-draft','company','branch',NULL,'sales','SO-3','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'creator',NULL,100,100),\
         ('quote-confirmed','company','branch',NULL,'quote','QT-1','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',10000,700,NULL,2,'creator','reviewer',100,200),\
         ('sales-other-branch','company','other-branch',NULL,'sales','SO-4','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',10000,700,NULL,2,'creator','reviewer',100,200),\
         ('sales-foreign','foreign','foreign-branch',NULL,'sales','SO-F','party-f','foreign-warehouse','THB','2026-11-01','2026-11-08','','confirmed',10000,700,NULL,2,'foreign-user','foreign-user',100,200);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type) VALUES \
         ('sales-confirmed-line','sales-confirmed','stock-1','Stock',10000,0,1000,700,10000,700,0,NULL,0,'stock'),\
         ('sales-confirmed-fulfilled-line','sales-confirmed-fulfilled','stock-1','Stock',10000,1000,1000,700,10000,700,0,NULL,0,'stock'),\
         ('sales-draft-line','sales-draft','stock-1','Stock',10000,0,1000,700,10000,700,0,NULL,0,'stock'),\
         ('quote-confirmed-line','quote-confirmed','stock-1','Stock',10000,0,1000,700,10000,700,0,NULL,0,'stock'),\
         ('sales-other-branch-line','sales-other-branch','stock-1','Stock',10000,0,1000,700,10000,700,0,NULL,0,'stock'),\
         ('sales-foreign-line','sales-foreign','stock-f','Foreign stock',10000,0,1000,700,10000,700,0,NULL,0,'stock');\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES \
         ('fulfillment-1','sales-confirmed-fulfilled','F-1','2026-11-02','creator',100,1);\
         INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,release_reason,created_at) VALUES \
         ('res-open','sales-confirmed-line','warehouse-1','stock-1',5000,1000,0,9000000000000,'creator','Sales reservation',NULL,100),\
         ('res-already','sales-confirmed-line','warehouse-1','stock-1',2000,500,1500,9000000000000,'creator','Sales reservation','Already released',100),\
         ('res-fulfilled','sales-confirmed-fulfilled-line','warehouse-1','stock-1',2000,0,0,9000000000000,'creator','Sales reservation',NULL,100),\
         ('res-draft','sales-draft-line','warehouse-1','stock-1',2000,0,0,9000000000000,'creator','Sales reservation',NULL,100);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn confirmed_cancel_releases_only_open_reservations_and_writes_one_unit() {
    fixture::with_database(5, |pool| async move {
        setup(&pool).await;
        let key = "sales-cancel-0001";
        let receipt = run_cancel(&pool, reviewer(), command("sales-confirmed", 3, key))
            .await
            .unwrap();
        assert_eq!(
            receipt,
            SalesCancellationReceipt {
                id: "sales-confirmed".into(),
                state: "cancelled".into(),
                version: 4,
                sales_id: None,
            }
        );

        let row = sqlx::query(
            "SELECT state,version,updated_at FROM commerce_orders WHERE id='sales-confirmed'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("state"), "cancelled");
        assert_eq!(row.get::<i64, _>("version"), 4);
        assert_eq!(row.get::<i64, _>("updated_at"), NOW.get());

        let rows: Vec<(String, i64, i64, Option<String>)> = sqlx::query_as(
            "SELECT id,released_milli,quantity_milli,release_reason \
             FROM inventory_order_reservations WHERE order_line_id='sales-confirmed-line' \
             ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    "res-already".into(),
                    1500,
                    2000,
                    Some("Already released".into())
                ),
                (
                    "res-open".into(),
                    4000,
                    5000,
                    Some("Source order cancelled".into())
                ),
            ]
        );
        let draft_release: i64 = sqlx::query_scalar(
            "SELECT released_milli FROM inventory_order_reservations WHERE id='res-draft'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(draft_release, 0);
        assert_eq!(counts(&pool, key).await, (1, 1, 1));

        let receipt_row = sqlx::query(
            "SELECT operation,actor_id,record_id,result_json FROM command_receipts \
             WHERE command_id=$1",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            receipt_row.get::<String, _>("operation"),
            "commerce.order.cancel"
        );
        assert_eq!(receipt_row.get::<String, _>("actor_id"), "reviewer");
        assert_eq!(receipt_row.get::<String, _>("record_id"), "sales-confirmed");
        assert!(
            receipt_row
                .get::<String, _>("result_json")
                .contains("\"state\":\"cancelled\"")
        );

        let payload: String =
            sqlx::query_scalar("SELECT payload_json FROM outbox_events WHERE command_id=$1")
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["state"], "cancelled");
        assert_eq!(payload["version"], 4);
        assert_eq!(payload["salesId"], serde_json::Value::Null);

        // Same key replays the canonical receipt without another write.
        let replay = run_cancel(&pool, reviewer(), command("sales-confirmed", 3, key))
            .await
            .unwrap();
        assert_eq!(replay, receipt);
        assert_eq!(counts(&pool, key).await, (1, 1, 1));
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM commerce_orders WHERE id='sales-confirmed'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(version, 4);
    })
    .await;
}

#[tokio::test]
async fn authority_scope_state_and_version_fail_closed_without_effects() {
    fixture::with_database(6, |pool| async move {
        setup(&pool).await;

        let error = run_error(
            &pool,
            creator(),
            command("sales-confirmed", 3, "sales-cancel-creator-01"),
        )
        .await;
        assert_eq!(error.code, "COMMERCE_APPROVAL_REQUIRED");

        let mut revoked = reviewer();
        revoked.user_id = "revoked".into();
        revoked.assignment_id = "assignment-revoked".into();
        assert_eq!(
            run_error(
                &pool,
                revoked,
                command("sales-confirmed", 3, "sales-cancel-revoked-01")
            )
            .await
            .code,
            "COMMERCE_PERMISSION_REQUIRED"
        );

        assert_eq!(
            run_error(
                &pool,
                reviewer(),
                command("sales-draft", 1, "sales-cancel-draft-0001")
            )
            .await
            .code,
            "SALES_CANCELLATION_CONFIRMED_REQUIRED"
        );
        assert_eq!(
            run_error(
                &pool,
                reviewer(),
                command("quote-confirmed", 2, "sales-cancel-quote-0001")
            )
            .await
            .code,
            "SALES_CANCELLATION_ORDER_REQUIRED"
        );
        assert_eq!(
            run_error(
                &pool,
                reviewer(),
                command("sales-confirmed", 2, "sales-cancel-version-01")
            )
            .await
            .code,
            "CONFLICT"
        );
        assert_eq!(
            run_error(
                &pool,
                reviewer(),
                command("sales-confirmed-fulfilled", 3, "sales-cancel-fulfilled-1")
            )
            .await
            .code,
            "ORDER_REVERSAL_REQUIRED"
        );
        assert_eq!(
            run_error(
                &pool,
                reviewer(),
                command("sales-other-branch", 2, "sales-cancel-branch-001")
            )
            .await
            .code,
            "SALES_ORDER_NOT_FOUND"
        );
        assert_eq!(
            run_error(
                &pool,
                foreign(),
                command("sales-foreign", 2, "sales-cancel-foreign-01")
            )
            .await
            .code,
            "INDEPENDENT_CANCELLER_REQUIRED"
        );
        assert_eq!(
            run_error(
                &pool,
                reviewer(),
                command("sales-foreign", 2, "sales-cancel-foreign-02")
            )
            .await
            .code,
            "SALES_ORDER_NOT_FOUND"
        );

        let state: String =
            sqlx::query_scalar("SELECT state FROM commerce_orders WHERE id='sales-confirmed'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(state, "confirmed");
        assert_eq!(counts(&pool, "sales-cancel-creator-01").await, (0, 0, 0));
        assert_eq!(counts(&pool, "sales-cancel-fulfilled-1").await, (0, 0, 0));
        let released: i64 = sqlx::query_scalar(
            "SELECT COALESCE(sum(released_milli),0)::bigint FROM inventory_order_reservations \
             WHERE id='res-fulfilled'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(released, 0);
    })
    .await;
}

#[tokio::test]
async fn rollback_removes_the_order_receipt_audit_outbox_and_release() {
    fixture::with_database(7, |pool| async move {
        setup(&pool).await;
        let key = "sales-cancel-rollback-1";
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut repository = PgSalesCancellation::new(connection);
                    cancel_confirmed_sales_order(
                        &reviewer(),
                        &command("sales-confirmed", 3, key),
                        &mut repository,
                        NOW,
                    )
                    .await?;
                    Err::<(), PgTransactionError<BusinessError>>(
                        BusinessError::new(
                            erpx_server_shared::public::FailureKind::Unavailable,
                            "SYNTHETIC_ROLLBACK",
                            "Roll back this unit.",
                        )
                        .into(),
                    )
                })
            })
            .await;
        assert!(result.is_err());
        assert_eq!(counts(&pool, key).await, (0, 0, 0));
        let row =
            sqlx::query("SELECT state,version FROM commerce_orders WHERE id='sales-confirmed'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(row.get::<String, _>("state"), "confirmed");
        assert_eq!(row.get::<i64, _>("version"), 3);
        let released: i64 = sqlx::query_scalar(
            "SELECT released_milli FROM inventory_order_reservations WHERE id='res-open'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(released, 0);
    })
    .await;
}
