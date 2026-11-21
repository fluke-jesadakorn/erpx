//! Support owner record-source reads on migrated PostgreSQL: Company fencing,
//! serializable-unit fencing, bounded payloads and change detection.
//!
//! This test owns its leased database locally because `erpx-server-support`
//! has no Tokio dev-dependency; `sqlx::test_block_on` runs the same fixture
//! lifecycle without adding a locked workspace dependency.
#![allow(dead_code)]

use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PoolLimits, connect_pool,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::record_source_contract::SourceKind;
use erpx_server_shared::public::{BusinessError, CompanyId};
use erpx_server_support::public::record_source::{lock_record_sources, read_record_source};
use sqlx::postgres::PgConnectOptions;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};
use std::future::Future;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn persistence_code(error: PgTransactionError<BusinessError>) -> String {
    match error {
        PgTransactionError::Persistence(error) => error.code.into(),
        other => panic!("expected a persistence failure, got {other:?}"),
    }
}

fn business_code(error: PgTransactionError<BusinessError>) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

/// Owns only newly-created, leased `erpx_dev_rust_*` databases. The test body
/// runs on its own thread so cleanup still happens when an assertion panics.
fn with_database<F, Fut>(max_connections: u32, test: F)
where
    F: FnOnce(DatabasePool) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    sqlx::test_block_on(async move {
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
        let token = format!(
            "{:x}{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let name = format!("erpx_dev_rust_{token}");
        let lease = format!("erpx-rust-test:{token}");
        // Both interpolations consist exclusively of a fixed prefix and generated lowercase hex.
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
        // The pool and its owner reads share one runtime; creating it in the
        // parent runtime and using it from a nested runtime times out instead.
        let outcome = std::thread::spawn(move || {
            sqlx::test_block_on(async move {
                let pool = connect_pool(
                    &config,
                    PoolLimits {
                        max_connections,
                        acquire_timeout: Duration::from_secs(5),
                    },
                )
                .await
                .unwrap();
                let test_pool = pool.clone();
                test(test_pool).await;
                pool.close().await;
            })
        })
        .join();
        let ownership = sqlx::query(
            "SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease \
             FROM pg_database WHERE datname=$1",
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
        // A short-lived autovacuum backend may still be finishing in our database.
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
        if let Err(payload) = outcome {
            std::panic::resume_unwind(payload);
        }
    });
}

async fn serializable<'c>(pool: &'c DatabasePool) -> sqlx::Transaction<'c, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO support_tickets\
         (id,company_id,branch_id,department_id,title,issue,owner_id,due_date,status,version,\
         created_by,created_at,updated_at,source_kind) VALUES \
         ('ticket-1','company','branch-1','department-1','Printer offline',\
          'The front desk printer is offline.','fixture-a','2026-11-10','open',2,'fixture-a',100,300,'general'),\
         ('ticket-2','company',NULL,NULL,'General request',\
          'This report has no branch or department.','fixture-a','2026-11-11','open',1,'fixture-a',110,310,'general'),\
         ('ticket-foreign','foreign','foreign-branch',NULL,'Foreign ticket',\
          'This row must never cross the Company.','fixture-f','2026-11-12','open',1,'fixture-f',120,320,'general');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[test]
fn ticket_facts_are_company_fenced_bounded_and_change_detected() {
    with_database(3, |pool| async move {
        setup(&pool).await;
        let company = CompanyId::new("company").unwrap();
        let mut tx = serializable(&pool).await;
        lock_record_sources(&mut tx).await.unwrap();

        let source = read_record_source(&mut tx, &company, "ticket-1")
            .await
            .unwrap()
            .expect("owner ticket");
        let data = source.data();
        assert_eq!(data.kind, SourceKind::SupportTicket);
        assert_eq!(data.id, "ticket-1");
        assert_eq!(data.company_id, "company");
        assert_eq!(data.branch_id.as_deref(), Some("branch-1"));
        assert_eq!(data.department_id.as_deref(), Some("department-1"));
        assert_eq!(data.version, 2);
        assert_eq!(data.title, "Printer offline");
        assert_eq!(data.status, "open");
        source.require_current(&mut tx).await.unwrap();

        let general = read_record_source(&mut tx, &company, "ticket-2")
            .await
            .unwrap()
            .expect("owner general ticket");
        assert_eq!(general.data().kind, SourceKind::SupportTicket);
        assert_eq!(general.data().branch_id, None);
        assert_eq!(general.data().department_id, None);
        general.require_current(&mut tx).await.unwrap();

        assert!(
            read_record_source(&mut tx, &company, "missing")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            read_record_source(&mut tx, &company, "ticket-foreign")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            read_record_source(
                &mut tx,
                &CompanyId::new("foreign").unwrap(),
                "ticket-1"
            )
            .await
            .unwrap()
            .is_none()
        );

        sqlx::query("UPDATE support_tickets SET version=version+1,status='in_progress' WHERE id='ticket-1'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            business_code(source.require_current(&mut tx).await.unwrap_err()),
            "SOURCE_RECORD_CHANGED"
        );
        let updated = read_record_source(&mut tx, &company, "ticket-1")
            .await
            .unwrap()
            .expect("updated ticket");
        assert_eq!(updated.data().version, 3);
        assert_eq!(updated.data().status, "in_progress");
        updated.require_current(&mut tx).await.unwrap();

        sqlx::query("UPDATE support_tickets SET issue=repeat('x',2097153) WHERE id='ticket-1'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            business_code(
                read_record_source(&mut tx, &company, "ticket-1")
                    .await
                    .err()
                    .expect("expected a source read failure")
            ),
            "RECORD_SOURCE_LIMIT"
        );
        tx.rollback().await.unwrap();
    });
}

#[test]
fn ticket_source_reads_require_a_serializable_unit() {
    with_database(2, |pool| async move {
        setup(&pool).await;
        let company = CompanyId::new("company").unwrap();

        let mut connection = pool.raw().acquire().await.unwrap();
        assert_eq!(
            persistence_code(
                read_record_source(&mut *connection, &company, "ticket-1")
                    .await
                    .err()
                    .expect("expected a source read failure")
            ),
            "RECORD_SOURCE_SERIALIZABLE_REQUIRED"
        );
        assert_eq!(
            persistence_code(lock_record_sources(&mut *connection).await.unwrap_err()),
            "RECORD_SOURCE_TRANSACTION_REQUIRED"
        );
        drop(connection);

        let mut plain = pool.raw().begin().await.unwrap();
        assert_eq!(
            persistence_code(
                read_record_source(&mut plain, &company, "ticket-1")
                    .await
                    .err()
                    .expect("expected a source read failure")
            ),
            "RECORD_SOURCE_SERIALIZABLE_REQUIRED"
        );
        plain.rollback().await.unwrap();
    });
}
