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

#[tokio::test]
async fn commerce_price_guard_matches_actual_typescript_sql() {
    use erpx_server_sales::public::commerce_order_price_guard::{lock_commerce_order_price_source,observe_commerce_order_price_guard};
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source=root.join("packages/server-sales/src/adapters/commerce-facts.ts");
    let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const db={prepare(sql){return{bind(...values){return{sql,values}}}}};const s=m.CommerceSalesFacts(db).guardRule18({key:'price-oracle',guardId:'price-list',values:['list','company',3,'2026-11-01','2026-11-01']});process.stdout.write(JSON.stringify(s));"#;
    let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(source).output().unwrap();
    assert!(oracle.status.success(),"actual Sales TS oracle: {}",String::from_utf8_lossy(&oracle.stderr));
    let step:serde_json::Value=serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(step["values"],serde_json::json!(["price-oracle","price-list","list","company",3,"2026-11-01","2026-11-01"]));
    let raw=step["sql"].as_str().unwrap();assert_eq!(raw.matches('?').count(),7);
    let mut n=0;let sql:String=raw.chars().flat_map(|ch|if ch=='?'{n+=1;format!("${n}").chars().collect::<Vec<_>>()}else{vec![ch]}).collect();
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,status,version,created_by,prepared_by,approved_by,approval_reason,activated_at,created_at,updated_at) VALUES('list','company','List','THB','customer','2026-11-01','2026-11-30','active',3,'creator','creator','reviewer','Approved',1,1,1),('foreign-list','foreign','Foreign','USD','customer-foreign','2026-11-01','2026-11-30','active',3,'creator','creator','reviewer','Approved',1,1,1),('draft-list','company','Draft','THB',NULL,'2026-11-01','2026-11-30','draft',1,'creator','creator',NULL,NULL,NULL,1,1)").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_commerce_order_price_source(&mut tx).await.unwrap();
        for (id,version,date,expected) in [("list",3_i64,"2026-11-01",true),("list",3,"2026-11-30",true),("list",3,"2026-10-31",false),("list",3,"2026-12-01",false),("list",4,"2026-11-15",false),("foreign-list",3,"2026-11-15",false),("draft-list",1,"2026-11-15",false),("missing",1,"2026-11-15",false)] {
            let guard=observe_commerce_order_price_guard(&mut tx,&company,id,version,date).await.unwrap();assert_eq!(guard.matched(),expected);guard.require_current(&mut tx).await.unwrap();
            sqlx::query("SAVEPOINT price_oracle").execute(&mut *tx).await.unwrap();
            let result=sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind("price-oracle").bind("price-list").bind(id).bind("company").bind(version).bind(date).bind(date).execute(&mut *tx).await;
            if expected {assert_eq!(result.unwrap().rows_affected(),1)} else {assert_eq!(result.unwrap_err().as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));}
            sqlx::query("ROLLBACK TO SAVEPOINT price_oracle").execute(&mut *tx).await.unwrap();
        }
        let held=observe_commerce_order_price_guard(&mut tx,&company,"list",3,"2026-11-15").await.unwrap();
        sqlx::query("SAVEPOINT price_change").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.sales_price_lists SET status='superseded',version=version+1 WHERE id='list'").execute(&mut *tx).await.unwrap();assert!(held.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT price_change").execute(&mut *tx).await.unwrap();held.require_current(&mut tx).await.unwrap();
        let limit="x".repeat(8192);assert!(!observe_commerce_order_price_guard(&mut tx,&company,&limit,3,"2026-11-15").await.unwrap().matched());assert!(observe_commerce_order_price_guard(&mut tx,&company,&(limit.clone()+"x"),3,"2026-11-15").await.is_err());assert!(observe_commerce_order_price_guard(&mut tx,&company,"missing",3,&(limit+"x")).await.is_err());
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();let (send,receive)=tokio::sync::oneshot::channel();let writer_pool=pool.clone();
        let writer=tokio::spawn(async move{let mut w=writer_pool.raw().begin().await.unwrap();sqlx::query("SET LOCAL lock_timeout='3s'").execute(&mut *w).await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *w).await.unwrap();send.send(pid).unwrap();let e=sqlx::query("UPDATE public.sales_price_lists SET name='blocked' WHERE id='draft-list'").execute(&mut *w).await.unwrap_err();assert_eq!(e.as_database_error().and_then(|x|x.code()).as_deref(),Some("55P03"));w.rollback().await.unwrap();});
        let writer_pid=receive.await.unwrap();tokio::time::timeout(std::time::Duration::from_secs(2),async{loop{let waiting:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2)) AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=$2 AND relation='public.sales_price_lists'::regclass AND mode='RowExclusiveLock' AND NOT granted)").bind(reader_pid).bind(writer_pid).fetch_one(&mut *tx).await.unwrap();if waiting{break}tokio::task::yield_now().await;}}).await.expect("writer blocked by price source fence");writer.await.unwrap();tx.rollback().await.unwrap();
        let mut fresh=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_commerce_order_price_source(&mut fresh).await.unwrap();assert!(held.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut save=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *save).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *save).await.unwrap();sqlx::query("SAVEPOINT before_price_lock").execute(&mut *save).await.unwrap();lock_commerce_order_price_source(&mut save).await.unwrap();let retained=observe_commerce_order_price_guard(&mut save,&company,"list",3,"2026-11-15").await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_price_lock").execute(&mut *save).await.unwrap();assert!(retained.require_current(&mut save).await.is_err());save.rollback().await.unwrap();
        let mut shadow=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *shadow).await.unwrap();sqlx::raw_sql("CREATE TEMP TABLE sales_price_lists(id text,company_id text,version bigint,status text,valid_from text,valid_until text);INSERT INTO sales_price_lists VALUES('missing','company',1,'active','2026-01-01','2026-12-31')").execute(&mut *shadow).await.unwrap();lock_commerce_order_price_source(&mut shadow).await.unwrap();let proof=observe_commerce_order_price_guard(&mut shadow,&company,"missing",1,"2026-11-15").await.unwrap();assert!(!proof.matched());sqlx::query("SET LOCAL search_path=pg_catalog,public").execute(&mut *shadow).await.unwrap();assert!(proof.require_current(&mut shadow).await.is_err());shadow.rollback().await.unwrap();
        let mut rls=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *rls).await.unwrap();let bypass:bool=sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *rls).await.unwrap();assert!(!bypass);sqlx::raw_sql("ALTER TABLE public.sales_price_lists ENABLE ROW LEVEL SECURITY; ALTER TABLE public.sales_price_lists FORCE ROW LEVEL SECURITY").execute(&mut *rls).await.unwrap();let active:bool=sqlx::query_scalar("SELECT row_security_active('public.sales_price_lists'::regclass)").fetch_one(&mut *rls).await.unwrap();assert!(active);lock_commerce_order_price_source(&mut rls).await.unwrap();assert!(observe_commerce_order_price_guard(&mut rls,&company,"list",3,"2026-11-15").await.is_err());rls.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commerce_selected_pricing_matches_actual_typescript_and_retains_absence() {
    use erpx_server_sales::public::commerce_selected_pricing::{inspect_commerce_selected_pricing,lock_commerce_selected_pricing};
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script=r#"
if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');
const {commercialOwnerProjectionSources:commerceProjectionSources}=await import('./packages/server-commerce/src/adapters/commercial-owner-projection-sources.ts');
const transpiler=new Bun.Transpiler({loader:'ts'});
const extract=(source,name)=>{const start=source.indexOf(`export function ${name}(`);if(start<0)throw Error(`${name}_SOURCE_MISSING`);return source.slice(start);};
const domain=await Bun.file('./packages/server-sales/src/domain/pricing.ts').text();
const domainFn=extract(domain,'evaluateSalesDiscounts').replace(/^export /,'');
const evaluateSalesDiscounts=Function('decimalToMinor','BusinessRuleError',transpiler.transformSync(domainFn)+'\nreturn evaluateSalesDiscounts;')(()=>{throw Error('UNEXPECTED_PRICE_EVALUATION')},class extends Error{});
const application=await Bun.file('./packages/server-sales/src/application/pricing.ts').text();
const applicationFn=extract(application,'createSalesPricingApplication').replace(/^export /,'');
const createSalesPricingApplication=Function('evaluateSalesDiscounts','BusinessRuleError',transpiler.transformSync(applicationFn)+'\nreturn createSalesPricingApplication;')(evaluateSalesDiscounts,class extends Error{});
const adapter=await Bun.file('./packages/server-sales/src/adapters/pricing.ts').text();
const start=adapter.indexOf('export async function resolveSalesPricing(');if(start<0)throw Error('SALES_RESOLVER_SOURCE_MISSING');
const resolver=adapter.slice(start).replace(/^export /,'');
const resolveSalesPricing=Function('createSalesPricingApplication','commerceProjectionSources','BusinessRuleError','ApiError',transpiler.transformSync(resolver)+'\nreturn resolveSalesPricing;')(createSalesPricingApplication,commerceProjectionSources,class extends Error{},class extends Error{});
const statements=[];
const db={prepare(sql){return{bind(...args){return{first(){statements.push({sql,args});return Promise.resolve({id:'specific',max_auto_discount_bps:0})},all(){statements.push({sql,args});return Promise.resolve({results:[]})}}}}}};
for(const id of [undefined,'specific'])await resolveSalesPricing(db,{companyId:'company'},{kind:'quote',partyId:'customer',orderDate:'2026-11-15',priceListId:id,lines:[]});process.stdout.write(JSON.stringify(statements));
"#;
    let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).output().unwrap();assert!(out.status.success(),"actual Sales TS oracle unavailable: {}",String::from_utf8_lossy(&out.stderr));let oracle:Vec<serde_json::Value>=serde_json::from_slice(&out.stdout).unwrap();assert_eq!(oracle.len(),4);
    let positional=|s:&str|{let mut n=0;s.chars().flat_map(|ch|if ch=='?'{n+=1;format!("${n}").chars().collect::<Vec<_>>()}else{vec![ch]}).collect::<String>()};
    let default_sql=positional(oracle[0]["sql"].as_str().unwrap());let explicit_sql=positional(oracle[2]["sql"].as_str().unwrap());let items_sql=positional(oracle[3]["sql"].as_str().unwrap());
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,status,version,created_by,prepared_by,approved_by,approval_reason,activated_at,created_at,updated_at) VALUES('generic','company','Generic','THB',NULL,'2026-11-01','2026-11-30','draft',1,'creator','creator',NULL,NULL,NULL,1,1),('specific','company','Specific','THB','customer','2026-11-01','2026-11-30','draft',1,'creator','creator',NULL,NULL,NULL,1,1),('foreign-list','foreign','Foreign','USD','customer-foreign','2026-11-01','2026-11-30','draft',1,'creator','creator',NULL,NULL,NULL,1,1); INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES('item-g','generic','product-1',9000,700,0),('item-s','specific','product-1',10000,700,0),('item-s2','specific','product-2',20000,700,1); UPDATE sales_price_lists SET status='active',approved_by='reviewer',approval_reason='Approved',activated_at=2 WHERE id IN ('generic','specific','foreign-list')").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();assert!(inspect_commerce_selected_pricing(&mut tx,&company,"customer","2026-11-15","THB",None).await.is_err());tx.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_commerce_selected_pricing(&mut tx).await.unwrap();
        let source=inspect_commerce_selected_pricing(&mut tx,&company,"customer","2026-11-15","THB",None).await.unwrap();let value=source.value().unwrap();
        let sql=format!("SELECT to_jsonb(q) FROM ({default_sql}) q");let original:serde_json::Value=sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind("company").bind("company").bind("2026-11-15").bind("2026-11-15").bind("customer").bind("customer").fetch_one(&mut *tx).await.unwrap();assert_eq!(value.list,original);assert_eq!(value.list["id"],"specific");
        let sql=format!("SELECT to_jsonb(q) FROM ({explicit_sql}) q");let original:serde_json::Value=sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind("specific").bind("company").bind("company").bind("2026-11-15").bind("2026-11-15").bind("customer").fetch_one(&mut *tx).await.unwrap();let selected=inspect_commerce_selected_pricing(&mut tx,&company,"customer","2026-11-15","THB",Some("specific")).await.unwrap();assert_eq!(selected.value().unwrap().list,original);
        let sql=format!("SELECT to_jsonb(q) FROM ({items_sql}) q");let items:Vec<serde_json::Value>=sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind("specific").fetch_all(&mut *tx).await.unwrap();let by_product=|rows:&[serde_json::Value]|rows.iter().map(|v|(v["product_id"].as_str().unwrap().to_owned(),v.clone())).collect::<std::collections::BTreeMap<_,_>>();assert_eq!(items.len(),2);assert_eq!(by_product(&selected.value().unwrap().items),by_product(&items));
        assert!(inspect_commerce_selected_pricing(&mut tx,&company,"customer","2026-12-01","THB",None).await.unwrap().value().is_none());assert!(inspect_commerce_selected_pricing(&mut tx,&company,"customer","2026-11-15","USD",Some("specific")).await.unwrap().value().is_none());assert!(inspect_commerce_selected_pricing(&mut tx,&company,"customer","2026-11-15","THB",Some("foreign-list")).await.unwrap().value().is_none());
        let generic=inspect_commerce_selected_pricing(&mut tx,&company,"customer-inactive","2026-11-15","THB",None).await.unwrap();assert_eq!(generic.value().unwrap().list["id"],"generic");
        let foreign=CompanyId::new("foreign").unwrap();let absent=inspect_commerce_selected_pricing(&mut tx,&foreign,"customer-foreign","2026-11-15","USD",Some("later")).await.unwrap();assert!(absent.value().is_none());
        sqlx::query("INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,status,version,created_by,prepared_by,created_at,updated_at) VALUES('later','foreign','Later','USD',NULL,'2026-11-01','2026-11-30','draft',1,'creator','creator',1,1)").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE sales_price_lists SET status='active',approved_by='reviewer',approval_reason='Approved',activated_at=2 WHERE id='later'").execute(&mut *tx).await.unwrap();assert!(absent.require_current(&mut tx).await.is_err());
        assert!(inspect_commerce_selected_pricing(&mut tx,&company,&"x".repeat(8193),"2026-11-15","THB",None).await.is_err());
        sqlx::query("SAVEPOINT price_bytes").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE sales_price_lists SET status='superseded',version=version+1 WHERE id='later'").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,status,version,created_by,prepared_by,approved_by,approval_reason,activated_at,created_at,updated_at) VALUES('huge','foreign',repeat('x',2097153),'USD',NULL,'2026-11-01','2026-11-30','active',1,'creator','creator','reviewer','Approved',1,1,1)").execute(&mut *tx).await.unwrap();let too_large=inspect_commerce_selected_pricing(&mut tx,&foreign,"customer-foreign","2026-11-15","USD",Some("huge")).await;assert!(matches!(too_large,Err(PgTransactionError::Business(ref e)) if e.code=="SALES_PRICING_SOURCE_LIMIT"));sqlx::query("ROLLBACK TO SAVEPOINT price_bytes").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT immutable_item").execute(&mut *tx).await.unwrap();let item_edit=sqlx::query("UPDATE sales_price_list_items SET price_minor=999 WHERE id='item-s'").execute(&mut *tx).await.unwrap_err();assert_eq!(item_edit.as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));sqlx::query("ROLLBACK TO SAVEPOINT immutable_item").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE sales_price_lists SET status='superseded',version=version+1 WHERE id='specific'").execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_commerce_selected_pricing(&mut other).await.unwrap();assert!(source.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut save=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *save).await.unwrap();sqlx::query("SAVEPOINT before_price_fence").execute(&mut *save).await.unwrap();lock_commerce_selected_pricing(&mut save).await.unwrap();let retained=inspect_commerce_selected_pricing(&mut save,&company,"customer","2026-11-15","THB",None).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_price_fence").execute(&mut *save).await.unwrap();assert!(retained.require_current(&mut save).await.is_err());save.rollback().await.unwrap();
        let mut shadow=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *shadow).await.unwrap();sqlx::query("CREATE TEMP TABLE sales_price_lists(id text)").execute(&mut *shadow).await.unwrap();lock_commerce_selected_pricing(&mut shadow).await.unwrap();assert!(inspect_commerce_selected_pricing(&mut shadow,&company,"customer","2026-11-15","THB",None).await.is_err());shadow.rollback().await.unwrap();
        let mut rls=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *rls).await.unwrap();let bypass:bool=sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *rls).await.unwrap();assert!(!bypass);sqlx::raw_sql("ALTER TABLE public.sales_price_list_items ENABLE ROW LEVEL SECURITY; ALTER TABLE public.sales_price_list_items FORCE ROW LEVEL SECURITY").execute(&mut *rls).await.unwrap();assert!(sqlx::query_scalar::<_,bool>("SELECT row_security_active('public.sales_price_list_items'::regclass)").fetch_one(&mut *rls).await.unwrap());lock_commerce_selected_pricing(&mut rls).await.unwrap();assert!(inspect_commerce_selected_pricing(&mut rls,&company,"customer","2026-11-15","THB",None).await.is_err());rls.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE public.sales_price_list_items IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();let mut blocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();let error=lock_commerce_selected_pricing(&mut blocked).await.unwrap_err();assert!(matches!(error,PgTransactionError::Persistence(ref e) if e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}
