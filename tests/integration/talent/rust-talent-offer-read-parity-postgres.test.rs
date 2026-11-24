//! Same-transaction live-vs-native parity for the Talent offer/candidate
//! read-plan family (`G4 / migrate.talent.offer-read-port`).
//!
//! For each caller-unit read, the exact live TypeScript statement and the
//! owner's registered `RD_*` plan run on ONE freshly opened transaction over
//! the same fixture rows. Both projections must match byte-for-byte, including
//! an uncommitted write made on that unit. Nothing here commits, no caller is
//! switched by this test and no composition mount or TS consumer is touched.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_talent::public::native_talent_read_sql::{
    read_statement, RD_CANDIDATE_EVENTS, RD_CANDIDATE_SOURCE_IMAGE,
    RD_CANDIDATE_SOURCE_IMAGE_FILE, RD_OFFER_EVENTS,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

// Owner-local Talent tables only; TEXT columns keep every projection readable
// as a string so live and native rows can be compared column-for-column.
const DDL: &str = "\
CREATE TABLE talent_requisitions(id text primary key,company_id text not null,branch_id text,department_id text,job_title text,version text,status text,updated_at text);\
CREATE TABLE talent_applications(id text primary key,company_id text not null,requisition_id text,candidate_id text,owner_id text,version text,status text,updated_at text);\
CREATE TABLE talent_candidates(id text primary key,company_id text not null,legal_name text,email text,phone text,consent_evidence text,consented_at text,retention_until text,status text,created_by text,version text,created_at text,updated_at text);\
CREATE TABLE talent_candidate_events(id text primary key,candidate_id text not null,version text,action text,actor_id text,reason text,status text,created_at text);\
CREATE TABLE talent_offers(id text primary key,company_id text not null,application_id text not null,application_version text,currency text,base_amount_minor text,start_date text,expires_on text,terms text,status text,version text,created_at text,updated_at text);\
CREATE TABLE talent_offer_events(id text primary key,offer_id text not null,version text,action text,actor_id text,reason text,status text,created_at text);\
INSERT INTO talent_requisitions VALUES ('req-1','company-1','branch-1',NULL,'Service specialist','2','open','1');\
INSERT INTO talent_candidates VALUES ('cand-1','company-1','Anong Candidate','anong@example.test','0812345678','Signed consent','1','2027-01-01','active','recruiter','1','1','1');\
INSERT INTO talent_applications VALUES ('app-1','company-1','req-1','cand-1','recruiter','1','selected','1');\
INSERT INTO talent_offers VALUES ('offer-1','company-1','app-1','1','THB','4500050','2026-12-01','2026-12-15','Twelve-month contract','accepted','2','1','1');\
INSERT INTO talent_candidate_events VALUES ('cand-ev-1','cand-1','1','consent','recruiter','Candidate consent recorded','active','1');\
INSERT INTO talent_offer_events VALUES ('offer-ev-1','offer-1','1','approve-offer','reviewer','Independent approval','approved','1');";

/// Live `talentOwnerSourceQuery` candidate image literals copied verbatim from
/// `packages/server-talent/src/adapters/purchases.ts` (file=false / file=true).
const LIVE_CANDIDATE_IMAGE: &str =
    "SELECT to_jsonb(c)::text image FROM talent_candidates c WHERE c.id=? AND c.company_id=?";
const LIVE_CANDIDATE_IMAGE_FILE: &str =
    "SELECT (to_jsonb(c)-'version'-'updated_at')::text image FROM talent_candidates c WHERE c.id=? AND c.company_id=?";
/// The candidate/offer event-history projections used by the TS detail reads,
/// mirroring the existing `talent_rd_application_events` shape.
const LIVE_CANDIDATE_EVENTS: &str =
    "SELECT * FROM talent_candidate_events WHERE candidate_id=? ORDER BY version";
const LIVE_OFFER_EVENTS: &str =
    "SELECT * FROM talent_offer_events WHERE offer_id=? ORDER BY version";

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. No registered statement contains a literal `?` in its SQL text
/// beyond these positional placeholders.
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

#[tokio::test]
async fn offer_and_candidate_reads_match_native_plans_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping Talent offer/candidate read parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    fixture::with_database(1, |pool| async move {
        sqlx::raw_sql(AssertSqlSafe(DDL.to_owned()))
            .execute(pool.raw())
            .await
            .unwrap();
        let mut conn = pool.raw().acquire().await.unwrap();
        let mut tx = conn.begin().await.unwrap();

        // Uncommitted writes on this unit must be visible to both sides.
        sqlx::query("INSERT INTO talent_candidate_events VALUES ('cand-ev-2','cand-1','2','hold','legal-a','Privacy hold recorded','active','2')")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO talent_offer_events VALUES ('offer-ev-2','offer-1','2','accept-offer','recruiter','Candidate accepted','accepted','2')")
            .execute(&mut *tx)
            .await
            .unwrap();

        parity(
            &mut tx,
            LIVE_CANDIDATE_IMAGE,
            vec![Some("cand-1".into()), Some("company-1".into())],
            RD_CANDIDATE_SOURCE_IMAGE,
        )
        .await;
        parity(
            &mut tx,
            LIVE_CANDIDATE_IMAGE_FILE,
            vec![Some("cand-1".into()), Some("company-1".into())],
            RD_CANDIDATE_SOURCE_IMAGE_FILE,
        )
        .await;
        parity(
            &mut tx,
            LIVE_CANDIDATE_EVENTS,
            vec![Some("cand-1".into())],
            RD_CANDIDATE_EVENTS,
        )
        .await;
        parity(
            &mut tx,
            LIVE_OFFER_EVENTS,
            vec![Some("offer-1".into())],
            RD_OFFER_EVENTS,
        )
        .await;

        // The candidate image exposes the full source row; the file variant
        // strips version/updated_at; event history is version-ordered.
        let full = rows(&mut tx, LIVE_CANDIDATE_IMAGE, &[Some("cand-1".into()), Some("company-1".into())]).await;
        let file = rows(&mut tx, LIVE_CANDIDATE_IMAGE_FILE, &[Some("cand-1".into()), Some("company-1".into())]).await;
        assert!(full[0][0].as_deref().unwrap().contains("Anong Candidate"));
        assert!(full[0][0].as_deref().unwrap().contains("\"version\""));
        assert!(file[0][0].as_deref().unwrap().contains("Anong Candidate"));
        assert!(!file[0][0].as_deref().unwrap().contains("\"version\""));
        let events = rows(&mut tx, LIVE_OFFER_EVENTS, &[Some("offer-1".into())]).await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0][2].as_deref(), Some("1"));
        assert_eq!(events[1][2].as_deref(), Some("2"));

        // A wrong company scope returns no rows on both sides (isolation).
        let miss = rows(&mut tx, LIVE_CANDIDATE_IMAGE, &[Some("cand-1".into()), Some("company-2".into())]).await;
        assert!(miss.is_empty());

        tx.rollback().await.unwrap();
    })
    .await;
}

#[test]
fn unknown_offer_candidate_read_kinds_fail_closed() {
    assert!(read_statement("SELECT * FROM talent_offers").is_none());
    assert!(read_statement("talent_rd_unknown").is_none());
    for kind in [
        RD_CANDIDATE_SOURCE_IMAGE,
        RD_CANDIDATE_SOURCE_IMAGE_FILE,
        RD_CANDIDATE_EVENTS,
        RD_OFFER_EVENTS,
    ] {
        assert!(read_statement(kind).is_some());
    }
}
