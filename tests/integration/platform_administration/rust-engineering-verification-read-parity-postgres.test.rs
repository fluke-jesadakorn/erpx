//! Same-transaction live-vs-native parity harness for the owner-local
//! engineering verification read port (`G4 / migrate.platform.engineering-read-port`).
//!
//! For each caller-unit read, the exact retained TypeScript statement (copied
//! verbatim from this owner's adapters, with `?` binds converted to `$n`) and the
//! native owner port are executed on ONE freshly opened transaction over the
//! same fixture rows. Projected rows must match, including an uncommitted write
//! made on that transaction. Nothing commits and no TS consumer is switched:
//! the TypeScript authority stays retained for this migration slice.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_platform_administration_server::public::native_engineering_verification_read::*;
use sqlx::{AssertSqlSafe, Connection, Row, postgres::PgRow};

// Verbatim from `engineering-verification.ts` (prior, predicate context read)
// and `collaboration-reply-rollout.ts` (reply rollout guard).
const LIVE_PRIOR: &str = "SELECT source_fingerprint,software_version,release_manifest_sha256 FROM engineering_verification_contexts WHERE company_id=? AND run_id=?";
const LIVE_ACTIVE: &str = "SELECT EXISTS(SELECT 1 FROM engineering_verification_contexts WHERE company_id=? AND run_id=? AND source_fingerprint=? AND software_version=? AND release_manifest_sha256=? AND enabled AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)";
const LIVE_REPLY_GUARD: &str = "SELECT company_id FROM engineering_verification_contexts WHERE company_id=? FOR SHARE";

const DDL: &str = "\
CREATE TABLE organization_nodes(id text primary key,kind text not null,slug text,display_name text,company_id text,status text);\
CREATE TABLE engineering_verification_contexts (\
 company_id text not null references organization_nodes(id),\
 run_id text not null check(run_id ~ '^[a-f0-9]{12}$'),\
 source_fingerprint text not null check(source_fingerprint ~ '^[a-f0-9]{64}$'),\
 software_version text not null check(software_version='release-'||source_fingerprint),\
 release_manifest_sha256 text not null check(release_manifest_sha256 ~ '^[a-f0-9]{64}$'),\
 enabled boolean not null default false,\
 expires_at bigint not null check(expires_at>0),\
 reason text not null check(length(reason)>=3),\
 created_at bigint not null,\
 primary key(company_id,run_id)\
);\
INSERT INTO organization_nodes VALUES\
 ('company-1','company','company-1','Company 1','company-1','active'),\
 ('company-2','company','company-2','Company 2','company-2','active');";

const RUN: &str = "abcdef123456";
const OTHER_RUN: &str = "123456abcdef";
const UNCOMMITTED_RUN: &str = "feedfeedfeed";
const NOW: i64 = 1_800_000_000_000;
const FUTURE: i64 = 9_007_199_254_740_991;

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

fn cell(row: &PgRow, index: usize) -> Option<String> {
    if let Ok(value) = row.try_get::<bool, _>(index) {
        return Some(value.to_string());
    }
    if let Ok(value) = row.try_get::<i64, _>(index) {
        return Some(value.to_string());
    }
    row.get::<Option<String>, _>(index)
}

async fn live(
    tx: &mut sqlx::PgConnection,
    sql: &str,
    values: &[&str],
) -> Vec<Vec<Option<String>>> {
    let mut query = sqlx::query(AssertSqlSafe(pg(sql)));
    for value in values {
        query = query.bind(*value);
    }
    query
        .fetch_all(&mut *tx)
        .await
        .unwrap()
        .iter()
        .map(|row| (0..row.columns().len()).map(|index| cell(row, index)).collect())
        .collect()
}

async fn seed(
    tx: &mut sqlx::PgConnection,
    company: &str,
    run: &str,
    source: &str,
    version: &str,
    manifest: &str,
    enabled: bool,
    expires_at: i64,
) {
    sqlx::query(
        "INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,\
         software_version,release_manifest_sha256,enabled,expires_at,reason,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(company)
    .bind(run)
    .bind(source)
    .bind(version)
    .bind(manifest)
    .bind(enabled)
    .bind(expires_at)
    .bind("Parity fixture context.")
    .bind(NOW)
    .execute(&mut *tx)
    .await
    .unwrap();
}

fn native_prior(value: Option<EngineeringVerificationPrior>) -> Vec<Vec<Option<String>>> {
    value
        .map(|row| {
            vec![
                Some(row.source_fingerprint),
                Some(row.software_version),
                Some(row.release_manifest_sha256),
            ]
        })
        .into_iter()
        .collect()
}

fn native_bool(value: bool) -> Vec<Vec<Option<String>>> {
    vec![vec![Some(value.to_string())]]
}

fn native_guard(values: Vec<String>) -> Vec<Vec<Option<String>>> {
    values.into_iter().map(|value| vec![Some(value)]).collect()
}

async fn native_prior_read(
    tx: &mut sqlx::PgConnection,
    company: &str,
    run: &str,
) -> Vec<Vec<Option<String>>> {
    let mut port = PgEngineeringVerificationRead::new(tx);
    native_prior(read_prior(&mut port, company, run).await.unwrap())
}

async fn native_active_read(
    tx: &mut sqlx::PgConnection,
    query: &ActiveVerificationQuery<'_>,
) -> Vec<Vec<Option<String>>> {
    let mut port = PgEngineeringVerificationRead::new(tx);
    native_bool(read_active(&mut port, query).await.unwrap())
}

async fn native_guard_read(tx: &mut sqlx::PgConnection, company: &str) -> Vec<Vec<Option<String>>> {
    let mut port = PgEngineeringVerificationRead::new(tx);
    native_guard(read_reply_rollout_guard(&mut port, company).await.unwrap())
}

#[tokio::test]
async fn engineering_verification_reads_match_retained_ts_rows_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping native engineering verification read parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    // Native statements must be verbatim mirrors of the retained TS authority.
    assert_eq!(pg(LIVE_PRIOR), PRIOR_SQL);
    assert_eq!(pg(LIVE_ACTIVE), ACTIVE_SQL);
    assert_eq!(pg(LIVE_REPLY_GUARD), REPLY_ROLLOUT_GUARD_SQL);

    fixture::with_database(1, |pool| async move {
        sqlx::raw_sql(AssertSqlSafe(DDL.to_owned()))
            .execute(pool.raw())
            .await
            .unwrap();
        let mut conn = pool.raw().acquire().await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        let source = "a".repeat(64);
        let version = format!("release-{source}");
        let manifest = "b".repeat(64);
        let other_manifest = "c".repeat(64);

        seed(&mut tx, "company-1", RUN, &source, &version, &manifest, true, FUTURE).await;
        seed(&mut tx, "company-1", OTHER_RUN, &source, &version, &manifest, false, FUTURE).await;
        seed(&mut tx, "company-2", RUN, &source, &version, &other_manifest, true, 1).await;
        // One uncommitted row: both readers must observe it on this unit.
        seed(&mut tx, "company-1", UNCOMMITTED_RUN, &source, &version, &manifest, true, FUTURE).await;

        let prior_live = live(&mut tx, LIVE_PRIOR, &["company-1", RUN]).await;
        let prior_native = native_prior_read(&mut tx, "company-1", RUN).await;
        assert!(!prior_live.is_empty());
        assert_eq!(prior_live, prior_native, "prior: live and native rows differ");

        let query = ActiveVerificationQuery {
            company_id: "company-1",
            run: RUN,
            source_fingerprint: &source,
            software_version: &version,
            file_manifest_sha256: &manifest,
        };
        let active_live = live(
            &mut tx,
            LIVE_ACTIVE,
            &["company-1", RUN, &source, &version, &manifest],
        )
        .await;
        let active_native = native_active_read(&mut tx, &query).await;
        assert_eq!(active_live, vec![vec![Some("true".into())]]);
        assert_eq!(active_live, active_native, "active: live and native rows differ");

        let guard_live = live(&mut tx, LIVE_REPLY_GUARD, &["company-1"]).await;
        let guard_native = native_guard_read(&mut tx, "company-1").await;
        assert_eq!(guard_live.len(), 3);
        assert_eq!(guard_live, guard_native, "reply guard: live and native rows differ");

        // Uncommitted write is visible to both readers on the same transaction.
        let uncommitted_live =
            live(&mut tx, LIVE_PRIOR, &["company-1", UNCOMMITTED_RUN]).await;
        let uncommitted_native = native_prior_read(&mut tx, "company-1", UNCOMMITTED_RUN).await;
        assert_eq!(uncommitted_live, uncommitted_native);
        assert_eq!(uncommitted_live.len(), 1);

        // Negatives: missing run, changed build, disabled and expired contexts.
        assert_eq!(
            live(&mut tx, LIVE_PRIOR, &["company-1", "999999999999"]).await,
            native_prior_read(&mut tx, "company-1", "999999999999").await
        );
        let changed = ActiveVerificationQuery {
            file_manifest_sha256: &other_manifest,
            ..query
        };
        let changed_live = live(
            &mut tx,
            LIVE_ACTIVE,
            &["company-1", RUN, &source, &version, &other_manifest],
        )
        .await;
        let changed_native = native_active_read(&mut tx, &changed).await;
        assert_eq!(changed_live, vec![vec![Some("false".into())]]);
        assert_eq!(changed_live, changed_native);
        let disabled = ActiveVerificationQuery {
            run: OTHER_RUN,
            ..query
        };
        assert_eq!(
            live(&mut tx, LIVE_ACTIVE, &["company-1", OTHER_RUN, &source, &version, &manifest]).await,
            native_active_read(&mut tx, &disabled).await
        );
        let expired = ActiveVerificationQuery {
            company_id: "company-2",
            file_manifest_sha256: &other_manifest,
            ..query
        };
        assert_eq!(
            live(&mut tx, LIVE_ACTIVE, &["company-2", RUN, &source, &version, &other_manifest]).await,
            native_active_read(&mut tx, &expired).await
        );
        assert_eq!(
            live(&mut tx, LIVE_REPLY_GUARD, &["company-3"]).await,
            native_guard_read(&mut tx, "company-3").await
        );

        tx.rollback().await.unwrap();
    })
    .await;
}
