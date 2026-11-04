//! Native Sales price-list lifecycle on migrated PostgreSQL.
//!
//! Proves Company isolation, current IAM, source checks, expected-version CAS,
//! independent activation, idempotency, receipt/audit/outbox, rollback and
//! persisted readback. The fixture is a disposable leased erpx_dev_rust_*
//! database; no staging or production database is touched.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_sales::public::sales_price_lists::{
    PgPriceLists, PriceListActor, PriceListCommandRepository, PriceListDecision,
    PriceListDecisionAction, PriceListInput, PriceListItemInput, PriceListReceipt, PriceListUpdate,
    create_price_list, decide_price_list, update_price_list,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis, UserId,
};
use sqlx::Row;

const NOW: i64 = 1_800_000_000_000;

fn actor(
    company: &str,
    user: &str,
    role: &str,
    assignment: &str,
    branch: Option<&str>,
    permissions: &[&str],
) -> PriceListActor {
    PriceListActor {
        user_id: UserId::new(user).unwrap(),
        company_id: CompanyId::new(company).unwrap(),
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

fn creator() -> PriceListActor {
    actor(
        "company",
        "creator",
        "sales_operator",
        "assignment-creator",
        None,
        &["commerce.read", "commerce.sales"],
    )
}

fn reviewer() -> PriceListActor {
    actor(
        "company",
        "reviewer",
        "sales_manager",
        "assignment-reviewer",
        None,
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn foreign() -> PriceListActor {
    actor(
        "foreign",
        "foreign-user",
        "sales_operator",
        "assignment-foreign",
        None,
        &["commerce.read", "commerce.sales"],
    )
}

fn input(name: &str) -> PriceListInput {
    PriceListInput {
        name: name.into(),
        currency: "THB".into(),
        customer_id: Some("customer".into()),
        valid_from: "2026-11-01".into(),
        valid_until: "2026-11-30".into(),
        max_auto_discount_bps: 500,
        items: vec![
            PriceListItemInput {
                product_id: "product-1".into(),
                price_minor: 10_000,
                tax_bps: 700,
            },
            PriceListItemInput {
                product_id: "product-2".into(),
                price_minor: 20_000,
                tax_bps: 700,
            },
        ],
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
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
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','creator','local:creator','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1),\
         ('company','revoked','local:revoked','active','local',1),\
         ('company','branch-user','local:branch-user','active','local',1),\
         ('foreign','foreign-user','local:foreign-user','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-creator','creator','sales_operator','[]','company',NULL,0,9000000000000,NULL,'holding',NULL,'manual'),\
         ('assignment-reviewer','reviewer','sales_manager','[]','company',NULL,0,9000000000000,NULL,'holding',NULL,'manual'),\
         ('assignment-revoked','revoked','sales_operator','[]','company',NULL,0,9000000000000,1,'holding',NULL,'manual'),\
         ('assignment-branch','branch-user','sales_operator','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-foreign','foreign-user','sales_operator','[]','foreign',NULL,0,9000000000000,NULL,'holding',NULL,'manual');\
         INSERT INTO finance_charts(id,company_id,name,base_currency,status,version,created_by,created_at,updated_at) VALUES \
         ('chart-company','company','Company chart','THB','active',1,'system',1,1),\
         ('chart-foreign','foreign','Foreign chart','USD','active',1,'system',1,1);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,status,version,created_at,updated_at) VALUES \
         ('account-company','chart-company','company','1000','Cash','asset','active',1,1,1),\
         ('account-foreign','chart-foreign','foreign','1000','Cash','asset','active',1,1,1);\
         INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES \
         ('company','chart-company','THB','account-company','account-company','account-company','account-company','account-company','account-company','account-company','account-company','account-company'),\
         ('foreign','chart-foreign','USD','account-foreign','account-foreign','account-foreign','account-foreign','account-foreign','account-foreign','account-foreign','account-foreign','account-foreign');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES \
         ('customer','company','customer','C1','Customer','THB',30,0,'active',1,1,1),\
         ('customer-inactive','company','customer','C2','Inactive customer','THB',30,0,'blocked',1,1,1),\
         ('customer-foreign','foreign','customer','CF','Foreign customer','USD',30,0,'active',1,1,1),\
         ('supplier','company','supplier','S1','Supplier','THB',30,0,'active',1,1,1);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('product-1','company','P1','Product one','each',10000,700,'active',3),\
         ('product-2','company','P2','Product two','each',20000,700,'active',3),\
         ('product-inactive','company','P3','Inactive product','each',10000,700,'archived',1),\
         ('product-foreign','foreign','PF','Foreign product','each',10000,700,'active',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run_create(
    pool: &DatabasePool,
    actor: PriceListActor,
    input: PriceListInput,
    key: CommandKey,
) -> Result<PriceListReceipt, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let input = input.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut repository = PgPriceLists::new(connection);
                create_price_list(
                    &actor,
                    input,
                    &key,
                    &mut repository,
                    TimestampMillis::new(NOW),
                )
                .await
            })
        })
        .await
}

async fn run_create_error(
    pool: &DatabasePool,
    actor: PriceListActor,
    input: PriceListInput,
    key: CommandKey,
) -> BusinessError {
    run_create(pool, actor, input, key)
        .await
        .map_err(business)
        .unwrap_err()
}

async fn run_update(
    pool: &DatabasePool,
    actor: PriceListActor,
    id: &str,
    input: PriceListUpdate,
    key: CommandKey,
) -> Result<PriceListReceipt, PgTransactionError<BusinessError>> {
    let id = id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let id = id.clone();
            let input = input.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut repository = PgPriceLists::new(connection);
                update_price_list(
                    &actor,
                    &id,
                    input,
                    &key,
                    &mut repository,
                    TimestampMillis::new(NOW),
                )
                .await
            })
        })
        .await
}

async fn run_update_error(
    pool: &DatabasePool,
    actor: PriceListActor,
    id: &str,
    input: PriceListUpdate,
    key: CommandKey,
) -> BusinessError {
    run_update(pool, actor, id, input, key)
        .await
        .map_err(business)
        .unwrap_err()
}

async fn run_decide(
    pool: &DatabasePool,
    actor: PriceListActor,
    id: &str,
    decision: PriceListDecision,
    key: CommandKey,
) -> Result<PriceListReceipt, PgTransactionError<BusinessError>> {
    let id = id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let id = id.clone();
            let decision = decision.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut repository = PgPriceLists::new(connection);
                decide_price_list(
                    &actor,
                    &id,
                    decision,
                    &key,
                    &mut repository,
                    TimestampMillis::new(NOW),
                )
                .await
            })
        })
        .await
}

async fn run_decide_error(
    pool: &DatabasePool,
    actor: PriceListActor,
    id: &str,
    decision: PriceListDecision,
    key: CommandKey,
) -> BusinessError {
    run_decide(pool, actor, id, decision, key)
        .await
        .map_err(business)
        .unwrap_err()
}

fn decision(action: PriceListDecisionAction, version: i64) -> PriceListDecision {
    PriceListDecision {
        expected_version: version,
        action,
        reason: "Independent pricing review".into(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn create_update_activate_and_cancel_persist_exact_readback_and_replay() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let created = run_create(
            &pool,
            creator(),
            input("Standard pricing"),
            key("sales-price-list-create-0001"),
        )
        .await
        .unwrap();
        let id = created.id.clone();
        assert_eq!(id, "price-list-sales-price-list-create-0001");
        assert_eq!(created.status, "draft");
        assert_eq!(created.version, 1);

        let row = sqlx::query(
            "SELECT name,currency,customer_id,status,version,created_by,prepared_by,approved_by,\
             activated_at FROM sales_price_lists WHERE id=$1 AND company_id='company'",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("name"), "Standard pricing");
        assert_eq!(row.get::<String, _>("currency"), "THB");
        assert_eq!(row.get::<String, _>("status"), "draft");
        assert_eq!(row.get::<i64, _>("version"), 1);
        assert_eq!(row.get::<String, _>("created_by"), "creator");
        assert_eq!(row.get::<String, _>("prepared_by"), "creator");
        assert_eq!(row.get::<Option<String>, _>("approved_by"), None);
        let items: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT product_id,price_minor,ordinal FROM sales_price_list_items \
             WHERE price_list_id=$1 ORDER BY ordinal",
        )
        .bind(&id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            items,
            vec![
                ("product-1".into(), 10_000, 0),
                ("product-2".into(), 20_000, 1)
            ]
        );
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
             (SELECT count(*) FROM audit_events WHERE command_id=$1),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind("sales-price-list-create-0001")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));

        let replay = run_create(
            &pool,
            creator(),
            input("Standard pricing"),
            key("sales-price-list-create-0001"),
        )
        .await
        .unwrap();
        assert_eq!(replay, created);
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
             (SELECT count(*) FROM audit_events WHERE command_id=$1),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind("sales-price-list-create-0001")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));

        let error = run_create_error(
            &pool,
            creator(),
            PriceListInput {
                name: "Different pricing".into(),
                ..input("Standard pricing")
            },
            key("sales-price-list-create-0001"),
        )
        .await;
        assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");

        let updated = run_update(
            &pool,
            creator(),
            &id,
            PriceListUpdate {
                input: input("Updated pricing"),
                expected_version: 1,
            },
            key("sales-price-list-update-0001"),
        )
        .await
        .unwrap();
        assert_eq!(updated.status, "draft");
        assert_eq!(updated.version, 2);
        let row = sqlx::query("SELECT name,version,prepared_by FROM sales_price_lists WHERE id=$1")
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("name"), "Updated pricing");
        assert_eq!(row.get::<i64, _>("version"), 2);
        assert_eq!(row.get::<String, _>("prepared_by"), "creator");
        let item_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM sales_price_list_items WHERE price_list_id=$1 ORDER BY ordinal",
        )
        .bind(&id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            item_ids,
            vec![format!("{id}-v2-item-0"), format!("{id}-v2-item-1")]
        );

        let error = run_update_error(
            &pool,
            creator(),
            &id,
            PriceListUpdate {
                input: input("Stale pricing"),
                expected_version: 1,
            },
            key("sales-price-list-update-0002"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_CHANGED");

        let activated = run_decide(
            &pool,
            reviewer(),
            &id,
            decision(PriceListDecisionAction::Activate, 2),
            key("sales-price-list-activate-0001"),
        )
        .await
        .unwrap();
        assert_eq!(activated.status, "active");
        assert_eq!(activated.version, 3);
        let row = sqlx::query(
            "SELECT status,version,approved_by,approval_reason,activated_at,prepared_by \
             FROM sales_price_lists WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("status"), "active");
        assert_eq!(row.get::<i64, _>("version"), 3);
        assert_eq!(
            row.get::<Option<String>, _>("approved_by").as_deref(),
            Some("reviewer")
        );
        assert_eq!(
            row.get::<Option<String>, _>("approval_reason").as_deref(),
            Some("Independent pricing review")
        );
        assert_eq!(row.get::<Option<i64>, _>("activated_at"), Some(NOW));
        assert_eq!(row.get::<String, _>("prepared_by"), "creator");

        let replay = run_decide(
            &pool,
            reviewer(),
            &id,
            decision(PriceListDecisionAction::Activate, 2),
            key("sales-price-list-activate-0001"),
        )
        .await
        .unwrap();
        assert_eq!(replay, activated);

        let error = run_update_error(
            &pool,
            creator(),
            &id,
            PriceListUpdate {
                input: input("Locked pricing"),
                expected_version: 3,
            },
            key("sales-price-list-update-0003"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_LOCKED");

        let second = run_create(
            &pool,
            creator(),
            input("Second pricing"),
            key("sales-price-list-create-0002"),
        )
        .await
        .unwrap();
        let error = run_decide_error(
            &pool,
            creator(),
            &second.id,
            decision(PriceListDecisionAction::Activate, 1),
            key("sales-price-list-activate-0002"),
        )
        .await;
        assert_eq!(error.code, "SALES_PRICING_APPROVAL_REQUIRED");
        let cancelled = run_decide(
            &pool,
            reviewer(),
            &second.id,
            PriceListDecision {
                expected_version: 1,
                action: PriceListDecisionAction::Cancel,
                reason: "Superseded before activation".into(),
            },
            key("sales-price-list-cancel-0001"),
        )
        .await
        .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(cancelled.version, 2);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn company_scope_iam_and_source_negatives_leave_no_rows() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let base_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM sales_price_lists")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(base_rows, 0);

        let branch = actor(
            "company",
            "branch-user",
            "sales_operator",
            "assignment-branch",
            Some("branch"),
            &["commerce.read", "commerce.sales"],
        );
        let error = run_create_error(
            &pool,
            branch,
            input("Branch pricing"),
            key("sales-price-list-branch-0001"),
        )
        .await;
        assert_eq!(error.code, "COMPANY_PRICING_SCOPE_REQUIRED");

        let revoked = actor(
            "company",
            "revoked",
            "sales_operator",
            "assignment-revoked",
            None,
            &["commerce.read", "commerce.sales"],
        );
        let error = run_create_error(
            &pool,
            revoked,
            input("Revoked pricing"),
            key("sales-price-list-revoked-0001"),
        )
        .await;
        assert_eq!(error.code, "SALES_PRICING_PERMISSION_REQUIRED");

        let mut no_read = creator();
        no_read.permissions = vec!["commerce.sales".into()];
        let error = run_create_error(
            &pool,
            no_read,
            input("No read pricing"),
            key("sales-price-list-permission-0001"),
        )
        .await;
        assert_eq!(error.code, "SALES_PRICING_PERMISSION_REQUIRED");

        let mut usd = input("Currency mismatch");
        usd.currency = "USD".into();
        let error =
            run_create_error(&pool, creator(), usd, key("sales-price-list-currency-0001")).await;
        assert_eq!(error.code, "PRICE_LIST_CURRENCY_INVALID");

        let mut inactive_customer = input("Inactive customer");
        inactive_customer.customer_id = Some("customer-inactive".into());
        let error = run_create_error(
            &pool,
            creator(),
            inactive_customer,
            key("sales-price-list-customer-0001"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_CUSTOMER_INVALID");

        let mut foreign_customer = input("Foreign customer");
        foreign_customer.customer_id = Some("customer-foreign".into());
        let error = run_create_error(
            &pool,
            creator(),
            foreign_customer,
            key("sales-price-list-customer-0002"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_CUSTOMER_INVALID");

        let mut supplier = input("Supplier is not a customer");
        supplier.customer_id = Some("supplier".into());
        let error = run_create_error(
            &pool,
            creator(),
            supplier,
            key("sales-price-list-customer-0003"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_CUSTOMER_INVALID");

        let mut inactive_product = input("Inactive product");
        inactive_product.items[0].product_id = "product-inactive".into();
        let error = run_create_error(
            &pool,
            creator(),
            inactive_product,
            key("sales-price-list-product-0001"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_PRODUCT_INVALID");

        let mut foreign_product = input("Foreign product");
        foreign_product.items[0].product_id = "product-foreign".into();
        let error = run_create_error(
            &pool,
            creator(),
            foreign_product,
            key("sales-price-list-product-0002"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_PRODUCT_INVALID");

        let mut duplicate = input("Duplicate product");
        duplicate.items[1].product_id = "product-1".into();
        let error = run_create_error(
            &pool,
            creator(),
            duplicate,
            key("sales-price-list-product-0003"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_PRODUCT_DUPLICATE");

        let mut reversed = input("Reversed dates");
        reversed.valid_from = "2026-11-30".into();
        reversed.valid_until = "2026-11-01".into();
        let error = run_create_error(
            &pool,
            creator(),
            reversed,
            key("sales-price-list-dates-0001"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_DATES_INVALID");

        let mut too_many = input("Too many items");
        too_many.items = (0..501)
            .map(|index| PriceListItemInput {
                product_id: format!("product-{index}"),
                price_minor: 1_000,
                tax_bps: 700,
            })
            .collect();
        let error = run_create_error(
            &pool,
            creator(),
            too_many,
            key("sales-price-list-items-0001"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_REQUEST_INVALID");

        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM sales_price_lists),\
             (SELECT count(*) FROM sales_price_list_items),\
             (SELECT count(*) FROM command_receipts WHERE operation LIKE 'sales.price-list.%')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0));

        let foreign = run_create(
            &pool,
            foreign(),
            PriceListInput {
                currency: "USD".into(),
                customer_id: Some("customer-foreign".into()),
                items: vec![PriceListItemInput {
                    product_id: "product-foreign".into(),
                    price_minor: 5_000,
                    tax_bps: 700,
                }],
                ..input("Foreign pricing")
            },
            key("sales-price-list-foreign-0001"),
        )
        .await
        .unwrap();
        assert_eq!(foreign.status, "draft");
        let error = run_update_error(
            &pool,
            creator(),
            &foreign.id,
            PriceListUpdate {
                input: input("Cross-company update"),
                expected_version: 1,
            },
            key("sales-price-list-foreign-0002"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_NOT_FOUND");
        let error = run_decide_error(
            &pool,
            reviewer(),
            &foreign.id,
            decision(PriceListDecisionAction::Activate, 1),
            key("sales-price-list-foreign-0003"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_NOT_FOUND");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn independence_tracks_latest_preparer_and_update_source_drift_rolls_back() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let created = run_create(
            &pool,
            creator(),
            input("Independence pricing"),
            key("sales-price-list-independence-0001"),
        )
        .await
        .unwrap();
        let updated = run_update(
            &pool,
            reviewer(),
            &created.id,
            PriceListUpdate {
                input: input("Reviewer prepared pricing"),
                expected_version: 1,
            },
            key("sales-price-list-independence-0002"),
        )
        .await
        .unwrap();
        assert_eq!(updated.version, 2);
        let error = run_decide_error(
            &pool,
            reviewer(),
            &created.id,
            decision(PriceListDecisionAction::Activate, 2),
            key("sales-price-list-independence-0003"),
        )
        .await;
        assert_eq!(error.code, "INDEPENDENT_PRICING_APPROVER_REQUIRED");

        let updated = run_update(
            &pool,
            creator(),
            &created.id,
            PriceListUpdate {
                input: input("Creator prepared pricing"),
                expected_version: 2,
            },
            key("sales-price-list-independence-0004"),
        )
        .await
        .unwrap();
        assert_eq!(updated.version, 3);
        let activated = run_decide(
            &pool,
            reviewer(),
            &created.id,
            decision(PriceListDecisionAction::Activate, 3),
            key("sales-price-list-independence-0005"),
        )
        .await
        .unwrap();
        assert_eq!(activated.status, "active");
        assert_eq!(activated.version, 4);

        let draft = run_create(
            &pool,
            creator(),
            input("Source drift pricing"),
            key("sales-price-list-independence-0006"),
        )
        .await
        .unwrap();
        let before: (String, i64) =
            sqlx::query_as("SELECT name,version FROM sales_price_lists WHERE id=$1")
                .bind(&draft.id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let mut usd = input("Currency drift");
        usd.currency = "USD".into();
        let error = run_update_error(
            &pool,
            creator(),
            &draft.id,
            PriceListUpdate {
                input: usd,
                expected_version: 1,
            },
            key("sales-price-list-independence-0007"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_CURRENCY_INVALID");
        let mut foreign_product = input("Product drift");
        foreign_product.items[0].product_id = "product-foreign".into();
        let error = run_update_error(
            &pool,
            creator(),
            &draft.id,
            PriceListUpdate {
                input: foreign_product,
                expected_version: 1,
            },
            key("sales-price-list-independence-0008"),
        )
        .await;
        assert_eq!(error.code, "PRICE_LIST_PRODUCT_INVALID");
        let after: (String, i64) =
            sqlx::query_as("SELECT name,version FROM sales_price_lists WHERE id=$1")
                .bind(&draft.id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(after, before);
        let items: Vec<String> = sqlx::query_scalar(
            "SELECT product_id FROM sales_price_list_items WHERE price_list_id=$1 ORDER BY ordinal",
        )
        .bind(&draft.id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(items, vec!["product-1", "product-2"]);
        let failed_receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN \
             ('sales-price-list-independence-0007','sales-price-list-independence-0008')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(failed_receipts, 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn late_outbox_failure_rolls_back_the_whole_unit_and_exact_retry_succeeds() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "CREATE FUNCTION fail_sales_price_list_outbox() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'SYNTHETIC_PRICE_LIST_OUTBOX_FAILURE' USING ERRCODE='23514'; END $$;\
             CREATE TRIGGER fail_sales_price_list_outbox BEFORE INSERT ON outbox_events \
             FOR EACH ROW EXECUTE FUNCTION fail_sales_price_list_outbox();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = run_create(
            &pool,
            creator(),
            input("Rollback pricing"),
            key("sales-price-list-rollback-0001"),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, PgTransactionError::Persistence(_)),
            "{error:?}"
        );
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM sales_price_lists),\
             (SELECT count(*) FROM sales_price_list_items),\
             (SELECT count(*) FROM command_receipts WHERE command_id='sales-price-list-rollback-0001'),\
             (SELECT count(*) FROM audit_events WHERE command_id='sales-price-list-rollback-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0, 0));

        sqlx::raw_sql(
            "DROP TRIGGER fail_sales_price_list_outbox ON outbox_events;\
             DROP FUNCTION fail_sales_price_list_outbox();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let receipt = run_create(
            &pool,
            creator(),
            input("Rollback pricing"),
            key("sales-price-list-rollback-0001"),
        )
        .await
        .unwrap();
        assert_eq!(receipt.status, "draft");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='sales-price-list-rollback-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn transaction_and_serializable_guards_fail_before_owner_sql() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut repository = PgPriceLists::new(&mut connection);
        let error = PriceListCommandRepository::list(
            &mut repository,
            &CompanyId::new("company").unwrap(),
            "missing",
            false,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                error,
                PgTransactionError::Persistence(ref value)
                    if value.to_string().contains("SALES_PRICING_TRANSACTION_REQUIRED")
            ),
            "{error:?}"
        );

        let mut transaction = pool.raw().begin().await.unwrap();
        let mut repository = PgPriceLists::new(&mut transaction);
        let error = PriceListCommandRepository::current_authority(
            &mut repository,
            &creator(),
            false,
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                error,
                PgTransactionError::Persistence(ref value)
                    if value.to_string().contains("SALES_PRICING_SERIALIZABLE_REQUIRED")
            ),
            "{error:?}"
        );
        transaction.rollback().await.unwrap();
    })
    .await;
}
