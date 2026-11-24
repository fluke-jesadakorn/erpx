//! Same-transaction live-vs-native parity harness for the Manufacturing
//! read-plan transport (`G4 / migrate.manufacturing.read-transport`).
//!
//! For each caller-unit read, the exact live TypeScript statement and the
//! owner's registered `RD_*` plan are executed on ONE freshly opened
//! transaction over the same fixture rows. Both result projections must match
//! byte-for-byte, including the unscoped-branch NULL binds and an uncommitted
//! write made on that transaction. Nothing here commits, and no caller is
//! switched by this test.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_manufacturing::public::{ReadScope, bind, native_command};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

const DDL: &str = "\
CREATE TABLE manufacturing_specifications(id text primary key,company_id text not null,name text,mode text,output_product_id text,version text,status text,updated_at text);\
CREATE TABLE manufacturing_specification_lines(id text primary key,specification_id text not null,product_id text,ordinal text);\
CREATE TABLE commerce_products(id text primary key,name text,tracking_mode text);\
CREATE TABLE manufacturing_orders(id text primary key,company_id text not null,branch_id text,specification_id text,status text,updated_at text);\
CREATE TABLE manufacturing_accounting_profiles(id text primary key,company_id text not null,status text,version text,currency text,chart_id text,inventory_account_id text,wip_account_id text,labor_account_id text,overhead_account_id text,updated_at text);\
INSERT INTO commerce_products VALUES ('output','Bounded output','serial'),('input','Material A','lot');\
INSERT INTO manufacturing_specifications VALUES ('spec-1','company-1','Bounded assembly','bom','output','1','approved','0000000000001');\
INSERT INTO manufacturing_specification_lines VALUES ('line-1','spec-1','input','0'),('line-2','spec-1','output','1');\
INSERT INTO manufacturing_orders VALUES ('order-1','company-1','branch-1','spec-1','released','0000000000002'),('order-2','company-1',NULL,'spec-1','draft','0000000000001');\
INSERT INTO manufacturing_accounting_profiles VALUES ('profile-1','company-1','active','1','USD','chart-1','inv-1','wip-1','lab-1','oh-1','0000000000003');";

/// Live `productionCommandSources` statements, copied verbatim from
/// `packages/server-manufacturing/src/adapters/production-command-sources.ts`.
const LIVE_SPECIFICATION: &str = "SELECT * FROM manufacturing_specifications WHERE id=? AND company_id=? ";
const LIVE_SPECIFICATION_LINES: &str = "SELECT * FROM manufacturing_specification_lines WHERE specification_id=? ORDER BY ordinal";
const LIVE_ORDERS: &str = "SELECT o.*,s.name specification_name,p.name output_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC";
const LIVE_ACCOUNTING_PROFILES: &str = "SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id FROM manufacturing_accounting_profiles WHERE company_id=? ORDER BY updated_at DESC";

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. None of the registered statements contains a literal `?`.
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

async fn rows(
    conn: &mut PgConnection,
    sql: &str,
    values: &[Option<String>],
) -> Vec<Vec<Option<String>>> {
    let mut query = sqlx::query(AssertSqlSafe(pg(sql)));
    for value in values {
        query = query.bind(value.clone());
    }
    query
        .fetch_all(&mut *conn)
        .await
        .unwrap()
        .iter()
        .map(|row| {
            (0..row.columns().len())
                .map(|index| row.get::<Option<String>, _>(index))
                .collect()
        })
        .collect()
}

/// Run the live statement and the registered owner plan on the same unit and
/// require identical rows and identical ordered binds.
async fn parity(
    conn: &mut PgConnection,
    live_sql: &str,
    live_values: Vec<Option<String>>,
    kind: &str,
    scope: &ReadScope<'_>,
) {
    let planned = bind(kind, scope).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
    assert_eq!(planned.sql.trim(), live_sql.trim(), "{kind}: exact SQL");
    let live = rows(conn, live_sql, &live_values).await;
    let native = rows(conn, planned.sql, &planned.binds).await;
    assert!(!live.is_empty(), "{kind}: fixture must return rows");
    assert_eq!(live, native, "{kind}: live and native rows must match");
    assert_eq!(planned.binds, live_values, "{kind}: bind order");
}

#[tokio::test]
async fn workspace_reads_match_native_plans_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping native Manufacturing read parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    fixture::with_database(1, |pool| async move {
        sqlx::raw_sql(AssertSqlSafe(DDL.to_owned()))
            .execute(pool.raw())
            .await
            .unwrap();
        let mut conn = pool.raw().acquire().await.unwrap();
        let mut tx = conn.begin().await.unwrap();

        // An uncommitted write on this unit must be visible to both sides.
        sqlx::query("INSERT INTO manufacturing_orders VALUES ('order-live','company-1','branch-1','spec-1','released','0000000000003')")
            .execute(&mut *tx)
            .await
            .unwrap();

        parity(
            &mut tx,
            LIVE_SPECIFICATION,
            vec![Some("spec-1".into()), Some("company-1".into())],
            native_command::RD_SPECIFICATION,
            &ReadScope {
                id: Some("spec-1"),
                company_id: Some("company-1"),
                ..ReadScope::default()
            },
        )
        .await;
        parity(
            &mut tx,
            LIVE_SPECIFICATION_LINES,
            vec![Some("spec-1".into())],
            native_command::RD_SPECIFICATION_LINES,
            &ReadScope {
                specification_id: Some("spec-1"),
                ..ReadScope::default()
            },
        )
        .await;
        parity(
            &mut tx,
            LIVE_ORDERS,
            vec![
                Some("company-1".into()),
                Some("branch-1".into()),
                Some("branch-1".into()),
            ],
            native_command::RD_ORDERS,
            &ReadScope {
                company_id: Some("company-1"),
                branch_id: Some("branch-1"),
                ..ReadScope::default()
            },
        )
        .await;
        // The unscoped branch binds SQL NULL twice and keeps the OR predicate.
        parity(
            &mut tx,
            LIVE_ORDERS,
            vec![Some("company-1".into()), None, None],
            native_command::RD_ORDERS,
            &ReadScope {
                company_id: Some("company-1"),
                ..ReadScope::default()
            },
        )
        .await;
        parity(
            &mut tx,
            LIVE_ACCOUNTING_PROFILES,
            vec![Some("company-1".into())],
            native_command::RD_ACCOUNTING_PROFILES,
            &ReadScope {
                company_id: Some("company-1"),
                ..ReadScope::default()
            },
        )
        .await;

        tx.rollback().await.unwrap();
    })
    .await;
}
