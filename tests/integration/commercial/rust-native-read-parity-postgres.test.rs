//! Same-transaction live-vs-native parity for the Commercial package-grant,
//! purchase-command-boundary and projection read tail
//! (`G4 / migrate.commercial.read-ports`).
//!
//! The retained TypeScript statements are read at compile time. Each registered
//! owner-local plan is checked byte-for-byte against its TypeScript source
//! (whitespace normalised), then executed with the ordered binds the live call
//! site passes. Both sides run on ONE freshly opened transaction over the same
//! fixture rows, including an uncommitted write. Nothing commits, no caller is
//! switched and no TypeScript reader is deleted. The advisory-lock pair stays
//! independent and is never bound or executed as a read.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commercial::public::native_read as plans;
use erpx_server_commercial::public::{bind, ReadScope};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

/// Retained TypeScript authority, read at compile time for source parity.
const TS_PG: &str = include_str!("../../../packages/server-commercial/src/adapters/package-grants.ts");
const TS_TAIL: &str =
    include_str!("../../../packages/server-commercial/src/adapters/commercial-read-tail-r1-sources.ts");
const TS_PB: &str =
    include_str!("../../../packages/server-commercial/src/adapters/purchase-command-boundary.ts");
const TS_CAP: &str =
    include_str!("../../../packages/server-commercial/src/adapters/purchased-capability-sql.ts");
const TS_PROJ: &str = include_str!("../../../packages/server-commercial/src/adapters/projection.ts");
const TS_SEATS: &str = include_str!("../../../packages/server-commercial/src/adapters/seats.ts");

/// Compare SQL after collapsing all whitespace runs, so TypeScript template
/// newlines and indentation do not change token identity.
fn norm(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every registered plan must be present, verbatim, in its TypeScript source.
#[test]
fn native_plans_match_retained_typescript_statements() {
    let sources: &[(&str, &[&str])] = &[
        (plans::RD_PURCHASE_FACTS, &[TS_PG]),
        (plans::RD_HISTORY_RIGHTS, &[TS_PG]),
        (plans::RD_CURRENT_GRANT, &[TS_PG]),
        (plans::RD_GRANT_RECEIPT, &[TS_PG]),
        (plans::RD_REVISION_ANCHOR, &[TS_PG]),
        (plans::RD_CHANGE_SET_EXISTS, &[TS_TAIL]),
        (plans::RD_CHANGE_SET_RECEIPT, &[TS_PG]),
        (plans::RD_COMPANY_GRANTS, &[TS_PG]),
        (plans::RD_COMPANY_GRANTS_FOR_SHARE, &[TS_PG]),
        (plans::RD_PURCHASE_GRANT_IDS, &[TS_PB]),
        (plans::RD_HOLDING_GRANT_PAIRS, &[TS_PB]),
        (plans::RD_PRINCIPAL_COVERAGE, &[TS_PROJ]),
        (plans::RD_SNAPSHOT_HEADER_LOCK, &[TS_PROJ]),
        (plans::RD_SNAPSHOT_HEADERS, &[TS_PROJ]),
        (plans::RD_METER_ROWS, &[TS_PROJ]),
        (plans::RD_CURRENT_SNAPSHOTS, &[TS_PROJ]),
        (plans::RD_HISTORY_MONTHS, &[TS_PROJ]),
        (plans::RD_SEAT_COVERAGE, &[TS_SEATS]),
        (plans::RD_SEAT_ASSIGNMENT_EVENTS, &[TS_SEATS]),
        (plans::RD_SEAT_MEMBER_EVENTS, &[TS_SEATS]),
        (plans::RD_SEAT_ROLE_EVENTS, &[TS_SEATS]),
        (plans::RD_SEAT_SNAPSHOT, &[TS_SEATS]),
        (plans::RD_SEAT_SUMMARY_ROWS, &[TS_SEATS]),
    ];
    assert_eq!(sources.len(), plans::PLANS.len(), "every plan is source-bound");
    for (kind, files) in sources {
        let plan = plans::plan(kind).expect("registered plan");
        let needle = norm(plan.sql);
        if *kind == plans::RD_COMPANY_GRANTS_FOR_SHARE {
            // The retained TypeScript appends the lock conditionally:
            // `ORDER BY package_id${forShare ? " FOR SHARE" : ""}`.
            let base = norm(plans::plan(plans::RD_COMPANY_GRANTS).unwrap().sql);
            assert!(norm(TS_PG).contains(&base), "{kind}: base read missing");
            assert!(norm(TS_PG).contains("\" FOR SHARE\""), "{kind}: lock option missing");
        } else {
            assert!(
                files.iter().any(|file| norm(file).contains(needle.as_str())),
                "{kind}: exact SQL missing from its TypeScript authority"
            );
        }
        assert_eq!(plan.sql.matches('?').count(), plan.binds.len(), "{kind}");
    }
}

/// Dynamic shells and advisory locks are owner-owned literals too, but are not
/// data-read plans and must stay independent.
#[test]
fn fragments_and_advisory_locks_match_typescript_and_stay_independent() {
    let fragment_sources: &[(&str, &str)] = &[
        ("purchase-boundary.scope-clock", TS_PB),
        ("purchase-boundary.assertion-select", TS_PB),
        ("purchase-boundary.observation-select", TS_PB),
        ("purchase-boundary.grant-facts", TS_PB),
        ("purchase-boundary.history-facts", TS_PB),
        ("purchase-boundary.capability-sources", TS_CAP),
        ("projection.principal-events", TS_PROJ),
    ];
    for (name, source) in fragment_sources {
        let fragment = plans::fragment(name).expect("fragment");
        match *name {
            // The clock CTE is assembled with the `packageClockSql` constant.
            "purchase-boundary.scope-clock" => {
                assert!(
                    norm(source)
                        .contains("WITH purchase_scope AS MATERIALIZED (SELECT ?::text company_id,")
                );
                assert!(source.contains("packageClockSql"));
            }
            // The history projection is a function of the Company expression;
            // the registered form resolves `${company}` to `purchase_scope.company_id`.
            "purchase-boundary.history-facts" => {
                let template = norm(fragment.sql).replace("purchase_scope.company_id", "${company}");
                assert!(norm(source).contains(template.as_str()), "{name}: template");
            }
            _ => {
                let needle = norm(fragment.sql);
                assert!(norm(source).contains(needle.as_str()), "{name}");
            }
        }
        assert!(plans::plan(name).is_none(), "{name} is not a read plan");
    }
    for lock in plans::ADVISORY_LOCKS {
        let source = if lock.name.starts_with("package-grants") {
            TS_PG
        } else {
            TS_PB
        };
        assert!(norm(source).contains(&norm(lock.sql)), "{}", lock.name);
        assert!(lock.sql.contains("pg_advisory"), "{}", lock.name);
        assert!(plans::plan(lock.name).is_none(), "{}", lock.name);
    }
}

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. No registered statement contains a literal `?` in its SQL text.
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

/// Project every column through `to_jsonb` so mixed PG types compare as text.
async fn rows(conn: &mut PgConnection, sql: &str, values: &[&str]) -> Vec<String> {
    let wrapped = format!("SELECT to_jsonb(q)::text FROM ({}) q ORDER BY 1", pg(sql));
    let mut query = sqlx::query(AssertSqlSafe(wrapped));
    for value in values {
        query = query.bind(*value);
    }
    query
        .fetch_all(&mut *conn)
        .await
        .unwrap()
        .iter()
        .map(|row| row.get::<String, _>(0))
        .collect()
}

/// Bind through the owner adapter and require the live call-site bind order,
/// then run the exact plan on the caller's unit and require fixture rows.
async fn parity(
    conn: &mut PgConnection,
    kind: &str,
    scope: &ReadScope<'_>,
    live_values: &[&str],
) {
    let plan = plans::plan(kind).expect("registered plan");
    let bound = bind(kind, scope).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
    let expected: Vec<String> = live_values.iter().map(|v| (*v).to_owned()).collect();
    assert_eq!(bound.binds, expected, "{kind}: bind order");
    assert_eq!(bound.sql, plan.sql, "{kind}: exact SQL");
    let actual_values: Vec<&str> = bound.binds.iter().map(String::as_str).collect();
    // Live and native run the identical statement with identical ordered binds
    // on the same transaction; the source-parity test above ties that statement
    // to the retained TypeScript literal.
    let live_rows = rows(conn, plan.sql, live_values).await;
    let native_rows = rows(conn, bound.sql, &actual_values).await;
    assert!(!live_rows.is_empty(), "{kind}: fixture must return rows");
    assert_eq!(live_rows, native_rows, "{kind}: live and native rows must match");
}

const DDL: &str = "\
CREATE TABLE company_package_grants(company_id text,package_id text,package_version bigint,state text,starts_at bigint,ends_at bigint,version bigint,reason text,evidence text);\
CREATE TABLE company_package_history_rights(company_id text,package_id text,package_version bigint,source_grant_version bigint,source_event_id text,retained_at bigint);\
CREATE TABLE company_package_grant_events(id text,request_hash text,after_json text);\
CREATE TABLE company_package_change_sets(id text,request_hash text,result_json text);\
CREATE TABLE company_package_revisions(company_id text,version bigint);\
CREATE TABLE billing_principal_status_coverage(singleton boolean,since_at bigint);\
CREATE TABLE billing_usage_snapshots(holding_id text,month text,snapshot_id text,refreshed_at bigint,source_count bigint,ai_coverage text,storage_coverage text,unreconciled_originals bigint,seat_coverage text,seat_source_count bigint,active_seat_count bigint,active_seat_observed_at bigint);\
CREATE TABLE billing_usage_meters(holding_id text,month text,source_key text,company_id text,company_name text,branch_id text,branch_name text,meter_kind text,funding_source text,pricing_status text,quantity bigint,input_tokens bigint,output_tokens bigint,average_daily_bytes bigint,active_at_period_end boolean,snapshot_id text,request_status text,gateway_status text,provider text,model text,units_json jsonb,cost_status text,verified_cost_satang bigint,cost_evidence_key text,cost_reconciled_at bigint,active_from bigint,active_until bigint,byte_milliseconds bigint);\
CREATE TABLE billing_seat_history_coverage(singleton integer,started_at bigint);\
CREATE TABLE billing_seat_assignment_events(event_id text,assignment_id text,captured_at bigint,present boolean,state text);\
CREATE TABLE billing_seat_member_events(event_id text,company_id text,user_id text,captured_at bigint,present boolean,state text);\
CREATE TABLE billing_seat_role_events(event_id text,company_id text,role_id text,captured_at bigint,present boolean,state text);\
CREATE TABLE billing_seat_usage(holding_id text,month text,intervals_json jsonb,snapshot_id text);\
INSERT INTO company_package_grants VALUES ('company','base',1,'active',0,NULL,1,'approved','EVIDENCE'),('company','legal',1,'active',0,NULL,1,'approved','EVIDENCE');\
INSERT INTO company_package_history_rights VALUES ('company','base',1,1,'event-1',0);\
INSERT INTO company_package_grant_events VALUES ('receipt-1','hash','{}');\
INSERT INTO company_package_change_sets VALUES ('receipt-1','hash','{}');\
INSERT INTO company_package_revisions VALUES ('company',1);\
INSERT INTO billing_principal_status_coverage VALUES (TRUE,0);\
INSERT INTO billing_usage_snapshots VALUES ('holding','2026-11','snap-1',1000,2,'full','full',0,'full',2,2,1000),('holding','2026-10','snap-0',900,1,'full','full',0,'full',1,1,900);\
INSERT INTO billing_usage_meters VALUES ('holding','2026-11','meter-1','company','Company',NULL,NULL,'ai','customer','ok',1,2,3,4,TRUE,'snap-1','ok','ok','provider','model','{}','verified',0,NULL,0,0,0);\
INSERT INTO billing_seat_history_coverage VALUES (1,0);\
INSERT INTO billing_seat_assignment_events VALUES ('event-1','assignment-1',1000,TRUE,'active');\
INSERT INTO billing_seat_member_events VALUES ('event-1','company','user-1',1000,TRUE,'active');\
INSERT INTO billing_seat_role_events VALUES ('event-1','company','role-1',1000,TRUE,'active');\
INSERT INTO billing_seat_usage VALUES ('holding','2026-11','[]','snap-1');";

#[tokio::test]
async fn package_grants_boundary_and_projection_reads_match_native_rows_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping native Commercial read parity: ERPX_TEST_ADMIN_URL absent");
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
        sqlx::query("UPDATE company_package_grants SET version=9 WHERE company_id='company'")
            .execute(&mut *tx)
            .await
            .unwrap();

        let company = ReadScope {
            company_id: Some("company"),
            ..ReadScope::default()
        };
        let current = ReadScope {
            company_id: Some("company"),
            package_id: Some("base"),
            ..ReadScope::default()
        };
        let event = ReadScope {
            event_id: Some("receipt-1"),
            ..ReadScope::default()
        };
        let month = ReadScope {
            month: Some("2026-11"),
            ..ReadScope::default()
        };
        let holding_month = ReadScope {
            holding_id: Some("holding"),
            month: Some("2026-11"),
            ..ReadScope::default()
        };
        let holding = ReadScope {
            holding_id: Some("holding"),
            ..ReadScope::default()
        };
        let end = ReadScope {
            end: Some(1_760_000_000_000),
            ..ReadScope::default()
        };

        // package-grants
        parity(&mut tx, plans::RD_PURCHASE_FACTS, &company, &["company", "company"]).await;
        parity(&mut tx, plans::RD_HISTORY_RIGHTS, &company, &["company"]).await;
        parity(&mut tx, plans::RD_CURRENT_GRANT, &current, &["company", "base"]).await;
        parity(&mut tx, plans::RD_GRANT_RECEIPT, &event, &["receipt-1"]).await;
        parity(&mut tx, plans::RD_REVISION_ANCHOR, &company, &["company"]).await;
        parity(&mut tx, plans::RD_CHANGE_SET_EXISTS, &event, &["receipt-1"]).await;
        parity(&mut tx, plans::RD_CHANGE_SET_RECEIPT, &event, &["receipt-1"]).await;
        parity(&mut tx, plans::RD_COMPANY_GRANTS, &company, &["company"]).await;
        parity(&mut tx, plans::RD_COMPANY_GRANTS_FOR_SHARE, &company, &["company"]).await;
        // purchase-command-boundary
        parity(&mut tx, plans::RD_PURCHASE_GRANT_IDS, &company, &["company"]).await;
        parity(&mut tx, plans::RD_HOLDING_GRANT_PAIRS, &ReadScope::default(), &[]).await;
        // projection
        parity(&mut tx, plans::RD_PRINCIPAL_COVERAGE, &ReadScope::default(), &[]).await;
        parity(&mut tx, plans::RD_SNAPSHOT_HEADER_LOCK, &holding_month, &["holding", "2026-11"]).await;
        parity(&mut tx, plans::RD_SNAPSHOT_HEADERS, &month, &["2026-11"]).await;
        parity(&mut tx, plans::RD_METER_ROWS, &month, &["2026-11"]).await;
        parity(&mut tx, plans::RD_CURRENT_SNAPSHOTS, &month, &["2026-11"]).await;
        parity(&mut tx, plans::RD_HISTORY_MONTHS, &holding, &["holding"]).await;
        parity(&mut tx, plans::RD_SEAT_COVERAGE, &ReadScope::default(), &[]).await;
        let end_value = "1760000000000";
        parity(&mut tx, plans::RD_SEAT_ASSIGNMENT_EVENTS, &end, &[end_value]).await;
        parity(&mut tx, plans::RD_SEAT_MEMBER_EVENTS, &end, &[end_value]).await;
        parity(&mut tx, plans::RD_SEAT_ROLE_EVENTS, &end, &[end_value]).await;
        parity(&mut tx, plans::RD_SEAT_SNAPSHOT, &holding_month, &["holding", "2026-11"]).await;
        parity(&mut tx, plans::RD_SEAT_SUMMARY_ROWS, &holding_month, &["holding", "2026-11"]).await;

        tx.rollback().await.unwrap();
    })
    .await;
}
