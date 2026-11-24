//! Same-transaction live-vs-native parity for the VAT/tax-register fixed point
//! and count read family (`G4 / migrate.finance.native-tax-read-port`).
//!
//! For every switched read, the exact retired TypeScript literal and the owner's
//! registered `RD_TAX_*` plan run on ONE freshly opened transaction over the same
//! fixture rows. SQL must be byte-identical, ordered binds must match and both
//! projections must return identical rows, including an uncommitted write made
//! on that unit. Nothing commits, no caller is switched by this test and no
//! composition mount or TS consumer is touched.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_finance::public::native_finance_read::{
    read_statement, RD_TAX_CHART, RD_TAX_CLOSE_PROFILE, RD_TAX_CORRECTION,
    RD_TAX_CURRENT_ENTRY, RD_TAX_ENTRY, RD_TAX_ORIGINAL_ENTRY, RD_TAX_PACKAGE,
    RD_TAX_PENDING_CORRECTION_COUNT, RD_TAX_PENDING_FAMILY, RD_TAX_PREVIOUS_ENTRY,
    RD_TAX_PROFILE, RD_TAX_PROFILES,
};
use sqlx::{postgres::PgRow, AssertSqlSafe, Connection, PgConnection, Row};

// Owner-local tax tables with TEXT projections so live and native rows compare
// column-for-column. `source_type` is retained in the name for readability.
const DDL: &str = "\
CREATE TABLE finance_charts(id text primary key,company_id text not null,name text);\
CREATE TABLE finance_accounting_profiles(id text primary key,company_id text not null,statutory_chart_id text,status text,effective_date text);\
CREATE TABLE finance_statutory_packages(id text primary key,company_id text not null,chart_id text,form_code text,filing_month text);\
CREATE TABLE finance_vat_corrections(id text primary key,company_id text not null,chart_id text,family_source_id text,status text,root_entry_id text);\
CREATE TABLE finance_tax_entries(id text primary key,company_id text not null,source_type text,source_id text,status text,accounting_date text);\
CREATE TABLE finance_current_tax_entries(id text primary key,company_id text not null,source_type text,source_id text,status text);\
INSERT INTO finance_charts VALUES ('chart-1','company-1','Primary');\
INSERT INTO finance_accounting_profiles VALUES ('profile-1','company-1','chart-1','approved','2026-01-01');\
INSERT INTO finance_statutory_packages VALUES ('pkg-1','company-1','chart-1','PP30','2026-05');\
INSERT INTO finance_vat_corrections VALUES ('corr-1','company-1','chart-1','family-1','prepared','entry-1');\
INSERT INTO finance_tax_entries VALUES ('entry-1','company-1','invoice','inv-1','approved','2026-05-15');\
INSERT INTO finance_current_tax_entries VALUES ('cur-1','company-1','invoice','inv-1','approved');";

// Retired TypeScript literals copied verbatim from
// `packages/server-finance/src/adapters/finance-tax-register.ts`.
const LIVE_PROFILE: &str = "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date<=?";
const LIVE_PACKAGE: &str = "SELECT * FROM finance_statutory_packages WHERE id=? AND company_id=? AND form_code='PP30' AND filing_month IS NOT NULL";
const LIVE_CORRECTION: &str = "SELECT * FROM finance_vat_corrections WHERE id=? AND company_id=?";
const LIVE_ENTRY: &str = "SELECT * FROM finance_tax_entries WHERE id=? AND company_id=?";
const LIVE_PREVIOUS_ENTRY: &str = "SELECT * FROM finance_current_tax_entries WHERE id=? AND company_id=? AND status='approved'";
const LIVE_ORIGINAL_ENTRY: &str = "SELECT * FROM finance_current_tax_entries WHERE company_id=? AND source_type='invoice' AND source_id=? AND status='approved'";
const LIVE_PENDING_FAMILY: &str = "SELECT 1 FROM finance_vat_corrections WHERE company_id=? AND family_source_id=? AND status='prepared'";
const LIVE_PROFILES: &str = "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND status='approved'";
const LIVE_CHART: &str = "SELECT 1 FROM finance_charts WHERE id=? AND company_id=?";
const LIVE_CURRENT_ENTRY: &str = "SELECT id FROM finance_current_tax_entries WHERE company_id=? AND source_type=? AND source_id=? AND status='approved'";
const LIVE_PENDING_CORRECTION_COUNT: &str = "SELECT COUNT(*) FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=? AND c.chart_id=? AND c.status='prepared' AND t.accounting_date<=?";
const LIVE_CLOSE_PROFILE: &str = "SELECT id,statutory_chart_id,effective_date FROM finance_accounting_profiles WHERE company_id=? AND status='approved' AND effective_date<=?";

/// Positional `?` binds to Postgres `$n`. No registered statement contains a
/// literal `?` beyond these placeholders.
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

/// TEXT, int4 and int8 columns all have a stable string projection.
fn cell(row: &PgRow, index: usize) -> Option<String> {
    if let Ok(v) = row.try_get::<Option<String>, _>(index) {
        return v;
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(index) {
        return v.map(|n| n.to_string());
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(index) {
        return v.map(|n| n.to_string());
    }
    panic!("unsupported column type at {index}");
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
        .map(|row| (0..row.columns().len()).map(|index| cell(row, index)).collect())
        .collect()
}

/// Run the retired literal and the registered owner plan on the same unit and
/// require exact SQL, exact ordered binds and identical rows.
async fn parity(
    conn: &mut PgConnection,
    live_sql: &str,
    live_values: Vec<Option<String>>,
    kind: &str,
) {
    let statement = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered owner read"));
    assert_eq!(statement.sql.trim(), live_sql.trim(), "{kind}: exact SQL");
    assert_eq!(statement.bind_count, live_values.len(), "{kind}: bind count");
    let live = rows(conn, live_sql, &live_values).await;
    let native = rows(conn, statement.sql, &live_values).await;
    assert!(!live.is_empty(), "{kind}: fixture must return rows");
    assert_eq!(live, native, "{kind}: live and native rows must match");
}

fn v(value: &str) -> Option<String> {
    Some(value.to_owned())
}

#[tokio::test]
async fn tax_register_point_and_count_reads_match_native_plans_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping Finance tax-register read parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    fixture::with_database(1, |pool| async move {
        sqlx::raw_sql(AssertSqlSafe(DDL.to_owned()))
            .execute(pool.raw())
            .await
            .unwrap();
        let mut conn = pool.raw().acquire().await.unwrap();
        let mut tx = conn.begin().await.unwrap();

        parity(&mut tx, LIVE_PROFILE, vec![v("company-1"), v("chart-1"), v("2026-06-01")], RD_TAX_PROFILE).await;
        parity(&mut tx, LIVE_PACKAGE, vec![v("pkg-1"), v("company-1")], RD_TAX_PACKAGE).await;
        parity(&mut tx, LIVE_CORRECTION, vec![v("corr-1"), v("company-1")], RD_TAX_CORRECTION).await;
        parity(&mut tx, LIVE_ENTRY, vec![v("entry-1"), v("company-1")], RD_TAX_ENTRY).await;
        parity(&mut tx, LIVE_PREVIOUS_ENTRY, vec![v("cur-1"), v("company-1")], RD_TAX_PREVIOUS_ENTRY).await;
        parity(&mut tx, LIVE_ORIGINAL_ENTRY, vec![v("company-1"), v("inv-1")], RD_TAX_ORIGINAL_ENTRY).await;
        parity(&mut tx, LIVE_PENDING_FAMILY, vec![v("company-1"), v("family-1")], RD_TAX_PENDING_FAMILY).await;
        parity(&mut tx, LIVE_PROFILES, vec![v("company-1")], RD_TAX_PROFILES).await;
        parity(&mut tx, LIVE_CHART, vec![v("chart-1"), v("company-1")], RD_TAX_CHART).await;
        parity(&mut tx, LIVE_CURRENT_ENTRY, vec![v("company-1"), v("invoice"), v("inv-1")], RD_TAX_CURRENT_ENTRY).await;
        parity(&mut tx, LIVE_CLOSE_PROFILE, vec![v("company-1"), v("2026-06-01")], RD_TAX_CLOSE_PROFILE).await;
        parity(&mut tx, LIVE_PENDING_CORRECTION_COUNT, vec![v("company-1"), v("chart-1"), v("2026-06-01")], RD_TAX_PENDING_CORRECTION_COUNT).await;

        // The count read observes an uncommitted prepared correction on this
        // unit identically on both sides; the owner lock/assertion stays in TS.
        sqlx::query("INSERT INTO finance_vat_corrections VALUES ('corr-2','company-1','chart-1','family-2','prepared','entry-1')")
            .execute(&mut *tx)
            .await
            .unwrap();
        let values = vec![v("company-1"), v("chart-1"), v("2026-06-01")];
        let live = rows(&mut tx, LIVE_PENDING_CORRECTION_COUNT, &values).await;
        let native = rows(&mut tx, LIVE_PENDING_CORRECTION_COUNT, &values).await;
        assert_eq!(live, native);
        assert_eq!(live[0][0].as_deref(), Some("2"));

        // A wrong company scope returns no rows on both sides (isolation).
        let miss = rows(&mut tx, LIVE_CURRENT_ENTRY, &[v("company-2"), v("invoice"), v("inv-1")]).await;
        assert!(miss.is_empty());

        tx.rollback().await.unwrap();
    })
    .await;
}

#[test]
fn unknown_tax_register_read_kinds_fail_closed() {
    assert!(read_statement("SELECT * FROM finance_current_tax_entries").is_none());
    assert!(read_statement("finance_rd_tax_unknown").is_none());
    assert!(read_statement("").is_none());
    for kind in [
        RD_TAX_PROFILE,
        RD_TAX_PACKAGE,
        RD_TAX_CORRECTION,
        RD_TAX_ENTRY,
        RD_TAX_PREVIOUS_ENTRY,
        RD_TAX_ORIGINAL_ENTRY,
        RD_TAX_PENDING_FAMILY,
        RD_TAX_PROFILES,
        RD_TAX_CHART,
        RD_TAX_CURRENT_ENTRY,
        RD_TAX_PENDING_CORRECTION_COUNT,
        RD_TAX_CLOSE_PROFILE,
    ] {
        assert!(read_statement(kind).is_some(), "{kind}");
    }
}
