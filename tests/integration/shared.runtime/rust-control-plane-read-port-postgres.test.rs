//! Same-connection live-vs-native parity harness for the runtime control-plane
//! read port (`G4 / migrate.runtime.control-plane-read-port`).
//!
//! For every read family the retained TypeScript statement (copied verbatim
//! from `packages/server-runtime/src/adapters/runtime/{postgres,migrations}.ts`)
//! and the owner's native port run against the same leased PostgreSQL database.
//! SQL text and projected results must agree, and native lock/unlock wrappers
//! must be mutually visible with the retained TypeScript statements. Nothing
//! switches a caller and no TypeScript authority is deleted by this test.

#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::control_plane as port;
use sqlx::AssertSqlSafe;

// Verbatim retained TypeScript statements. `postgres.ts` converts `?` to `$n`;
// these statements already use the executed `$n` form.
const LIVE_RECEIPTS: &str = "SELECT name FROM erpx_migration_receipts() ORDER BY name";
const LIVE_TABLE_PRESENT: &str = "SELECT to_regclass('public.erpx_migrations') IS NOT NULL present";
const LIVE_CHECKSUM: &str = "SELECT checksum FROM erpx_migrations WHERE name=$1";
const LIVE_COVERAGE_PRESENT: &str =
    "SELECT to_regprocedure('ensure_work_cache_coverage()') IS NOT NULL present";
const LIVE_RUNTIME_HEALTH: &str = "SELECT current_setting('lock_timeout') AS value";
const LIVE_MIGRATION_LOCK: &str = "SELECT pg_advisory_lock(714928001)";
const LIVE_MIGRATION_UNLOCK: &str = "SELECT pg_advisory_unlock(714928001)";
const LIVE_OBJECT_LOCK: &str = "SELECT pg_advisory_lock(hashtextextended($1,0))";
const LIVE_OBJECT_UNLOCK: &str = "SELECT pg_advisory_unlock(hashtextextended($1,0)) AS released";
const LIVE_OBJECT_XACT_LOCK: &str = "SELECT pg_advisory_xact_lock(hashtextextended($1,0))";
const LIVE_PAIR_LOCK: &str = "SELECT pg_advisory_lock(hashtext($1),hashtext($2))";
const LIVE_PAIR_UNLOCK: &str = "SELECT pg_advisory_unlock(hashtext($1),hashtext($2)) released";

const MIGRATIONS_TS: &str =
    include_str!("../../../packages/server-runtime/src/adapters/runtime/migrations.ts");
const POSTGRES_TS: &str =
    include_str!("../../../packages/server-runtime/src/adapters/runtime/postgres.ts");

/// The port constants must remain verbatim in the retained TypeScript
/// authority. This check needs no database and catches SQL drift immediately.
#[test]
fn port_sql_is_verbatim_retained_ts_authority() {
    for (port_sql, ts) in [
        (port::sql::MIGRATION_RECEIPTS, MIGRATIONS_TS),
        (port::sql::RECEIPT_TABLE_PRESENT, MIGRATIONS_TS),
        (port::sql::RECEIPT_CHECKSUM, MIGRATIONS_TS),
        (port::sql::COVERAGE_PROCEDURE_PRESENT, MIGRATIONS_TS),
        (port::sql::MIGRATION_LOCK, MIGRATIONS_TS),
        (port::sql::MIGRATION_UNLOCK, MIGRATIONS_TS),
        (port::sql::RUNTIME_HEALTH, POSTGRES_TS),
        (port::sql::OBJECT_LOCK, POSTGRES_TS),
        (port::sql::OBJECT_UNLOCK, POSTGRES_TS),
        (port::sql::OBJECT_XACT_LOCK, POSTGRES_TS),
        (port::sql::PAIR_LOCK, POSTGRES_TS),
        (port::sql::PAIR_UNLOCK, POSTGRES_TS),
    ] {
        assert!(
            ts.contains(port_sql),
            "retained TS no longer contains: {port_sql}"
        );
    }
}

const DDL: &str = r#"
CREATE TABLE erpx_migrations(name TEXT PRIMARY KEY,checksum TEXT NOT NULL,applied_at TIMESTAMPTZ NOT NULL DEFAULT now());
INSERT INTO erpx_migrations(name,checksum) VALUES ('0001_initial.sql','aaa'),('0002_receipts.sql','bbb');
CREATE FUNCTION erpx_migration_receipts() RETURNS TABLE(name TEXT) LANGUAGE sql AS $$ SELECT m.name FROM erpx_migrations m ORDER BY m.name $$;
CREATE FUNCTION ensure_work_cache_coverage() RETURNS void LANGUAGE plpgsql AS $$ BEGIN RETURN; END; $$;
"#;

#[tokio::test]
async fn control_plane_reads_match_retained_ts_authority() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping runtime control-plane read parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    fixture::with_database(2, |pool| async move {
        sqlx::raw_sql(AssertSqlSafe(DDL.to_owned()))
            .execute(pool.raw())
            .await
            .unwrap();
        let mut a = pool.raw().acquire().await.unwrap();
        let mut b = pool.raw().acquire().await.unwrap();

        // Migration receipt/history reads.
        assert_eq!(port::sql::MIGRATION_RECEIPTS, LIVE_RECEIPTS);
        let live: Vec<String> = sqlx::query_scalar(LIVE_RECEIPTS)
            .fetch_all(&mut *a)
            .await
            .unwrap();
        let native = port::migration_receipt_names(&mut *a).await.unwrap();
        assert_eq!(live, native);
        assert_eq!(native, vec!["0001_initial.sql", "0002_receipts.sql"]);

        assert_eq!(port::sql::RECEIPT_TABLE_PRESENT, LIVE_TABLE_PRESENT);
        let live: bool = sqlx::query_scalar(LIVE_TABLE_PRESENT)
            .fetch_one(&mut *a)
            .await
            .unwrap();
        assert_eq!(live, port::receipt_table_present(&mut *a).await.unwrap());

        assert_eq!(port::sql::RECEIPT_CHECKSUM, LIVE_CHECKSUM);
        let live: Option<String> = sqlx::query_scalar(LIVE_CHECKSUM)
            .bind("0001_initial.sql")
            .fetch_optional(&mut *a)
            .await
            .unwrap();
        let native = port::migration_receipt_checksum(&mut *a, "0001_initial.sql")
            .await
            .unwrap();
        assert_eq!(live, native);
        assert_eq!(native.as_deref(), Some("aaa"));
        assert!(
            port::migration_receipt_checksum(&mut *a, "missing.sql")
                .await
                .unwrap()
                .is_none()
        );

        assert_eq!(port::sql::COVERAGE_PROCEDURE_PRESENT, LIVE_COVERAGE_PRESENT);
        let live: bool = sqlx::query_scalar(LIVE_COVERAGE_PRESENT)
            .fetch_one(&mut *a)
            .await
            .unwrap();
        assert_eq!(
            live,
            port::coverage_procedure_present(&mut *a).await.unwrap()
        );

        // Runtime health: the live session setting used by `withObjectLocks`.
        assert_eq!(port::sql::RUNTIME_HEALTH, LIVE_RUNTIME_HEALTH);
        let live: String = sqlx::query_scalar(LIVE_RUNTIME_HEALTH)
            .fetch_one(&mut *a)
            .await
            .unwrap();
        assert_eq!(
            live,
            port::runtime_health(&mut *a).await.unwrap().lock_timeout
        );

        // Migration advisory lock: native lock / retained TS unlock and back.
        assert_eq!(port::sql::MIGRATION_LOCK, LIVE_MIGRATION_LOCK);
        assert_eq!(port::sql::MIGRATION_UNLOCK, LIVE_MIGRATION_UNLOCK);
        port::migration_advisory_lock(&mut *a).await.unwrap();
        let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(714928001)")
            .fetch_one(&mut *b)
            .await
            .unwrap();
        assert!(
            !held,
            "native lock must be visible to the retained TS session"
        );
        let released: bool = sqlx::query_scalar(LIVE_MIGRATION_UNLOCK)
            .fetch_one(&mut *a)
            .await
            .unwrap();
        assert!(released, "retained TS SQL must release the native lock");
        sqlx::query(LIVE_MIGRATION_LOCK)
            .execute(&mut *a)
            .await
            .unwrap();
        assert!(port::migration_advisory_unlock(&mut *a).await.unwrap());
        let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(714928001)")
            .fetch_one(&mut *b)
            .await
            .unwrap();
        assert!(held, "native unlock must release the retained TS lock");
        let _: bool = sqlx::query_scalar(LIVE_MIGRATION_UNLOCK)
            .fetch_one(&mut *b)
            .await
            .unwrap();

        // Object advisory lock (hashtextextended) and transaction-scoped variant.
        assert_eq!(port::sql::OBJECT_LOCK, LIVE_OBJECT_LOCK);
        assert_eq!(port::sql::OBJECT_UNLOCK, LIVE_OBJECT_UNLOCK);
        assert_eq!(port::sql::OBJECT_XACT_LOCK, LIVE_OBJECT_XACT_LOCK);
        let key = "attachment-object:company-1:asset-1";
        port::object_advisory_lock(&mut *a, key).await.unwrap();
        let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1,0))")
            .bind(key)
            .fetch_one(&mut *b)
            .await
            .unwrap();
        assert!(!held);
        let released: bool = sqlx::query_scalar(LIVE_OBJECT_UNLOCK)
            .bind(key)
            .fetch_one(&mut *a)
            .await
            .unwrap();
        assert!(released);
        assert!(!port::object_advisory_unlock(&mut *a, key).await.unwrap());
        sqlx::query(LIVE_OBJECT_LOCK)
            .bind(key)
            .execute(&mut *a)
            .await
            .unwrap();
        assert!(port::object_advisory_unlock(&mut *a, key).await.unwrap());
        port::object_advisory_xact_lock(&mut *a, key).await.unwrap();

        // Two-key transaction session lock (hashtext pair).
        assert_eq!(port::sql::PAIR_LOCK, LIVE_PAIR_LOCK);
        assert_eq!(port::sql::PAIR_UNLOCK, LIVE_PAIR_UNLOCK);
        let (namespace, pair_key) = ("transaction", "aggregate-1");
        port::pair_advisory_lock(&mut *a, namespace, pair_key)
            .await
            .unwrap();
        let held: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtext($1),hashtext($2))")
                .bind(namespace)
                .bind(pair_key)
                .fetch_one(&mut *b)
                .await
                .unwrap();
        assert!(!held);
        let released: bool = sqlx::query_scalar(LIVE_PAIR_UNLOCK)
            .bind(namespace)
            .bind(pair_key)
            .fetch_one(&mut *a)
            .await
            .unwrap();
        assert!(released);
        assert!(
            !port::pair_advisory_unlock(&mut *a, namespace, pair_key)
                .await
                .unwrap()
        );

        // Receipt-reader fallback: a missing reader with a present ledger fails
        // closed; with neither present it reports no receipts, matching TS.
        sqlx::raw_sql(AssertSqlSafe(
            "DROP FUNCTION erpx_migration_receipts()".to_owned(),
        ))
        .execute(&mut *a)
        .await
        .unwrap();
        let unavailable = port::migration_receipt_names(&mut *a).await.unwrap_err();
        assert_eq!(unavailable.code, "MIGRATION_RECEIPTS_UNAVAILABLE");
        sqlx::raw_sql(AssertSqlSafe("DROP TABLE erpx_migrations".to_owned()))
            .execute(&mut *a)
            .await
            .unwrap();
        assert!(
            port::migration_receipt_names(&mut *a)
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await;
}
