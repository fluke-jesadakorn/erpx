//! Native POS core workspace read ports (shifts family) on isolated PostgreSQL.
//!
//! The retained TypeScript `pos.ts` held three `SELECT` literals backing the
//! shifts/tables/cash-movement workspace tabs. Their SQL now lives in the
//! native POS owner registry (`erpx-server-pos::adapters::core_reads`); the TS
//! bridge sends a typed kind and positional values only. This oracle
//! reconstructs the retired literals verbatim, asserts byte equality with the
//! mounted owner statements, then executes both on a seeded database so an
//! invalid SQL shape or a caller-scope leak cannot pass as "ported".
use erpx_server_pos::public::core_reads as mounted;
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::str::FromStr;

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. None of these closed statements contains a literal `?` in a
/// Postgres operator position.
fn pg(sql: &str) -> String {
    let mut out = String::new();
    let mut chunks = sql.split('?');
    if let Some(first) = chunks.next() {
        out.push_str(first);
        for (index, chunk) in chunks.enumerate() {
            out.push_str(&format!("${}", index + 1));
            out.push_str(chunk);
        }
    }
    out
}

/// Closed owner kinds the TypeScript shifts bridge requests (literal, not
/// imported from the registry so the parity check can detect drift).
const CORE_READ_KINDS: &[&str] = &[
    "pos_core_shifts",
    "pos_core_tables",
    "pos_core_cash_movements",
    "pos_core_register_by_name",
    "pos_core_open_shift_by_register",
    "pos_core_table_by_name",
    "pos_core_registers_by_kind",
    "pos_core_restaurant_register_property",
    "pos_core_open_shift_by_id",
];

/// Expected positional bind count for each closed kind.
fn core_binds(kind: &str) -> usize {
    match kind {
        "pos_core_shifts" | "pos_core_tables" | "pos_core_cash_movements"
        | "pos_core_register_by_name" => 3,
        "pos_core_table_by_name" => 2,
        "pos_core_registers_by_kind" => 4,
        "pos_core_open_shift_by_register" | "pos_core_restaurant_register_property"
        | "pos_core_open_shift_by_id" => 1,
        _ => panic!("unknown core kind: {kind}"),
    }
}

/// `pos.ts` shifts-family reads reconstructed verbatim as the parity oracle.
fn legacy_core_sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "pos_core_shifts" => "SELECT s.*,r.name register_name,\n   s.opening_minor+COALESCE((SELECT SUM(p.amount_minor) FROM pos_payments p JOIN pos_sales b ON b.id=p.sale_id WHERE b.shift_id=s.id AND p.method='cash' AND p.state='succeeded'),0)\n   -COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rf.payout_shift_id=s.id AND p.method='cash' AND rp.state='succeeded'),0)\n   +COALESCE((SELECT SUM(m.amount_minor) FROM pos_cash_movements m WHERE m.shift_id=s.id AND m.approved_by IS NOT NULL),0) cash_expected_minor\n   FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.opened_at DESC LIMIT 100",
        "pos_core_tables" => "SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY t.name",
        "pos_core_cash_movements" => "SELECT m.*,s.register_id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY m.created_at DESC LIMIT 100",
        "pos_core_register_by_name" => "SELECT id FROM pos_registers WHERE company_id=? AND branch_id=? AND name=?",
        "pos_core_open_shift_by_register" => "SELECT id FROM pos_shifts WHERE register_id=? AND state='open'",
        "pos_core_table_by_name" => "SELECT id FROM pos_tables WHERE register_id=? AND name=?",
        "pos_core_registers_by_kind" => "SELECT id FROM pos_registers WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?<>'recipe' OR mode='restaurant')",
        "pos_core_restaurant_register_property" => "SELECT id,property_id FROM pos_registers WHERE id=? AND mode='restaurant'",
        "pos_core_open_shift_by_id" => "SELECT id FROM pos_shifts WHERE id=? AND state='open'",
        _ => return None,
    })
}

#[test]
fn core_kinds_are_mounted_and_match_the_owner_statements() {
    assert_eq!(mounted::KINDS, CORE_READ_KINDS);
    for kind in CORE_READ_KINDS {
        assert!(mounted::supported(kind), "core kind must be mounted: {kind}");
        let sql = mounted::sql(kind).unwrap_or_else(|| panic!("missing SQL for mounted kind {kind}"));
        assert!(sql.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
        assert_eq!(sql.matches('?').count(), core_binds(kind), "bind count drifted for {kind}");
        for forbidden in [
            "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
            "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties",
        ] {
            assert!(
                !sql.contains(forbidden),
                "kind must not mutate, lock or cross owners: {kind} contains {forbidden}"
            );
        }
        assert_eq!(sql, legacy_core_sql(kind).unwrap(), "mounted SQL drifted for {kind}");
    }
    assert!(mounted::sql("pos_core_unlisted").is_none());
}

async fn rows(connection: &mut PgConnection, sql: &str, values: &[Option<&str>]) -> Vec<Value> {
    let wrapped = format!("SELECT row_to_json(t)::text FROM ({}) t", pg(sql));
    let mut query = sqlx::query(AssertSqlSafe(wrapped));
    for value in values {
        query = query.bind(*value);
    }
    query
        .fetch_all(&mut *connection)
        .await
        .unwrap()
        .into_iter()
        .map(|row| serde_json::from_str::<Value>(&row.try_get::<String, _>(0).unwrap()).unwrap())
        .collect()
}

async fn ids(connection: &mut PgConnection, sql: &str, values: &[Option<&str>]) -> Vec<String> {
    rows(connection, sql, values)
        .await
        .into_iter()
        .map(|row| row.get("id").and_then(Value::as_str).unwrap_or_default().to_string())
        .collect()
}

async fn seed(connection: &mut PgConnection) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('other','branch','other','Other',NULL,'company',NULL,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1),\
         ('warehouse-other','company','other','WHO','Other','active',1);\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) VALUES \
         ('register','company','branch','warehouse','Retail','restaurant',true,1),\
         ('register-other','company','other','warehouse-other','Other','restaurant',true,1);\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version) VALUES \
         ('shift-1','register','company','branch','alice',1,0,'open',1),\
         ('shift-2','register','company','branch','bob',2,0,'closed',1),\
         ('shift-other','register-other','company','other','alice',1,0,'open',1);\
         INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,version,accounts_json,service_mode,state,total_minor) VALUES \
         ('sale','company','branch','register','shift-1','S-1','THB','2027-01-15','alice',1,1,1,'{}','dine_in','completed',100);\
         INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,state,created_by,created_at,updated_at) VALUES \
         ('pay','sale','company','cash',100,100,'succeeded','alice',1,1);\
         INSERT INTO pos_tables(id,register_id,name,seats) VALUES \
         ('table-1','register','T1',2),\
         ('table-2','register','T2',4),\
         ('table-other','register-other','TO',2);\
         INSERT INTO pos_cash_movements(id,shift_id,amount_minor,reason,created_by,approved_by,created_at) VALUES \
         ('move-1','shift-1',100,'float','alice',NULL,1),\
         ('move-2','shift-1',50,'float','bob','manager',2),\
         ('move-other','shift-other',100,'float','alice',NULL,1);",
    )
    .execute(&mut *connection)
    .await
    .expect("seed must apply on migrated schema");
}

fn with_database<F, Fut>(test: F)
where
    F: FnOnce(PgConnection) -> Fut,
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
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);
        let token = format!(
            "{}{}{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_DATABASE.fetch_add(1, Ordering::SeqCst)
        );
        let name = format!("erpx_dev_rust_pos_core_{token}");
        let lease = format!("erpx-rust-pos-core-test:{token}");
        sqlx::raw_sql(AssertSqlSafe(format!("CREATE DATABASE \"{name}\" TEMPLATE template0")))
            .execute(&mut admin)
            .await
            .unwrap();
        sqlx::raw_sql(AssertSqlSafe(format!(
            "REVOKE CONNECT ON DATABASE \"{name}\" FROM PUBLIC; COMMENT ON DATABASE \"{name}\" IS '{lease}'"
        )))
        .execute(&mut admin)
        .await
        .unwrap();
        let owned = admin_options.clone().database(&name);
        let mut connection = PgConnection::connect_with(&owned).await.unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = erpx_server_runtime::public::postgres::load_migrations(
            root,
            erpx_server_runtime::public::postgres::MigrationTrack::HoldingWrite,
        )
        .await
        .unwrap();
        erpx_server_runtime::public::postgres::migrate_database(
            &erpx_server_runtime::public::postgres::connect_pool(
                &erpx_server_runtime::public::postgres::DatabaseConfig::from_options(owned).unwrap(),
                erpx_server_runtime::public::postgres::PoolLimits {
                    max_connections: 2,
                    acquire_timeout: std::time::Duration::from_secs(5),
                },
            )
            .await
            .unwrap(),
            erpx_server_runtime::public::postgres::MigrationTrack::HoldingWrite,
            &files,
        )
        .await
        .unwrap();
        seed(&mut connection).await;
        test(connection).await;
        let ownership = sqlx::query(
            "SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=$1",
        )
        .bind(&name)
        .fetch_one(&mut admin)
        .await
        .unwrap();
        assert_eq!(ownership.get::<String, _>("owner"), "erpx_test_admin");
        assert_eq!(ownership.get::<String, _>("lease"), lease);
        sqlx::raw_sql(AssertSqlSafe(format!("DROP DATABASE \"{name}\"")))
            .execute(&mut admin)
            .await
            .unwrap();
        admin.close().await.unwrap();
    });
}

#[test]
fn core_read_statements_execute_and_scope_on_postgres() {
    with_database(|mut connection| async move {
        // Row-for-row parity with the retained literals at every scope.
        for kind in CORE_READ_KINDS {
            let mounted_sql = mounted::sql(kind).unwrap();
            let retained_sql = legacy_core_sql(kind).unwrap();
            for values in [
                vec![Some("company"), None, None],
                vec![Some("company"), Some("branch"), Some("branch")],
                vec![Some("company"), Some("other"), Some("other")],
                vec![Some("foreign"), None, None],
            ] {
                let via_owner = rows(&mut connection, mounted_sql, &values).await;
                let via_retained = rows(&mut connection, retained_sql, &values).await;
                assert_eq!(via_owner, via_retained, "parity for {kind} at {values:?}");
            }
        }
        // Substantive results: all rows, branch scope, cash-expected projection.
        let shifts = rows(
            &mut connection,
            mounted::sql(mounted::SHIFTS).unwrap(),
            &[Some("company"), None, None],
        )
        .await;
        assert_eq!(shifts.len(), 3);
        let first = shifts
            .iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some("shift-1"))
            .unwrap();
        assert_eq!(first.get("register_name").and_then(Value::as_str), Some("Retail"));
        assert_eq!(first.get("cash_expected_minor").and_then(Value::as_i64), Some(150));
        assert_eq!(
            rows(
                &mut connection,
                mounted::sql(mounted::SHIFTS).unwrap(),
                &[Some("company"), Some("other"), Some("other")],
            )
            .await
            .len(),
            1
        );
        assert!(rows(
            &mut connection,
            mounted::sql(mounted::SHIFTS).unwrap(),
            &[Some("foreign"), None, None],
        )
        .await
        .is_empty());
        assert_eq!(
            rows(
                &mut connection,
                mounted::sql(mounted::TABLES).unwrap(),
                &[Some("company"), None, None],
            )
            .await
            .len(),
            3
        );
        assert_eq!(
            rows(
                &mut connection,
                mounted::sql(mounted::CASH_MOVEMENTS).unwrap(),
                &[Some("company"), None, None],
            )
            .await
            .len(),
            3
        );
        // Keyed register/shift/table reads resolve the seeded owner rows and
        // stay closed to out-of-scope, closed or mismatched records.
        assert_eq!(
            ids(&mut connection, mounted::sql(mounted::REGISTER_BY_NAME).unwrap(),
                &[Some("company"), Some("branch"), Some("Retail")]).await,
            vec!["register"]
        );
        assert!(ids(&mut connection, mounted::sql(mounted::REGISTER_BY_NAME).unwrap(),
            &[Some("company"), Some("other"), Some("Retail")]).await.is_empty());
        assert_eq!(
            ids(&mut connection, mounted::sql(mounted::OPEN_SHIFT_BY_REGISTER).unwrap(),
                &[Some("register")]).await,
            vec!["shift-1"]
        );
        assert_eq!(
            ids(&mut connection, mounted::sql(mounted::TABLE_BY_NAME).unwrap(),
                &[Some("register"), Some("T1")]).await,
            vec!["table-1"]
        );
        assert_eq!(
            ids(&mut connection, mounted::sql(mounted::REGISTERS_BY_KIND).unwrap(),
                &[Some("company"), Some("branch"), Some("branch"), Some("recipe")]).await,
            vec!["register"]
        );
        assert_eq!(
            ids(&mut connection, mounted::sql(mounted::RESTAURANT_REGISTER_PROPERTY).unwrap(),
                &[Some("register")]).await,
            vec!["register"]
        );
        assert_eq!(
            ids(&mut connection, mounted::sql(mounted::OPEN_SHIFT_BY_ID).unwrap(),
                &[Some("shift-1")]).await,
            vec!["shift-1"]
        );
        assert!(ids(&mut connection, mounted::sql(mounted::OPEN_SHIFT_BY_ID).unwrap(),
            &[Some("shift-2")]).await.is_empty());
    });
}
