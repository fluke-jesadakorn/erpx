//! Native Ecommerce catalog, channel draft and review on migrated PostgreSQL.
//! These tests prove owner SQL, Company isolation, exact CAS, command
//! idempotency, audit/outbox persistence, rollback and committed readback. They
//! do not prove a Next/browser journey or composition mounting.
use erpx_server_ecommerce::public::catalog::{
    EcommerceActor, PgEcommerceCatalogRead, storefront_catalog,
};
use erpx_server_ecommerce::public::channel::{CreateChannel, PgEcommerceCommand, create_channel};
use erpx_server_ecommerce::public::review::{
    ReviewAction, ReviewCommand, ReviewKind, review_ecommerce_record,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PoolLimits,
    TransactionOptions, connect_pool, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use serde_json::json;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{str::FromStr, time::Duration};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(company: &str, branch: Option<&str>) -> EcommerceActor {
    EcommerceActor {
        user_id: UserId::new("owner").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        role: Some("holding_owner".into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: vec![
            "commerce.read".into(),
            "commerce.sales".into(),
            "commerce.approve".into(),
        ],
        modules: vec!["ecommerce".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn create_command(key: &str) -> CreateChannel {
    CreateChannel {
        key: CommandKey::new(key).unwrap(),
        name: "  Storefront  ".into(),
        channel_type: "web".into(),
        configuration: json!({"storefront":"https://shop.example"}),
    }
}

fn review(kind: ReviewKind, action: ReviewAction, id: &str, version: i64) -> ReviewCommand {
    ReviewCommand {
        key: CommandKey::new(format!(
            "ecommerce-review-{}-{}-{}",
            kind.as_str(),
            action.as_str(),
            id
        ))
        .unwrap(),
        kind,
        id: RecordId::new(id).unwrap(),
        action,
        expected_version: version,
        reason: "Independent review of current terms".into(),
    }
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn setup(pool: &DatabasePool) {
    migrate(pool).await;
    sqlx::raw_sql(
        "INSERT INTO commerce_products\
         (id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('product-1','company','SKU-1','Online product','each',12500,0,'active',1),\
         ('product-2','company','SKU-2','Second online product','each',10000,0,'active',1),\
         ('product-f','foreign','SKU-F','Foreign product','each',9900,0,'active',1);\
         INSERT INTO ecommerce_channels\
         (id,company_id,name,channel_type,configuration_json,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('channel-draft','company','Draft store','web','{\"storefront\":\"https://draft.example\"}','draft','creator',NULL,1,100,300),\
         ('channel-active','company','Active store','web','{\"storefront\":\"https://active.example\"}','active','creator','reviewer',2,100,200),\
         ('channel-foreign','foreign','Foreign store','web','{}','draft','foreign',NULL,1,100,400);\
         INSERT INTO ecommerce_listings\
         (id,channel_id,product_id,title,currency,price_minor,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('listing-draft','channel-draft','product-1','Draft online product','THB',12500,'draft','creator',NULL,1,100,300),\
         ('listing-active','channel-active','product-1','Active online product','THB',12500,'published','creator','reviewer',2,100,200),\
         ('listing-foreign','channel-foreign','product-f','Foreign online product','USD',9900,'published','foreign','reviewer',1,100,400);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn count(pool: &DatabasePool, table: &str, filter: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table} WHERE {filter}");
    sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

fn with_database<F, Fut>(test: F)
where
    F: FnOnce(DatabasePool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    sqlx::test_block_on(async move {
        let admin_url = std::env::var("ERPX_TEST_ADMIN_URL")
            .expect("ERPX_TEST_ADMIN_URL required for isolated native Postgres tests");
        let admin_options = PgConnectOptions::from_str(&admin_url).expect("invalid test admin URL");
        assert_eq!(admin_options.get_host(), "127.0.0.1");
        assert_eq!(admin_options.get_port(), 5432);
        assert_eq!(admin_options.get_database(), Some("postgres"));
        assert_eq!(admin_options.get_username(), "erpx_test_admin");
        let mut admin = PgConnection::connect_with(&admin_options)
            .await
            .expect("test administrator unavailable");
        let token = format!(
            "{}{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let name = format!("erpx_dev_rust_{token}");
        let lease = format!("erpx-rust-test:{token}");
        sqlx::raw_sql(AssertSqlSafe(format!(
            "CREATE DATABASE \"{name}\" TEMPLATE template0"
        )))
        .execute(&mut admin)
        .await
        .unwrap();
        sqlx::raw_sql(AssertSqlSafe(format!(
            "REVOKE CONNECT ON DATABASE \"{name}\" FROM PUBLIC; COMMENT ON DATABASE \"{name}\" IS '{lease}'"
        )))
        .execute(&mut admin)
        .await
        .unwrap();
        let config = DatabaseConfig::from_options(admin_options.clone().database(&name)).unwrap();
        let pool = connect_pool(
            &config,
            PoolLimits {
                max_connections: 2,
                acquire_timeout: Duration::from_secs(5),
            },
        )
        .await
        .unwrap();
        test(pool.clone()).await;
        pool.close().await;
        let ownership = sqlx::query(
            "SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=$1",
        )
        .bind(&name)
        .fetch_one(&mut admin)
        .await
        .unwrap();
        assert_eq!(ownership.get::<String, _>("owner"), "erpx_test_admin");
        assert_eq!(ownership.get::<String, _>("lease"), lease);
        for attempt in 0..30 {
            match sqlx::raw_sql(AssertSqlSafe(format!("DROP DATABASE \"{name}\"")))
                .execute(&mut admin)
                .await
            {
                Ok(_) => break,
                Err(error)
                    if error.as_database_error().and_then(|e| e.code()).as_deref()
                        == Some("55006")
                        && attempt < 29 =>
                {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(error) => panic!(
                    "owned test database cleanup failed (SQLSTATE {:?})",
                    error.as_database_error().and_then(|e| e.code())
                ),
            }
        }
    });
}

async fn run_create(
    pool: &DatabasePool,
    actor: EcommerceActor,
    command: CreateChannel,
) -> Result<
    erpx_server_ecommerce::public::channel::EcommerceReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgEcommerceCommand::new(connection);
                create_channel(&actor, &command, &mut repository, NOW).await
            })
        })
        .await
}

async fn run_review(
    pool: &DatabasePool,
    actor: EcommerceActor,
    command: ReviewCommand,
) -> Result<
    erpx_server_ecommerce::public::channel::EcommerceReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgEcommerceCommand::new(connection);
                review_ecommerce_record(&actor, &command, &mut repository, NOW).await
            })
        })
        .await
}

async fn read_catalog(
    pool: &DatabasePool,
    actor: EcommerceActor,
) -> Result<
    erpx_server_ecommerce::public::catalog::StorefrontCatalog,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            Box::pin(async move {
                let mut repository = PgEcommerceCatalogRead::new(connection);
                storefront_catalog(&actor, &mut repository, NOW).await
            })
        })
        .await
}

#[test]
fn create_channel_persists_row_receipt_audit_outbox_and_replays() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let receipt = run_create(
                &pool,
                actor("company", None),
                create_command("ecommerce-command-0001"),
            )
            .await
            .unwrap();
            assert_eq!(receipt.id, "ecommerce-channel-ecommerce-command-0001");
            assert_eq!(receipt.status, "draft");
            assert_eq!(receipt.version, 1);

            let row = sqlx::query(
                "SELECT company_id,name,channel_type,configuration_json,status,created_by,\
                 reviewed_by,version,created_at,updated_at FROM ecommerce_channels WHERE id=$1",
            )
            .bind(&receipt.id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("company_id"), "company");
            assert_eq!(row.get::<String, _>("name"), "Storefront");
            assert_eq!(row.get::<String, _>("status"), "draft");
            assert_eq!(row.get::<Option<String>, _>("reviewed_by"), None);
            assert_eq!(row.get::<i64, _>("version"), 1);
            assert_eq!(
                row.get::<String, _>("configuration_json"),
                "{\"storefront\":\"https://shop.example\"}"
            );

            let command = sqlx::query(
                "SELECT operation,actor_id,record_id,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind("ecommerce-command-0001")
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                command.get::<String, _>("operation"),
                "ecommerce.channel.create"
            );
            assert_eq!(command.get::<String, _>("actor_id"), "owner");
            assert_eq!(
                command.get::<String, _>("record_id"),
                "ecommerce-channel-ecommerce-command-0001"
            );
            assert_eq!(
                command.get::<String, _>("result_json"),
                json!({"id":"ecommerce-channel-ecommerce-command-0001","status":"draft","version":1}).to_string()
            );
            assert_eq!(
                count(&pool, "audit_events", "command_id='ecommerce-command-0001'").await,
                1
            );
            assert_eq!(
                count(
                    &pool,
                    "outbox_events",
                    "command_id='ecommerce-command-0001'"
                )
                .await,
                1
            );
            let outbox = sqlx::query(
                "SELECT event_type,aggregate_type,aggregate_version,payload_json FROM outbox_events WHERE command_id=$1",
            )
            .bind("ecommerce-command-0001")
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                outbox.get::<String, _>("event_type"),
                "ecommerce.channel.create"
            );
            assert_eq!(
                outbox.get::<String, _>("aggregate_type"),
                "ecommerce_channel"
            );
            assert_eq!(outbox.get::<i64, _>("aggregate_version"), 1);
            let payload: serde_json::Value =
                serde_json::from_str(&outbox.get::<String, _>("payload_json")).unwrap();
            assert_eq!(payload["state"], "draft");
            assert_eq!(payload["companyId"], "company");

            let replay = run_create(
                &pool,
                actor("company", None),
                create_command("ecommerce-command-0001"),
            )
            .await
            .unwrap();
            assert_eq!(replay, receipt);
            assert_eq!(
                count(
                    &pool,
                    "ecommerce_channels",
                    "id='ecommerce-channel-ecommerce-command-0001'"
                )
                .await,
                1
            );
            assert_eq!(
                count(&pool, "audit_events", "command_id='ecommerce-command-0001'").await,
                1
            );

            let conflict = run_create(
                &pool,
                actor("company", None),
                CreateChannel {
                    name: "Other store".into(),
                    ..create_command("ecommerce-command-0001")
                },
            )
            .await
            .unwrap_err();
            assert_eq!(business(conflict).code, "IDEMPOTENCY_CONFLICT");
        })
    });
}

#[test]
fn create_channel_rolls_back_and_rejects_a_duplicate_owner_identity() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let result: Result<(), PgTransactionError<BusinessError>> =
                PgUnitOfWork::new(pool.clone())
                    .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                        Box::pin(async move {
                            let command = create_command("ecommerce-command-rollback");
                            let mut repository = PgEcommerceCommand::new(connection);
                            create_channel(&actor("company", None), &command, &mut repository, NOW)
                                .await
                                .map_err(|error| match error {
                                    PgTransactionError::Business(error) => error,
                                    other => panic!("unexpected owner failure: {other:?}"),
                                })?;
                            Err(PgTransactionError::Business(BusinessError::new(
                                FailureKind::Unavailable,
                                "TEST_ROLLBACK",
                                "Forced rollback after the owner insert.",
                            )))
                        })
                    })
                    .await;
            assert_eq!(business(result.unwrap_err()).code, "TEST_ROLLBACK");
            assert_eq!(
                count(
                    &pool,
                    "ecommerce_channels",
                    "id='ecommerce-channel-ecommerce-command-rollback'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &pool,
                    "command_receipts",
                    "command_id='ecommerce-command-rollback'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &pool,
                    "outbox_events",
                    "command_id='ecommerce-command-rollback'"
                )
                .await,
                0
            );

            sqlx::raw_sql(
                "INSERT INTO ecommerce_channels\
                 (id,company_id,name,channel_type,configuration_json,status,created_by,\
                 reviewed_by,version,created_at,updated_at) VALUES \
                 ('ecommerce-channel-ecommerce-command-0001','company','Pre-existing','web','{}','draft','creator',NULL,1,1,1);",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let duplicate = run_create(
                &pool,
                actor("company", None),
                create_command("ecommerce-command-0001"),
            )
            .await
            .unwrap_err();
            assert_eq!(business(duplicate).code, "IDEMPOTENCY_CONFLICT");
            assert_eq!(
                count(
                    &pool,
                    "command_receipts",
                    "command_id='ecommerce-command-0001'"
                )
                .await,
                0
            );
            let row = sqlx::query(
                "SELECT name,version FROM ecommerce_channels WHERE id='ecommerce-channel-ecommerce-command-0001'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("name"), "Pre-existing");
        })
    });
}

#[test]
fn review_channel_and_listing_applies_company_cas_independence_and_audit() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let activate = run_review(
                &pool,
                actor("company", None),
                review(
                    ReviewKind::Channel,
                    ReviewAction::Activate,
                    "channel-draft",
                    1,
                ),
            )
            .await
            .unwrap();
            assert_eq!(activate.id, "channel-draft");
            assert_eq!(activate.status, "active");
            assert_eq!(activate.version, 2);
            let row = sqlx::query(
                "SELECT status,reviewed_by,version FROM ecommerce_channels WHERE id='channel-draft'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("status"), "active");
            assert_eq!(row.get::<String, _>("reviewed_by"), "owner");
            assert_eq!(row.get::<i64, _>("version"), 2);

            let publish = run_review(
                &pool,
                actor("company", None),
                review(
                    ReviewKind::Listing,
                    ReviewAction::Publish,
                    "listing-draft",
                    1,
                ),
            )
            .await
            .unwrap();
            assert_eq!(publish.status, "published");
            assert_eq!(publish.version, 2);
            assert_eq!(
                count(
                    &pool,
                    "audit_events",
                    "record_id IN ('channel-draft','listing-draft')"
                )
                .await,
                2
            );

            let stale = run_review(
                &pool,
                actor("company", None),
                review(
                    ReviewKind::Channel,
                    ReviewAction::Retire,
                    "channel-draft",
                    1,
                ),
            )
            .await
            .unwrap_err();
            assert_eq!(business(stale).code, "ECOMMERCE_SOURCE_CHANGED");

            sqlx::raw_sql(
                "INSERT INTO ecommerce_channels\
                 (id,company_id,name,channel_type,configuration_json,status,created_by,\
                 reviewed_by,version,created_at,updated_at) VALUES \
                 ('channel-own','company','Owner draft','web','{}','draft','owner',NULL,1,100,50);",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let own = run_review(
                &pool,
                actor("company", None),
                review(
                    ReviewKind::Channel,
                    ReviewAction::Activate,
                    "channel-own",
                    1,
                ),
            )
            .await
            .unwrap_err();
            assert_eq!(business(own).code, "ECOMMERCE_INDEPENDENT_REVIEW_REQUIRED");

            let retire = run_review(
                &pool,
                actor("company", None),
                review(
                    ReviewKind::Channel,
                    ReviewAction::Retire,
                    "channel-active",
                    2,
                ),
            )
            .await
            .unwrap();
            assert_eq!(retire.status, "retired");

            let foreign = run_review(
                &pool,
                actor("company", None),
                review(
                    ReviewKind::Channel,
                    ReviewAction::Activate,
                    "channel-foreign",
                    1,
                ),
            )
            .await
            .unwrap_err();
            assert_eq!(business(foreign).code, "ECOMMERCE_RECORD_NOT_FOUND");

            let branch = run_review(
                &pool,
                actor("company", Some("branch")),
                review(
                    ReviewKind::Channel,
                    ReviewAction::Retire,
                    "channel-draft",
                    2,
                ),
            )
            .await
            .unwrap_err();
            assert_eq!(business(branch).code, "COMPANY_ECOMMERCE_REQUIRED");

            let independent = run_review(
                &pool,
                EcommerceActor {
                    user_id: UserId::new("creator").unwrap(),
                    ..actor("company", None)
                },
                review(
                    ReviewKind::Channel,
                    ReviewAction::Retire,
                    "channel-draft",
                    2,
                ),
            )
            .await
            .unwrap();
            assert_eq!(independent.status, "retired");
            assert_eq!(independent.version, 3);
        })
    });
}

#[test]
fn catalog_reads_company_isolated_descending_rows_with_transaction_guards() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let catalog = read_catalog(&pool, actor("company", None)).await.unwrap();
            assert_eq!(
                catalog
                    .channels
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                ["channel-draft", "channel-active"]
            );
            assert_eq!(
                catalog
                    .listings
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                ["listing-draft", "listing-active"]
            );
            let foreign = read_catalog(&pool, actor("foreign", None)).await.unwrap();
            assert_eq!(foreign.channels.len(), 1);
            assert_eq!(foreign.channels[0].id, "channel-foreign");
            assert_eq!(foreign.listings.len(), 1);
            assert_eq!(foreign.listings[0].id, "listing-foreign");
            assert!(
                foreign
                    .channels
                    .iter()
                    .all(|row| row.company_id == "foreign")
            );

            let mut connection = pool.raw().acquire().await.unwrap();
            let mut repository = PgEcommerceCatalogRead::new(&mut connection);
            let error = repository
                .channels(&CompanyId::new("company").unwrap())
                .await
                .unwrap_err();
            assert!(matches!(error, PgTransactionError::Persistence(_)));

            sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
                .execute(&mut *connection)
                .await
                .unwrap();
            let mut repository = PgEcommerceCatalogRead::new(&mut connection);
            let error = repository
                .channels(&CompanyId::new("company").unwrap())
                .await
                .unwrap_err();
            assert!(matches!(error, PgTransactionError::Persistence(_)));
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .unwrap();

            let before = count(&pool, "ecommerce_listings", "id='listing-new'").await;
            let result: Result<(), PgTransactionError<BusinessError>> =
                PgUnitOfWork::new(pool.clone())
                    .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                        Box::pin(async move {
                            sqlx::query(
                                "INSERT INTO ecommerce_listings\
                                 (id,channel_id,product_id,title,currency,price_minor,status,\
                                 created_by,reviewed_by,version,created_at,updated_at) VALUES \
                                 ('listing-new','channel-draft','product-2','New online product',\
                                 'THB',100,'draft','owner',NULL,1,500,500)",
                            )
                            .execute(&mut *connection)
                            .await
                            .map_err(PgTransactionError::from)?;
                            let mut catalog = PgEcommerceCatalogRead::new(connection);
                            let _ = catalog
                                .listings(&CompanyId::new("company").unwrap())
                                .await?;
                            Err(PgTransactionError::Business(BusinessError::new(
                                FailureKind::Unavailable,
                                "TEST_ROLLBACK",
                                "Forced rollback after in-transaction readback.",
                            )))
                        })
                    })
                    .await;
            assert_eq!(business(result.unwrap_err()).code, "TEST_ROLLBACK");
            assert_eq!(
                count(&pool, "ecommerce_listings", "id='listing-new'").await,
                before
            );
            let readback = read_catalog(&pool, actor("company", None)).await.unwrap();
            assert!(readback.listings.iter().all(|row| row.id != "listing-new"));
        })
    });
}
