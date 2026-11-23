//! Native POS purchase-boundary read ports on isolated PostgreSQL.
//!
//! The retained TypeScript `purchase-boundary.ts` held nine `SELECT` literals
//! used by the purchased-command guard (restaurant add-on detection and the
//! receipt replay check). Their SQL now lives in the native POS owner registry
//! (`erpx-server-pos::adapters::boundary_reads`); the TS bridge sends a typed
//! kind and positional values only. This oracle reconstructs the retired
//! literals verbatim, asserts byte equality with the mounted owner statements,
//! then executes the owner SQL on a seeded database so an invalid SQL shape
//! cannot pass as "ported".
use erpx_server_pos::public::boundary_reads as mounted;
use erpx_server_pos::public::attachment_read_sources as attachment;
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

/// Closed owner kinds the TypeScript purchase bridge requests (literal, not
/// imported from the registry so the parity check can detect drift).
const BOUNDARY_READ_KINDS: &[&str] = &[
    "pos_boundary_sale_create_mode",
    "pos_boundary_shift_open_mode",
    "pos_boundary_sale_add_mode",
    "pos_boundary_sale_mode",
    "pos_boundary_cash_approve_mode",
    "pos_boundary_shift_mode",
    "pos_boundary_payment_mode",
    "pos_boundary_refund_mode",
    "pos_boundary_receipt_replay",
];

/// `purchase-boundary.ts` reconstructed verbatim as the parity oracle.
fn legacy_boundary_sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "pos_boundary_sale_create_mode" => "SELECT r.mode FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?",
        "pos_boundary_shift_open_mode" => "SELECT mode FROM pos_registers WHERE id=? AND company_id=?",
        "pos_boundary_sale_add_mode" => "SELECT CASE WHEN r.mode='restaurant' OR c.kind='recipe' THEN 'restaurant' ELSE r.mode END mode FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id LEFT JOIN pos_catalog c ON c.product_id=? AND c.company_id=s.company_id WHERE s.id=? AND s.company_id=?",
        "pos_boundary_sale_mode" => "SELECT CASE WHEN r.mode='restaurant' OR EXISTS(SELECT 1 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=s.id AND f.kind='recipe') THEN 'restaurant' ELSE r.mode END mode FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?",
        "pos_boundary_cash_approve_mode" => "SELECT r.mode FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id JOIN pos_registers r ON r.id=s.register_id WHERE m.id=? AND s.company_id=?",
        "pos_boundary_shift_mode" => "SELECT r.mode FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?",
        "pos_boundary_payment_mode" => "SELECT r.mode FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE p.id=? AND s.company_id=?",
        "pos_boundary_refund_mode" => "SELECT r.mode FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE f.id=? AND s.company_id=?",
        "pos_boundary_receipt_replay" => "SELECT 1 FROM pos_receipts WHERE id=? AND company_id=?",
        _ => return None,
    })
}

#[test]
fn boundary_kinds_are_mounted_and_match_the_owner_statements() {
    assert_eq!(mounted::KINDS.len(), BOUNDARY_READ_KINDS.len());
    assert_eq!(mounted::KINDS, BOUNDARY_READ_KINDS);
    for kind in BOUNDARY_READ_KINDS {
        assert!(mounted::supported(kind), "boundary kind must be mounted: {kind}");
        let sql = mounted::sql(kind).unwrap_or_else(|| panic!("missing SQL for mounted kind {kind}"));
        assert!(sql.starts_with("SELECT"), "kind must be SELECT-only: {kind}");
        assert!(sql.contains('?'), "kind must be parameterized: {kind}");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "kind must not mutate: {kind} contains {forbidden}");
        }
        assert_eq!(sql, legacy_boundary_sql(kind).unwrap(), "mounted SQL drifted for {kind}");
    }
    assert!(mounted::sql("pos_boundary_unlisted").is_none());
}

async fn seed(connection: &mut PgConnection) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1);\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) VALUES \
         ('register','company','branch','warehouse','Retail','restaurant',true,1);\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version) VALUES \
         ('shift','register','company','branch','alice',1,0,'open',1);\
         INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,version,accounts_json,service_mode,state,total_minor) VALUES \
         ('sale','company','branch','register','shift','S-1','THB','2027-01-15','alice',1,1,1,'{}','dine_in','completed',100);\
         INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,state,created_by,created_at,updated_at) VALUES \
         ('pay','sale','company','cash',100,100,'succeeded','alice',1,1);\
         INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,reason,lines_json,amount_minor,state,requested_by,created_at,accounting_date,kind) VALUES \
         ('refund','sale','company','branch','review','[]',20,'requested','alice',1,'2027-01-15','sale');\
         INSERT INTO pos_cash_movements(id,shift_id,amount_minor,reason,created_by,created_at) VALUES \
         ('cash','shift',100,'deposit','alice',1);\
         INSERT INTO pos_receipts(id,company_id,request_hash,result_json,created_at) VALUES \
         ('receipt','company','hash','{}',1);",
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
        let name = format!("erpx_dev_rust_pos_boundary_{token}");
        let lease = format!("erpx-rust-pos-boundary-test:{token}");
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

async fn first_col(connection: &mut PgConnection, sql: &str, values: &[&str]) -> Option<String> {
    let mut query = sqlx::query(AssertSqlSafe(format!("SELECT ({})::text", pg(sql))));
    for value in values {
        query = query.bind(*value);
    }
    query
        .fetch_optional(connection)
        .await
        .unwrap()
        .and_then(|row| row.try_get::<Option<String>, _>(0).unwrap())
}

#[test]
fn boundary_read_statements_execute_on_postgres() {
    with_database(|mut connection| async move {
        let cases: &[(&str, &[&str], &str)] = &[
            ("pos_boundary_sale_create_mode", &["shift", "company"], "restaurant"),
            ("pos_boundary_shift_open_mode", &["register", "company"], "restaurant"),
            ("pos_boundary_sale_add_mode", &["product", "sale", "company"], "restaurant"),
            ("pos_boundary_sale_mode", &["sale", "company"], "restaurant"),
            ("pos_boundary_cash_approve_mode", &["cash", "company"], "restaurant"),
            ("pos_boundary_shift_mode", &["shift", "company"], "restaurant"),
            ("pos_boundary_payment_mode", &["pay", "company"], "restaurant"),
            ("pos_boundary_refund_mode", &["refund", "company"], "restaurant"),
        ];
        for (kind, values, expected) in cases {
            let sql = mounted::sql(kind).unwrap();
            assert_eq!(sql.matches('?').count(), values.len(), "bind arity for {kind}");
            assert_eq!(
                first_col(&mut connection, sql, values).await.as_deref(),
                Some(*expected),
                "owner SQL result for {kind}"
            );
        }
        // Receipt replay is an existence check; its closed kind must resolve.
        let replay = mounted::sql("pos_boundary_receipt_replay").unwrap();
        assert_eq!(replay.matches('?').count(), 2);
        assert!(first_col(&mut connection, replay, &["receipt", "company"]).await.is_some());
        assert!(first_col(&mut connection, replay, &["missing", "company"]).await.is_none());
    });
}

/// `attachment-read-sources.ts` reconstructed verbatim: one closed literal per
/// original owner kind, with the table fixed by the allow-listed kind only.
fn legacy_attachment_sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "pos_attachment_source_pos_sale" => "SELECT to_jsonb(source)::text image FROM pos_sales source WHERE source.id=? AND source.company_id=?",
        "pos_attachment_source_pos_refund" => "SELECT to_jsonb(source)::text image FROM pos_refunds source WHERE source.id=? AND source.company_id=?",
        "pos_attachment_source_pos_shift" => "SELECT to_jsonb(source)::text image FROM pos_shifts source WHERE source.id=? AND source.company_id=?",
        _ => return None,
    })
}

#[test]
fn attachment_source_read_kinds_are_mounted_and_execute_on_postgres() {
    const KINDS: &[&str] = &[
        "pos_attachment_source_pos_sale",
        "pos_attachment_source_pos_refund",
        "pos_attachment_source_pos_shift",
    ];
    assert_eq!(attachment::KINDS, KINDS);
    for kind in KINDS {
        assert!(attachment::supported(kind), "attachment kind must be mounted: {kind}");
        let sql = attachment::sql(kind).unwrap();
        assert!(sql.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
        assert_eq!(sql.matches('?').count(), 2, "kind must carry id/Company scope: {kind}");
        assert_eq!(sql, legacy_attachment_sql(kind).unwrap(), "mounted SQL drifted for {kind}");
    }
    assert!(attachment::sql("pos_attachment_source_unlisted").is_none());
    with_database(|mut connection| async move {
        let cases: &[(&str, &str)] = &[
            ("pos_attachment_source_pos_sale", "sale"),
            ("pos_attachment_source_pos_refund", "refund"),
            ("pos_attachment_source_pos_shift", "shift"),
        ];
        for (kind, source) in cases {
            let sql = attachment::sql(kind).unwrap();
            assert!(first_col(&mut connection, sql, &[source, "company"]).await.is_some(), "owner SQL result for {kind}");
            assert!(first_col(&mut connection, sql, &[source, "other-company"]).await.is_none(), "Company fence for {kind}");
        }
    });
}
