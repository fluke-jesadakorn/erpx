//! Native Commerce owner-side source for Ecommerce listing creation on migrated
//! PostgreSQL. These tests exercise the real producer SQL that composition will
//! mount behind `PgEcommerceListingCommerceSource`: active-product witness and
//! the Company functional currency joined to the Finance chart base. They prove
//! Company isolation, fail-closed reads and exact oracle parity, not a Next or
//! browser journey.
use erpx_server_commerce::public::ecommerce_listing_source::{
    EcommerceListingCurrency, active_product, functional_currency,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PoolLimits, connect_pool, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::CompanyId;
use sqlx::{AssertSqlSafe, Connection, PgConnection, postgres::PgConnectOptions};
use std::{str::FromStr, sync::atomic::AtomicU64, time::Duration};

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

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
        "INSERT INTO finance_charts (id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES \
         ('chart-company','company','Company chart','THB','system',100,100),\
         ('chart-foreign','foreign','Foreign chart','USD','system',100,100);\
         INSERT INTO finance_accounts (id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES \
         ('acct-0','chart-company','company','0','Cash','asset',100,100),\
         ('acct-1','chart-company','company','1','Receivable','asset',100,100),\
         ('acct-2','chart-company','company','2','Payable','liability',100,100),\
         ('acct-3','chart-company','company','3','Revenue','revenue',100,100),\
         ('acct-4','chart-company','company','4','Inventory','asset',100,100),\
         ('acct-5','chart-company','company','5','Clearing','asset',100,100),\
         ('acct-6','chart-company','company','6','Cogs','expense',100,100),\
         ('acct-7','chart-company','company','7','Input tax','asset',100,100),\
         ('acct-8','chart-company','company','8','Output tax','liability',100,100),\
         ('acct-9','chart-company','company','9','Bank','asset',100,100);\
         INSERT INTO commerce_settings \
         (company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,\
          inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES \
         ('company','chart-company','THB','acct-1','acct-2','acct-3','acct-4','acct-5','acct-6','acct-7','acct-8','acct-9'),\
         ('mismatch','chart-foreign','USD','acct-1','acct-2','acct-3','acct-4','acct-5','acct-6','acct-7','acct-8','acct-9');\
         INSERT INTO commerce_products (id,company_id,sku,name,price_minor,status) VALUES \
         ('product-1','company','SKU-1','Active product',1250,'active'),\
         ('product-off','company','SKU-OFF','Archived product',900,'archived'),\
         ('product-f','foreign','SKU-F','Foreign product',9900,'active');",
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
                Err(error) => panic!("owned test database cleanup failed: {error}"),
            }
        }
    });
}

#[test]
fn active_product_is_company_scoped_and_status_guarded() {
    with_database(|pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        let company = CompanyId::new("company").unwrap();
        assert!(active_product(&mut tx, &company, "product-1")
            .await
            .unwrap());
        assert!(!active_product(&mut tx, &company, "product-off")
            .await
            .unwrap());
        assert!(!active_product(&mut tx, &company, "product-f")
            .await
            .unwrap());
        assert!(!active_product(&mut tx, &company, "missing")
            .await
            .unwrap());
        let foreign = CompanyId::new("foreign").unwrap();
        assert!(active_product(&mut tx, &foreign, "product-f")
            .await
            .unwrap());
        tx.rollback().await.unwrap();
    });
}

#[test]
fn functional_currency_uses_company_settings_and_chart_base() {
    with_database(|pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        let company = CompanyId::new("company").unwrap();
        assert_eq!(
            functional_currency(&mut tx, &company).await.unwrap(),
            Some(EcommerceListingCurrency {
                currency: "THB".into(),
                base_currency: "THB".into(),
            })
        );
        for absent in ["none", "foreign", "mismatch"] {
            let company = CompanyId::new(absent).unwrap();
            assert_eq!(
                functional_currency(&mut tx, &company).await.unwrap(),
                None,
                "no configured currency for {absent}"
            );
        }
        tx.rollback().await.unwrap();
    });
}

#[test]
fn source_reads_fail_closed_without_a_transaction() {
    with_database(|pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let company = CompanyId::new("company").unwrap();
        for error in [
            active_product(&mut connection, &company, "product-1")
                .await
                .unwrap_err(),
            functional_currency(&mut connection, &company)
                .await
                .unwrap_err(),
        ] {
            assert_eq!(error.code, "ECOMMERCE_LISTING_SOURCE_UNAVAILABLE");
        }
    });
}
