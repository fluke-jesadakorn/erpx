//! Native Ecommerce listing draft/create/change on migrated PostgreSQL. These
//! tests prove owner SQL, Company isolation, source guards, exact author-only
//! CAS, Finance Money evidence, command idempotency, audit/outbox persistence,
//! rollback and committed readback. The Commerce product/functional-currency
//! source and the Finance Money capture are implemented here as the producer
//! ports composition will supply; their owner SQL is not exercised. These tests
//! do not prove a Next/browser journey or composition mounting.
use erpx_server_ecommerce::public::listing::{
    ChangeListing, CreateListing, EcommerceActor, EcommerceFunctionalCurrency,
    EcommerceListingMoneyRequest, EcommerceListingMoneySnapshot, PgEcommerceListing,
    PgEcommerceListingCommerceSource, PgEcommerceListingMoneyCapture, change_listing,
    create_listing,
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
const KEY: &str = "ecommerce-listing-command-0001";
/// Parallel tests in one process may observe the same clock tick, so the owned
/// database name also carries a monotonic counter.
static NEXT_DATABASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(company: &str) -> EcommerceActor {
    EcommerceActor {
        user_id: UserId::new("owner").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: None,
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

fn create(key: &str, channel: &str, product: &str) -> CreateListing {
    CreateListing {
        key: CommandKey::new(key).unwrap(),
        channel_id: RecordId::new(channel).unwrap(),
        product_id: RecordId::new(product).unwrap(),
        title: "  Online product  ".into(),
        currency: "thb".into(),
        price: "12.5".into(),
    }
}

fn change(key: &str, id: &str, version: i64, title: &str) -> ChangeListing {
    ChangeListing {
        key: CommandKey::new(key).unwrap(),
        id: RecordId::new(id).unwrap(),
        expected_version: version,
        title: title.into(),
    }
}

/// The Commerce-owned source witness composition will supply. The SQL here is
/// the producer's, not Ecommerce owner SQL.
struct CommerceSource;

impl PgEcommerceListingCommerceSource for CommerceSource {
    fn active_product<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company: &'a CompanyId,
        product_id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<bool, BusinessError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let active = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM commerce_products \
                 WHERE id=$1 AND company_id=$2 AND status='active')",
            )
            .bind(product_id)
            .bind(company.as_str())
            .fetch_one(connection)
            .await
            .map_err(|_| source_unavailable())?;
            Ok(active)
        })
    }

    fn functional_currency<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a CompanyId,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Option<EcommerceFunctionalCurrency>, BusinessError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            Ok(Some(EcommerceFunctionalCurrency {
                currency: "THB".into(),
                base_currency: "THB".into(),
            }))
        })
    }
}

fn source_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_LISTING_SOURCE_UNAVAILABLE",
        "The Commerce listing source could not be read.",
    )
}

/// A USD chart used to prove the functional-currency rule.
struct UsdCommerceSource;

impl PgEcommerceListingCommerceSource for UsdCommerceSource {
    fn active_product<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company: &'a CompanyId,
        product_id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<bool, BusinessError>> + Send + 'a>,
    > {
        CommerceSource.active_product(connection, company, product_id)
    }

    fn functional_currency<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a CompanyId,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Option<EcommerceFunctionalCurrency>, BusinessError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            Ok(Some(EcommerceFunctionalCurrency {
                currency: "USD".into(),
                base_currency: "USD".into(),
            }))
        })
    }
}

/// The Finance-owned `snapshotMoney` producer composition will supply. It
/// retains the listing `unit_price` row in the caller's transaction.
struct MoneyCapture;

impl PgEcommerceListingMoneyCapture for MoneyCapture {
    fn capture<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        _: &'a EcommerceActor,
        request: &'a EcommerceListingMoneyRequest,
        now: TimestampMillis,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<EcommerceListingMoneySnapshot, BusinessError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO money_transaction_snapshots\
                 (company_id,source_type,source_id,amount_kind,transaction_date,currency,\
                 currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) \
                 VALUES($1,'ecommerce_listing',$2,'unit_price',CURRENT_DATE,$3,2,$4::numeric,\
                 $4::numeric,NULL,false,$5)",
            )
            .bind(&request.company_id)
            .bind(&request.listing_id)
            .bind(&request.currency)
            .bind(request.amount_minor.to_string())
            .bind(now.get())
            .execute(&mut *connection)
            .await
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "ECOMMERCE_MONEY_EVIDENCE_INVALID",
                    "The Finance unit-price snapshot could not be retained.",
                )
            })?;
            Ok(EcommerceListingMoneySnapshot {
                currency_scale: 2,
                amount_units: request.amount_minor,
                usd_units: request.amount_minor,
                rate_id: None,
                used_latest_fallback: false,
                captured_at: now.get(),
            })
        })
    }
}

static COMMERCE: CommerceSource = CommerceSource;
static USD_COMMERCE: UsdCommerceSource = UsdCommerceSource;
static MONEY: MoneyCapture = MoneyCapture;

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
         ('product-1','company','SKU-1','Online product','each',1250,0,'active',1),\
         ('product-2','company','SKU-2','Second online product','each',1000,0,'active',1),\
         ('product-3','company','SKU-3','Third online product','each',990,0,'active',1),\
         ('product-off','company','SKU-OFF','Retired online product','each',900,0,'archived',1),\
         ('product-f','foreign','SKU-F','Foreign product','each',9900,0,'active',1);\
         INSERT INTO ecommerce_channels\
         (id,company_id,name,channel_type,configuration_json,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('channel-active','company','Active store','web','{}','active','creator','reviewer',2,100,200),\
         ('channel-draft','company','Draft store','web','{}','draft','creator',NULL,1,100,300),\
         ('channel-foreign','foreign','Foreign store','web','{}','active','foreign','reviewer',2,100,400);\
         INSERT INTO ecommerce_listings\
         (id,channel_id,product_id,title,currency,price_minor,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('listing-published','channel-active','product-1','Published product','THB',1250,'published','creator','reviewer',2,100,200),\
         ('listing-own','channel-active','product-2','Own draft product','THB',1000,'draft','owner',NULL,1,100,100),\
         ('listing-other','channel-draft','product-3','Other draft product','THB',990,'draft','creator',NULL,1,100,300),\
         ('listing-foreign','channel-foreign','product-f','Foreign product','USD',9900,'published','foreign','reviewer',2,100,400);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
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
            "{}{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_DATABASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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

async fn count(pool: &DatabasePool, table: &str, filter: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table} WHERE {filter}");
    sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

async fn run_create(
    pool: &DatabasePool,
    actor: EcommerceActor,
    command: CreateListing,
    commerce: &'static dyn PgEcommerceListingCommerceSource,
) -> Result<
    erpx_server_ecommerce::public::listing::EcommerceReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository =
                    PgEcommerceListing::with_sources(connection, commerce, &MONEY);
                create_listing(&actor, &command, &mut repository, NOW).await
            })
        })
        .await
}

async fn run_create_without_sources(
    pool: &DatabasePool,
    actor: EcommerceActor,
    command: CreateListing,
) -> Result<
    erpx_server_ecommerce::public::listing::EcommerceReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgEcommerceListing::new(connection);
                create_listing(&actor, &command, &mut repository, NOW).await
            })
        })
        .await
}

async fn run_change(
    pool: &DatabasePool,
    actor: EcommerceActor,
    command: ChangeListing,
) -> Result<
    erpx_server_ecommerce::public::listing::EcommerceReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgEcommerceListing::new(connection);
                change_listing(&actor, &command, &mut repository, NOW).await
            })
        })
        .await
}

#[test]
fn create_listing_persists_draft_receipt_audit_outbox_and_money() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let receipt = run_create(
                &pool,
                actor("company"),
                create(KEY, "channel-active", "product-3"),
                &COMMERCE,
            )
            .await
            .unwrap();
            let id = format!("ecommerce-listing-{KEY}");
            assert_eq!(receipt.id, id);
            assert_eq!(receipt.status, "draft");
            assert_eq!(receipt.version, 1);

            let row = sqlx::query(
                "SELECT l.channel_id,l.product_id,l.title,l.currency,l.price_minor,l.status,\
                 l.created_by,l.reviewed_by,l.version,l.created_at,l.updated_at,c.company_id \
                 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id \
                 WHERE l.id=$1",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("company_id"), "company");
            assert_eq!(row.get::<String, _>("channel_id"), "channel-active");
            assert_eq!(row.get::<String, _>("product_id"), "product-3");
            assert_eq!(row.get::<String, _>("title"), "Online product");
            assert_eq!(row.get::<String, _>("currency"), "THB");
            assert_eq!(row.get::<i64, _>("price_minor"), 1_250);
            assert_eq!(row.get::<String, _>("created_by"), "owner");
            assert_eq!(row.get::<Option<String>, _>("reviewed_by"), None);
            assert_eq!(row.get::<i64, _>("version"), 1);

            let command = sqlx::query(
                "SELECT operation,actor_id,record_id,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(KEY)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                command.get::<String, _>("operation"),
                "ecommerce.listing.create"
            );
            assert_eq!(command.get::<String, _>("actor_id"), "owner");
            assert_eq!(command.get::<String, _>("record_id"), id);
            assert_eq!(
                command.get::<String, _>("result_json"),
                json!({"id":id,"status":"draft","version":1}).to_string()
            );
            assert_eq!(
                count(&pool, "audit_events", &format!("command_id='{KEY}'")).await,
                1
            );
            assert_eq!(
                count(&pool, "outbox_events", &format!("command_id='{KEY}'")).await,
                1
            );
            let outbox = sqlx::query(
                "SELECT event_type,aggregate_type,aggregate_version,payload_json \
                 FROM outbox_events WHERE command_id=$1",
            )
            .bind(KEY)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                outbox.get::<String, _>("event_type"),
                "ecommerce.listing.create"
            );
            assert_eq!(
                outbox.get::<String, _>("aggregate_type"),
                "ecommerce_listing"
            );
            assert_eq!(outbox.get::<i64, _>("aggregate_version"), 1);
            let payload: serde_json::Value =
                serde_json::from_str(&outbox.get::<String, _>("payload_json")).unwrap();
            assert_eq!(payload["state"], "draft");
            assert_eq!(payload["companyId"], "company");

            let money = sqlx::query(
                "SELECT currency,currency_scale,amount_units::text,usd_units::text,captured_at \
                 FROM money_transaction_snapshots \
                 WHERE company_id='company' AND source_type='ecommerce_listing' \
                 AND source_id=$1 AND amount_kind='unit_price'",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(money.get::<String, _>("currency"), "THB");
            assert_eq!(money.get::<i16, _>("currency_scale"), 2);
            assert_eq!(money.get::<String, _>("amount_units"), "1250");
            assert_eq!(money.get::<i64, _>("captured_at"), NOW.get());

            let replay = run_create(
                &pool,
                actor("company"),
                create(KEY, "channel-active", "product-3"),
                &COMMERCE,
            )
            .await
            .unwrap();
            assert_eq!(replay, receipt);
            assert_eq!(
                count(&pool, "ecommerce_listings", &format!("id='{id}'")).await,
                1
            );
            assert_eq!(
                count(&pool, "money_transaction_snapshots", &format!("source_id='{id}'")).await,
                1
            );
            assert_eq!(
                count(&pool, "audit_events", &format!("command_id='{KEY}'")).await,
                1
            );

            let conflict = run_create(
                &pool,
                actor("company"),
                CreateListing {
                    title: "Another product".into(),
                    ..create(KEY, "channel-active", "product-3")
                },
                &COMMERCE,
            )
            .await
            .unwrap_err();
            assert_eq!(business(conflict).code, "IDEMPOTENCY_CONFLICT");
        })
    });
}

#[test]
fn create_listing_keeps_company_scope_and_source_guards() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            for (command, commerce, code) in [
                (
                    create("ecommerce-listing-command-0002", "channel-draft", "product-3"),
                    &COMMERCE as &'static dyn PgEcommerceListingCommerceSource,
                    "ECOMMERCE_CHANNEL_NOT_ACTIVE",
                ),
                (
                    create("ecommerce-listing-command-0003", "channel-foreign", "product-f"),
                    &COMMERCE,
                    "ECOMMERCE_CHANNEL_NOT_ACTIVE",
                ),
                (
                    create("ecommerce-listing-command-0004", "channel-active", "product-off"),
                    &COMMERCE,
                    "ECOMMERCE_PRODUCT_NOT_ACTIVE",
                ),
                (
                    create("ecommerce-listing-command-0005", "channel-active", "product-f"),
                    &COMMERCE,
                    "ECOMMERCE_PRODUCT_NOT_ACTIVE",
                ),
                (
                    create("ecommerce-listing-command-0006", "channel-active", "product-3"),
                    &USD_COMMERCE,
                    "ECOMMERCE_CURRENCY_MAPPING_REQUIRED",
                ),
            ] {
                assert_eq!(
                    business(
                        run_create(&pool, actor("company"), command, commerce)
                            .await
                            .unwrap_err()
                    )
                    .code,
                    code
                );
            }
            assert_eq!(
                count(&pool, "ecommerce_listings", "created_by='owner'").await,
                1
            );

            let closed = run_create_without_sources(
                &pool,
                actor("company"),
                create("ecommerce-listing-command-0007", "channel-active", "product-3"),
            )
            .await
            .unwrap_err();
            assert_eq!(
                business(closed).code,
                "ECOMMERCE_LISTING_SOURCE_UNAVAILABLE"
            );

            let duplicate = run_create(
                &pool,
                actor("company"),
                create("ecommerce-listing-command-0008", "channel-active", "product-1"),
                &COMMERCE,
            )
            .await
            .unwrap_err();
            assert_eq!(business(duplicate).code, "ECOMMERCE_LISTING_EXISTS");
            assert_eq!(
                count(
                    &pool,
                    "money_transaction_snapshots",
                    "source_id='ecommerce-listing-ecommerce-listing-command-0008'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &pool,
                    "command_receipts",
                    "command_id='ecommerce-listing-command-0008'"
                )
                .await,
                0
            );
        })
    });
}

#[test]
fn change_listing_cas_author_and_immutable_terms() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let receipt = run_change(
                &pool,
                actor("company"),
                change(
                    "ecommerce-listing-change-0001",
                    "listing-own",
                    1,
                    "  Revised own product  ",
                ),
            )
            .await
            .unwrap();
            assert_eq!(receipt.id, "listing-own");
            assert_eq!(receipt.status, "draft");
            assert_eq!(receipt.version, 2);
            let row = sqlx::query(
                "SELECT title,channel_id,product_id,currency,price_minor,status,\
                 created_by,reviewed_by,version,created_at,updated_at \
                 FROM ecommerce_listings WHERE id='listing-own'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("title"), "Revised own product");
            assert_eq!(row.get::<String, _>("channel_id"), "channel-active");
            assert_eq!(row.get::<String, _>("product_id"), "product-2");
            assert_eq!(row.get::<String, _>("currency"), "THB");
            assert_eq!(row.get::<i64, _>("price_minor"), 1_000);
            assert_eq!(row.get::<String, _>("created_by"), "owner");
            assert_eq!(row.get::<Option<String>, _>("reviewed_by"), None);
            assert_eq!(row.get::<i64, _>("version"), 2);
            assert_eq!(row.get::<i64, _>("created_at"), 100);
            assert_eq!(row.get::<i64, _>("updated_at"), NOW.get());
            let replay = run_change(
                &pool,
                actor("company"),
                change(
                    "ecommerce-listing-change-0001",
                    "listing-own",
                    1,
                    "  Revised own product  ",
                ),
            )
            .await
            .unwrap();
            assert_eq!(replay, receipt);
            assert_eq!(
                count(
                    &pool,
                    "command_receipts",
                    "command_id='ecommerce-listing-change-0001'"
                )
                .await,
                1
            );

            for (command, user, code) in [
                (
                    change("ecommerce-listing-change-0002", "listing-own", 1, "Stale"),
                    "owner",
                    "ECOMMERCE_SOURCE_CHANGED",
                ),
                (
                    change("ecommerce-listing-change-0003", "listing-own", 2, "Other author"),
                    "someone",
                    "ECOMMERCE_LISTING_AUTHOR_REQUIRED",
                ),
                (
                    change(
                        "ecommerce-listing-change-0004",
                        "listing-published",
                        2,
                        "Published retitle",
                    ),
                    "owner",
                    "ECOMMERCE_SOURCE_CHANGED",
                ),
                (
                    change("ecommerce-listing-change-0005", "listing-foreign", 2, "Foreign"),
                    "owner",
                    "ECOMMERCE_RECORD_NOT_FOUND",
                ),
            ] {
                let actor = EcommerceActor {
                    user_id: UserId::new(user).unwrap(),
                    ..actor("company")
                };
                assert_eq!(
                    business(run_change(&pool, actor, command).await.unwrap_err()).code,
                    code
                );
            }

            let guard = sqlx::raw_sql(
                "UPDATE ecommerce_listings SET price_minor=1 WHERE id='listing-own'",
            )
            .execute(pool.raw())
            .await;
            assert!(guard.is_err());
            let row =
                sqlx::query("SELECT price_minor,version FROM ecommerce_listings WHERE id='listing-own'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(row.get::<i64, _>("price_minor"), 1_000);
            assert_eq!(row.get::<i64, _>("version"), 2);
        })
    });
}

#[test]
fn listing_commands_require_a_serializable_transaction() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let mut connection = pool.raw().acquire().await.unwrap();
            let mut repository =
                PgEcommerceListing::with_sources(&mut connection, &COMMERCE, &MONEY);
            let error = create_listing(
                &actor("company"),
                &create("ecommerce-listing-command-0009", "channel-active", "product-3"),
                &mut repository,
                NOW,
            )
            .await
            .unwrap_err();
            assert!(matches!(error, PgTransactionError::Persistence(_)));

            sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
                .execute(&mut *connection)
                .await
                .unwrap();
            let mut repository = PgEcommerceListing::new(&mut connection);
            let error = change_listing(
                &actor("company"),
                &change("ecommerce-listing-change-0006", "listing-own", 1, "Committed"),
                &mut repository,
                NOW,
            )
            .await
            .unwrap_err();
            assert!(matches!(error, PgTransactionError::Persistence(_)));
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .unwrap();
            assert_eq!(
                count(
                    &pool,
                    "command_receipts",
                    "command_id='ecommerce-listing-change-0006'"
                )
                .await,
                0
            );
        })
    });
}

#[test]
fn failed_listing_commands_leave_no_owner_or_ledger_rows() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let result: Result<(), PgTransactionError<BusinessError>> =
                PgUnitOfWork::new(pool.clone())
                    .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                        Box::pin(async move {
                            let command = create(
                                "ecommerce-listing-command-rollback",
                                "channel-active",
                                "product-3",
                            );
                            let mut repository =
                                PgEcommerceListing::with_sources(connection, &COMMERCE, &MONEY);
                            create_listing(&actor("company"), &command, &mut repository, NOW)
                                .await
                                .map_err(|error| match error {
                                    PgTransactionError::Business(error) => error,
                                    other => panic!("unexpected owner failure: {other:?}"),
                                })?;
                            Err(PgTransactionError::Business(BusinessError::new(
                                FailureKind::Unavailable,
                                "TEST_ROLLBACK",
                                "Forced rollback after the listing write.",
                            )))
                        })
                    })
                    .await;
            assert_eq!(business(result.unwrap_err()).code, "TEST_ROLLBACK");
            assert_eq!(
                count(
                    &pool,
                    "ecommerce_listings",
                    "id='ecommerce-listing-ecommerce-listing-command-rollback'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &pool,
                    "money_transaction_snapshots",
                    "source_id='ecommerce-listing-ecommerce-listing-command-rollback'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &pool,
                    "command_receipts",
                    "command_id='ecommerce-listing-command-rollback'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &pool,
                    "outbox_events",
                    "command_id='ecommerce-listing-command-rollback'"
                )
                .await,
                0
            );
        })
    });
}
