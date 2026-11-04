//! Native Commerce quotation/sales-order read and lifecycle on migrated
//! PostgreSQL: current IAM, Company/branch isolation, versions/CAS,
//! idempotency, audit/outbox, rollback and persisted readback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commerce::public::order::{
    CommerceOrderActor, CommerceOrderReadRepository, OrderAction, PgCommerceOrders,
    SalesOrderDocument, TransitionCommand, order_detail, transition_order,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis,
    UserId,
};
use serde_json::json;
use sqlx::Row;

const NOW: i64 = 1_800_000_000_000;

fn actor(
    user: &str,
    role: &str,
    assignment: &str,
    branch: Option<&str>,
    permissions: &[&str],
) -> CommerceOrderActor {
    CommerceOrderActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: Some(role.into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["sales".into()],
        expires_at: NOW + 60_000,
    }
}

fn creator() -> CommerceOrderActor {
    actor(
        "creator",
        "sales_operator",
        "assignment-creator",
        Some("branch"),
        &["commerce.read", "commerce.sales"],
    )
}

fn reviewer() -> CommerceOrderActor {
    actor(
        "reviewer",
        "sales_manager",
        "assignment-reviewer",
        Some("branch"),
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn other_branch() -> CommerceOrderActor {
    actor(
        "other-branch",
        "sales_operator",
        "assignment-other-branch",
        Some("other-branch"),
        &["commerce.read", "commerce.sales"],
    )
}

fn foreign() -> CommerceOrderActor {
    CommerceOrderActor {
        company_id: CompanyId::new("foreign").unwrap(),
        branch_id: Some(RecordId::new("foreign-branch").unwrap()),
        user_id: UserId::new("foreign-user").unwrap(),
        assignment_id: AssignmentId::new("assignment-foreign").unwrap(),
        role: Some("sales_operator".into()),
        ..creator()
    }
}

fn command(order_id: &str, action: OrderAction, expected: i64, key: &str) -> TransitionCommand {
    TransitionCommand {
        key: CommandKey::new(key).unwrap(),
        order_id: RecordId::new(order_id).unwrap(),
        action,
        expected_version: expected,
        reason: None,
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
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
         ('assignment-revoked','revoked','sales_operator','[]','company','branch',0,9000000000000,1,'branch','branch','manual'),\
         ('assignment-other-branch','other-branch','sales_operator','[]','company','other-branch',0,9000000000000,NULL,'branch','other-branch','manual'),\
         ('assignment-foreign','foreign-user','sales_operator','[]','foreign','foreign-branch',0,9000000000000,NULL,'branch','foreign-branch','manual');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES \
         ('party-1','company','customer','C1','Customer','THB',30,0,'active',1,1,1),\
         ('party-f','foreign','customer','CF','Foreign customer','THB',30,0,'active',1,1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) VALUES \
         ('product-1','company','SVC-1','Service','each',10000,700,'active',1,'none','service'),\
         ('product-f','foreign','SVC-F','Foreign service','each',10000,700,'active',1,'none','service');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO commerce_orders\
         (id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,\
         order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,\
         approved_by,created_at,updated_at,price_list_id,price_list_version,discount_reason,\
         discount_approval_required,discount_approved_by,discount_approved_at) VALUES \
         ('quote-draft','company','branch',NULL,'quote','QT-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','', 'draft',10000,700,NULL,1,'creator',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL),\
         ('quote-discount','company','branch',NULL,'quote','QT-2','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','submitted',10000,700,NULL,2,'creator',NULL,100,200,NULL,NULL,'Exceptional discount',1,NULL,NULL),\
         ('quote-confirmed','company','branch',NULL,'quote','QT-3','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','confirmed',10000,700,NULL,3,'creator','reviewer',100,300,NULL,NULL,NULL,0,NULL,NULL),\
         ('quote-confirmed-fulfilled','company','branch',NULL,'quote','QT-4','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','confirmed',10000,700,NULL,4,'creator','reviewer',100,400,NULL,NULL,NULL,0,NULL,NULL),\
         ('sales-draft','company','branch',NULL,'sales','SO-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'creator',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL),\
         ('sales-submitted','company','branch',NULL,'sales','SO-2','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','submitted',10000,700,NULL,2,'creator',NULL,100,200,NULL,NULL,NULL,0,NULL,NULL),\
         ('sales-confirmed','company','branch',NULL,'sales','SO-3','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','confirmed',10000,700,NULL,3,'creator','reviewer',100,300,NULL,NULL,NULL,0,NULL,NULL),\
         ('purchase-draft','company','branch',NULL,'purchase','PO-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'creator',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL),\
         ('foreign-quote','foreign','foreign-branch',NULL,'quote','QT-F','party-f',NULL,'THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'foreign-user',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL),\
         ('other-branch-quote','company','other-branch',NULL,'quote','QT-O','party-1',NULL,'THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'other-branch',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL);\
         INSERT INTO commerce_order_lines\
         (id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,\
         subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type) VALUES \
         ('quote-draft-line','quote-draft','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('quote-discount-line','quote-discount','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('quote-confirmed-line','quote-confirmed','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('quote-confirmed-fulfilled-line','quote-confirmed-fulfilled','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('sales-draft-line','sales-draft','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('sales-submitted-line','sales-submitted','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('sales-confirmed-line','sales-confirmed','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('purchase-draft-line','purchase-draft','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('foreign-quote-line','foreign-quote','product-f','Foreign service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('other-branch-quote-line','other-branch-quote','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service');\
         INSERT INTO commerce_fulfillments\
         (id,order_id,number,fulfillment_date,created_by,created_at,journal_id,invoice_id,version) VALUES \
         ('fulfillment-1','quote-confirmed-fulfilled','F-1','2026-11-02','creator',100,NULL,NULL,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run_detail(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    id: &str,
) -> Result<SalesOrderDocument, PgTransactionError<BusinessError>> {
    let id = id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let id = id.clone();
            Box::pin(async move {
                let mut repository = PgCommerceOrders::new(connection);
                order_detail(&actor, &id, &mut repository, TimestampMillis::new(NOW)).await
            })
        })
        .await
}

async fn run_transition(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: TransitionCommand,
) -> Result<
    erpx_server_commerce::public::order::CommerceOrderReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgCommerceOrders::new(connection);
                transition_order(&actor, &command, &mut repository, TimestampMillis::new(NOW)).await
            })
        })
        .await
}

async fn run_error(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: TransitionCommand,
) -> BusinessError {
    run_transition(pool, actor, command)
        .await
        .map_err(business)
        .unwrap_err()
}

#[tokio::test(flavor = "multi_thread")]
async fn read_is_company_and_branch_scoped_and_requires_current_iam() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let document = run_detail(&pool, creator(), "quote-draft").await.unwrap();
        assert_eq!(document.order.id, "quote-draft");
        assert_eq!(document.order.party_id, "party-1");
        assert_eq!(document.order.state, "draft");
        assert_eq!(document.lines.len(), 1);
        assert_eq!(document.lines[0].sku, "SVC-1");

        let error = run_detail(&pool, other_branch(), "quote-draft")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "ORDER_NOT_FOUND");
        let error = run_detail(&pool, creator(), "other-branch-quote")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "ORDER_NOT_FOUND");
        let error = run_detail(&pool, creator(), "foreign-quote")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "ORDER_NOT_FOUND");
        let error = run_detail(&pool, creator(), "purchase-draft")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "ORDER_NOT_FOUND");

        let revoked = actor(
            "revoked",
            "sales_operator",
            "assignment-revoked",
            Some("branch"),
            &["commerce.read", "commerce.sales"],
        );
        let error = run_detail(&pool, revoked, "quote-draft")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "COMMERCE_PERMISSION_REQUIRED");

        let foreign_document = run_detail(&pool, foreign(), "foreign-quote").await.unwrap();
        assert_eq!(foreign_document.order.company_id, "foreign");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_persists_exact_receipt_audit_outbox_and_replays() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key = "commerce-order-submit-0001";
        let receipt = run_transition(
            &pool,
            creator(),
            command("quote-draft", OrderAction::Submit, 1, key),
        )
        .await
        .unwrap();
        assert_eq!(receipt.id, "quote-draft");
        assert_eq!(receipt.state, "submitted");
        assert_eq!(receipt.version, 2);
        assert_eq!(receipt.sales_id, None);

        let row = sqlx::query("SELECT state,version,approved_by FROM commerce_orders WHERE id='quote-draft'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("state"), "submitted");
        assert_eq!(row.get::<i64, _>("version"), 2);
        assert_eq!(row.get::<Option<String>, _>("approved_by"), None);

        let receipt_row = sqlx::query(
            "SELECT operation,actor_id,record_id,result_json FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt_row.get::<String, _>("operation"), "commerce.order.submit");
        assert_eq!(receipt_row.get::<String, _>("actor_id"), "creator");
        assert_eq!(receipt_row.get::<String, _>("record_id"), "quote-draft");
        assert_eq!(
            receipt_row.get::<String, _>("result_json"),
            json!({"id":"quote-draft","state":"submitted","version":2,"salesId":null}).to_string()
        );
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
             (SELECT count(*) FROM audit_events WHERE command_id=$1),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
        let payload: String =
            sqlx::query_scalar("SELECT payload_json FROM outbox_events WHERE command_id=$1")
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["state"], "submitted");
        assert_eq!(payload["companyId"], "company");
        assert_eq!(payload["branchId"], "branch");
        assert_eq!(payload["version"], 2);
        assert_eq!(payload["salesId"], serde_json::Value::Null);

        let replay = run_transition(
            &pool,
            creator(),
            command("quote-draft", OrderAction::Submit, 1, key),
        )
        .await
        .unwrap();
        assert_eq!(replay, receipt);
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
             (SELECT count(*) FROM audit_events WHERE command_id=$1),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));

        let error = run_error(
            &pool,
            creator(),
            TransitionCommand {
                reason: Some("different request".into()),
                ..command("quote-draft", OrderAction::Submit, 1, key)
            },
        )
        .await;
        assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn approve_discount_and_cancel_paths_preserve_independence_and_guards() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let error = run_error(
            &pool,
            creator(),
            command(
                "quote-discount",
                OrderAction::Approve,
                2,
                "commerce-order-approve-0001",
            ),
        )
        .await;
        assert_eq!(error.code, "COMMERCE_APPROVAL_REQUIRED");

        let error = run_error(
            &pool,
            reviewer(),
            command(
                "quote-discount",
                OrderAction::Approve,
                2,
                "commerce-order-approve-0002",
            ),
        )
        .await;
        assert_eq!(error.code, "DISCOUNT_APPROVAL_REASON_REQUIRED");

        let receipt = run_transition(
            &pool,
            reviewer(),
            TransitionCommand {
                reason: Some("Independent discount review".into()),
                ..command(
                    "quote-discount",
                    OrderAction::Approve,
                    2,
                    "commerce-order-approve-0003",
                )
            },
        )
        .await
        .unwrap();
        assert_eq!(receipt.state, "confirmed");
        let row = sqlx::query(
            "SELECT state,version,approved_by,discount_approved_by,discount_approved_at \
             FROM commerce_orders WHERE id='quote-discount'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("state"), "confirmed");
        assert_eq!(row.get::<i64, _>("version"), 3);
        assert_eq!(
            row.get::<Option<String>, _>("approved_by").as_deref(),
            Some("reviewer")
        );
        assert_eq!(
            row.get::<Option<String>, _>("discount_approved_by")
                .as_deref(),
            Some("reviewer")
        );
        assert_eq!(row.get::<Option<i64>, _>("discount_approved_at"), Some(NOW));

        let error = run_error(
            &pool,
            reviewer(),
            command(
                "sales-submitted",
                OrderAction::Approve,
                2,
                "commerce-order-approve-0004",
            ),
        )
        .await;
        assert_eq!(error.code, "COMMERCE_SALES_APPROVAL_FINANCE_GUARD");

        let cancelled = run_transition(
            &pool,
            reviewer(),
            command(
                "quote-confirmed",
                OrderAction::Cancel,
                3,
                "commerce-order-cancel-0001",
            ),
        )
        .await
        .unwrap();
        assert_eq!(cancelled.state, "cancelled");
        assert_eq!(cancelled.version, 4);

        let error = run_error(
            &pool,
            reviewer(),
            command(
                "quote-confirmed-fulfilled",
                OrderAction::Cancel,
                4,
                "commerce-order-cancel-0002",
            ),
        )
        .await;
        assert_eq!(error.code, "ORDER_REVERSAL_REQUIRED");

        let error = run_error(
            &pool,
            reviewer(),
            command(
                "sales-confirmed",
                OrderAction::Cancel,
                3,
                "commerce-order-cancel-0003",
            ),
        )
        .await;
        assert_eq!(error.code, "ORDER_REVERSAL_REQUIRED");

        let cancelled = run_transition(
            &pool,
            creator(),
            command(
                "sales-draft",
                OrderAction::Cancel,
                1,
                "commerce-order-cancel-0004",
            ),
        )
        .await
        .unwrap();
        assert_eq!(cancelled.state, "cancelled");
        assert_eq!(cancelled.version, 2);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn convert_copies_the_quote_into_one_draft_sales_order() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key = "commerce-order-convert-0001";
        let receipt = run_transition(
            &pool,
            creator(),
            command("quote-confirmed", OrderAction::Convert, 3, key),
        )
        .await
        .unwrap();
        assert_eq!(receipt.id, "quote-confirmed");
        assert_eq!(receipt.state, "converted");
        assert_eq!(receipt.version, 4);
        assert_eq!(
            receipt.sales_id.as_deref(),
            Some("order-commerce-order-convert-0001")
        );

        let sales_id = receipt.sales_id.clone().unwrap();
        let quote =
            sqlx::query("SELECT state,version FROM commerce_orders WHERE id='quote-confirmed'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(quote.get::<String, _>("state"), "converted");
        assert_eq!(quote.get::<i64, _>("version"), 4);

        let sales = sqlx::query(
            "SELECT company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,\
             due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,\
             created_at,updated_at FROM commerce_orders WHERE id=$1",
        )
        .bind(&sales_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(sales.get::<String, _>("company_id"), "company");
        assert_eq!(sales.get::<String, _>("branch_id"), "branch");
        assert_eq!(sales.get::<String, _>("kind"), "sales");
        assert!(sales.get::<String, _>("number").starts_with("SO-"));
        assert_eq!(sales.get::<String, _>("party_id"), "party-1");
        assert_eq!(sales.get::<String, _>("warehouse_id"), "warehouse-1");
        assert_eq!(sales.get::<String, _>("state"), "draft");
        assert_eq!(sales.get::<i64, _>("version"), 1);
        assert_eq!(sales.get::<String, _>("source_id"), "quote-confirmed");
        assert_eq!(sales.get::<String, _>("created_by"), "creator");
        assert_eq!(sales.get::<Option<String>, _>("approved_by"), None);
        assert_eq!(sales.get::<i64, _>("created_at"), NOW);
        assert_eq!(sales.get::<i64, _>("updated_at"), NOW);

        let lines: Vec<(String, i64, i64, String)> = sqlx::query_as(
            "SELECT id,quantity_milli,subtotal_minor,item_type FROM commerce_order_lines \
             WHERE order_id=$1 ORDER BY ordinal",
        )
        .bind(&sales_id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![(format!("{sales_id}-0"), 1000, 10000, "service".into())]
        );

        let replay = run_transition(
            &pool,
            creator(),
            command("quote-confirmed", OrderAction::Convert, 3, key),
        )
        .await
        .unwrap();
        assert_eq!(replay, receipt);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM commerce_orders WHERE source_id='quote-confirmed'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 1);

        let error = run_error(
            &pool,
            creator(),
            command(
                "quote-confirmed",
                OrderAction::Convert,
                3,
                "commerce-order-convert-0002",
            ),
        )
        .await;
        assert_eq!(error.code, "COMMERCE_ORDER_CHANGED");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn late_outbox_failure_rolls_back_and_exact_retry_persists_once() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "CREATE FUNCTION fail_commerce_order_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN IF NEW.command_id='commerce-order-rollback-0001' THEN \
             RAISE EXCEPTION 'blocked outbox'; END IF; RETURN NEW; END $$; \
             CREATE TRIGGER fail_commerce_order_outbox BEFORE INSERT ON outbox_events \
             FOR EACH ROW EXECUTE FUNCTION fail_commerce_order_outbox();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let result = run_transition(
            &pool,
            creator(),
            command("sales-draft", OrderAction::Submit, 1, "commerce-order-rollback-0001"),
        )
        .await;
        assert!(result.is_err());
        let row = sqlx::query("SELECT state,version FROM commerce_orders WHERE id='sales-draft'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("state"), "draft");
        assert_eq!(row.get::<i64, _>("version"), 1);
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='commerce-order-rollback-0001'),\
             (SELECT count(*) FROM audit_events WHERE command_id='commerce-order-rollback-0001'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='commerce-order-rollback-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0));

        sqlx::raw_sql(
            "DROP TRIGGER fail_commerce_order_outbox ON outbox_events; \
             DROP FUNCTION fail_commerce_order_outbox();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let receipt = run_transition(
            &pool,
            creator(),
            command("sales-draft", OrderAction::Submit, 1, "commerce-order-rollback-0001"),
        )
        .await
        .unwrap();
        assert_eq!(receipt.state, "submitted");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='commerce-order-rollback-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn transaction_and_serializable_guards_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut repository = PgCommerceOrders::new(&mut connection);
        let error = CommerceOrderReadRepository::order(&mut repository, &creator(), "quote-draft")
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                PgTransactionError::Persistence(ref value)
                    if value.to_string().contains("COMMERCE_ORDER_TRANSACTION_REQUIRED")
            ),
            "{error:?}"
        );

        let mut transaction = pool.raw().begin().await.unwrap();
        let mut repository = PgCommerceOrders::new(&mut transaction);
        let error = CommerceOrderReadRepository::order(&mut repository, &creator(), "quote-draft")
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                PgTransactionError::Persistence(ref value)
                    if value.to_string().contains("COMMERCE_ORDER_SERIALIZABLE_REQUIRED")
            ),
            "{error:?}"
        );
        transaction.rollback().await.unwrap();
    })
    .await;
}
