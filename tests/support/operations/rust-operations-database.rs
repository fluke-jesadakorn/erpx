//! Operations-only disposable PostgreSQL lease. No uuid dependency is added;
//! names use the process id and a monotonic nanosecond/counter token.
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, PoolLimits, connect_pool,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{
    future::Future,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static TOKEN: AtomicU64 = AtomicU64::new(0);

pub async fn with_database<F, Fut>(max_connections: u32, test: F)
where
    F: FnOnce(DatabasePool) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let admin_url = std::env::var("ERPX_TEST_ADMIN_URL")
        .expect("ERPX_TEST_ADMIN_URL required for isolated native Postgres tests");
    let admin_options = PgConnectOptions::from_str(&admin_url).expect("invalid test admin URL");
    assert!(
        admin_options.get_host() == "127.0.0.1",
        "test admin must be local"
    );
    assert_eq!(
        admin_options.get_port(),
        5432,
        "only existing shared PostgreSQL port is authorized"
    );
    assert_eq!(admin_options.get_database(), Some("postgres"));
    assert_eq!(admin_options.get_username(), "erpx_test_admin");
    let mut admin = PgConnection::connect_with(&admin_options)
        .await
        .expect("test administrator unavailable");
    let role = sqlx::query(
        "SELECT current_user::text AS role,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut admin)
    .await
    .unwrap();
    assert_eq!(role.get::<String, _>("role"), "erpx_test_admin");
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(role.get::<bool, _>("rolcreatedb"));
    assert!(!role.get::<bool, _>("rolcreaterole"));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let counter = TOKEN.fetch_add(1, Ordering::SeqCst);
    let token = format!("{nanos:x}{:x}{counter:x}", std::process::id());
    let name = format!("erpx_dev_rust_{token}");
    let lease = format!("erpx-rust-test:{token}");
    sqlx::raw_sql(AssertSqlSafe(format!(
        "CREATE DATABASE \"{name}\" TEMPLATE template0"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    let setup = async {
        sqlx::raw_sql(AssertSqlSafe(format!(
            "REVOKE CONNECT ON DATABASE \"{name}\" FROM PUBLIC; COMMENT ON DATABASE \"{name}\" IS '{lease}'"
        )))
        .execute(&mut admin)
        .await
        .unwrap();
        let config = DatabaseConfig::from_options(admin_options.clone().database(&name)).unwrap();
        connect_pool(
            &config,
            PoolLimits {
                max_connections,
                acquire_timeout: Duration::from_secs(5),
            },
        )
        .await
        .unwrap()
    }
    .await;
    let test_pool = setup.clone();
    let result = tokio::spawn(async move { test(test_pool).await }).await;
    setup.close().await;
    let ownership = sqlx::query(
        "SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=$1",
    )
    .bind(&name)
    .fetch_one(&mut admin)
    .await
    .unwrap();
    assert!(name.starts_with("erpx_dev_rust_"));
    assert_eq!(ownership.get::<String, _>("owner"), "erpx_test_admin");
    assert_eq!(
        ownership.get::<String, _>("lease"),
        lease,
        "refusing to drop a database without our exact lease"
    );
    for attempt in 0..30 {
        match sqlx::raw_sql(AssertSqlSafe(format!("DROP DATABASE \"{name}\"")))
            .execute(&mut admin)
            .await
        {
            Ok(_) => break,
            Err(error)
                if error.as_database_error().and_then(|e| e.code()).as_deref() == Some("55006")
                    && attempt < 29 =>
            {
                tokio::time::sleep(Duration::from_millis(100)).await
            }
            Err(error) => panic!(
                "owned test database cleanup failed (SQLSTATE {:?})",
                error.as_database_error().and_then(|e| e.code())
            ),
        }
    }
    if let Err(error) = result {
        if error.is_panic() {
            std::panic::resume_unwind(error.into_panic());
        }
        panic!("native database test was cancelled");
    }
}
