//! Finance-owned generic Money snapshot producer on migrated PostgreSQL:
//! serializable locks, exact insert/retained image, replay denial, scale/rate
//! failures and Company isolation.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_finance::public::money_snapshot::{
    MoneySnapshotInput, lock_money_snapshot_sources, prepare_money_snapshot,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, TimestampMillis};
use sqlx::PgConnection;

const NOW: i64 = 1_800_000_000_000;

struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
    }
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}
fn other_company() -> CompanyId {
    CompanyId::new("other").unwrap()
}

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('other','company','other','Other',NULL,'other',NULL,'active',1);\
         INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) \
         VALUES('company','THB',2,'fixture',0),('company','XAG',4,'fixture',0),('other','THB',2,'fixture',0);\
         INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) \
         VALUES('company-old','company','THB','2026-01-01',30,'fixture',10,'old'),\
         ('company-new','company','THB','2026-06-01',40,'fixture',20,'new'),\
         ('company-xag','company','XAG','2026-01-01',2,'fixture',1,'xag'),\
         ('other-rate','other','THB','2026-01-01',10,'fixture',1,'other');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn serial(c: &mut PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(c)
        .await
        .unwrap();
}

fn input(
    source_id: &str,
    date: &str,
    currency: &str,
    currency_scale: Option<i64>,
    amount_units: i64,
) -> MoneySnapshotInput {
    MoneySnapshotInput {
        source_type: "ecommerce_listing".into(),
        source_id: source_id.into(),
        amount_kind: "unit_price".into(),
        transaction_date: date.into(),
        currency: currency.into(),
        currency_scale,
        amount_units,
    }
}

fn denied<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        _ => panic!("expected {code}"),
    }
}

fn persistence_denied<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Persistence(error)) => assert_eq!(error.code, code),
        _ => panic!("expected persistence {code}"),
    }
}

#[tokio::test]
async fn inserts_retains_and_rejects_replay_of_the_exact_money_source() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        lock_money_snapshot_sources(&mut tx).await.unwrap();
        let prepared = prepare_money_snapshot(
            &mut tx,
            &company(),
            input("listing-1", "2026-07-15", "THB", Some(2), 12_345),
            &TestClock,
        )
        .await
        .unwrap();
        let evidence = prepared.evidence();
        assert_eq!(evidence.source_type, "ecommerce_listing");
        assert_eq!(evidence.source_id, "listing-1");
        assert_eq!(evidence.amount_kind, "unit_price");
        assert_eq!(evidence.transaction_date, "2026-07-15");
        assert_eq!(evidence.currency, "THB");
        assert_eq!(evidence.currency_scale, 2);
        assert_eq!(evidence.amount_units, "12345");
        assert_eq!(evidence.usd_units, "309");
        assert_eq!(evidence.rate_id.as_deref(), Some("company-new"));
        assert!(!evidence.used_latest_fallback);
        assert_eq!(evidence.captured_at, NOW);
        let retained = prepared.persist(&mut tx).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        let stored: (String, String, String, String, String, i32, String, String, Option<String>, bool, i64) =
            sqlx::query_as(
                "SELECT source_type,source_id,amount_kind,transaction_date::text,currency,\
                 currency_scale::int,amount_units::text,usd_units::text,fx_rate_id,\
                 used_latest_fallback,captured_at FROM public.money_transaction_snapshots \
                 WHERE company_id='company' AND source_type='ecommerce_listing' \
                 AND source_id='listing-1' AND amount_kind='unit_price'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            stored,
            (
                "ecommerce_listing".into(),
                "listing-1".into(),
                "unit_price".into(),
                "2026-07-15".into(),
                "THB".into(),
                2,
                "12345".into(),
                "309".into(),
                Some("company-new".into()),
                false,
                NOW,
            )
        );
        // Replaying the same immutable source key is a source conflict, never
        // an append or update.
        denied(
            prepare_money_snapshot(
                &mut tx,
                &company(),
                input("listing-1", "2026-07-15", "THB", Some(2), 12_345),
                &TestClock,
            )
            .await,
            "MONEY_SNAPSHOT_CHANGED",
        );
        // Signed minor units keep the retained refund/reversal behaviour.
        let credit = prepare_money_snapshot(
            &mut tx,
            &company(),
            input("listing-refund", "2026-07-15", "THB", Some(2), -12_345),
            &TestClock,
        )
        .await
        .unwrap();
        assert_eq!(credit.evidence().amount_units, "-12345");
        assert_eq!(credit.evidence().usd_units, "-309");
        credit.persist(&mut tx).await.unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.money_transaction_snapshots WHERE company_id='company'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(count, 2);
        tx.commit().await.unwrap();
        // A retained effect is not reusable in another transaction unit even
        // when the immutable row is unchanged.
        let mut next = pool.raw().begin().await.unwrap();
        serial(&mut next).await;
        lock_money_snapshot_sources(&mut next).await.unwrap();
        denied(
            retained.require_current(&mut next).await,
            "MONEY_SNAPSHOT_CHANGED",
        );
        denied(
            prepare_money_snapshot(
                &mut next,
                &company(),
                input("listing-1", "2026-07-15", "THB", Some(2), 12_345),
                &TestClock,
            )
            .await,
            "MONEY_SNAPSHOT_CHANGED",
        );
        next.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn scale_rate_and_company_rules_fail_closed_without_partial_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        lock_money_snapshot_sources(&mut tx).await.unwrap();
        // JPY is a zero-decimal ISO currency, so a pinned two-decimal request
        // is the retained CURRENCY_SCALE_MISMATCH.
        denied(
            prepare_money_snapshot(
                &mut tx,
                &company(),
                input("jpy-mismatch", "2026-07-15", "JPY", Some(2), 100),
                &TestClock,
            )
            .await,
            "CURRENCY_SCALE_MISMATCH",
        );
        // The scale resolves first; the missing confirmed quote is next.
        denied(
            prepare_money_snapshot(
                &mut tx,
                &company(),
                input("jpy-rate", "2026-07-15", "JPY", None, 100),
                &TestClock,
            )
            .await,
            "MANUAL_RATE_REQUIRED",
        );
        denied(
            prepare_money_snapshot(
                &mut tx,
                &company(),
                input("eur-rate", "2026-07-15", "EUR", None, 100),
                &TestClock,
            )
            .await,
            "MANUAL_RATE_REQUIRED",
        );
        // XAU has no ISO minor unit and this Company never configured one.
        denied(
            prepare_money_snapshot(
                &mut tx,
                &company(),
                input("xau-scale", "2026-07-15", "XAU", None, 100),
                &TestClock,
            )
            .await,
            "CURRENCY_SCALE_REQUIRED",
        );
        // Each Company quotes only its own rate rows for the same source key.
        let owned = prepare_money_snapshot(
            &mut tx,
            &company(),
            input("shared-source", "2026-07-15", "THB", Some(2), 12_345),
            &TestClock,
        )
        .await
        .unwrap();
        assert_eq!(owned.evidence().rate_id.as_deref(), Some("company-new"));
        assert_eq!(owned.evidence().usd_units, "309");
        owned.persist(&mut tx).await.unwrap();
        let foreign = prepare_money_snapshot(
            &mut tx,
            &other_company(),
            input("shared-source", "2026-07-15", "THB", Some(2), 12_345),
            &TestClock,
        )
        .await
        .unwrap();
        assert_eq!(foreign.evidence().rate_id.as_deref(), Some("other-rate"));
        assert_eq!(foreign.evidence().usd_units, "1235");
        foreign.persist(&mut tx).await.unwrap();
        let stored: Vec<(String, String)> = sqlx::query_as(
            "SELECT company_id,fx_rate_id FROM public.money_transaction_snapshots \
             WHERE source_id='shared-source' ORDER BY company_id",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            stored,
            [
                ("company".into(), "company-new".into()),
                ("other".into(), "other-rate".into()),
            ]
        );
        tx.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM public.money_transaction_snapshots")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0);
        // The unit must be serializable and hold the exact Finance locks.
        let mut missing_lock = pool.raw().begin().await.unwrap();
        serial(&mut missing_lock).await;
        persistence_denied(
            prepare_money_snapshot(
                &mut missing_lock,
                &company(),
                input("no-lock", "2026-07-15", "THB", Some(2), 100),
                &TestClock,
            )
            .await,
            "MONEY_SNAPSHOT_LOCKS_REQUIRED",
        );
        missing_lock.rollback().await.unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        persistence_denied(
            prepare_money_snapshot(
                &mut connection,
                &company(),
                input("no-unit", "2026-07-15", "THB", Some(2), 100),
                &TestClock,
            )
            .await,
            "FINANCE_SERIALIZABLE_REQUIRED",
        );
    })
    .await;
}

#[tokio::test]
async fn effective_dates_company_scales_fallback_and_usd_are_exact() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        lock_money_snapshot_sources(&mut tx).await.unwrap();
        // The transaction date selects the older effective quote.
        let dated = prepare_money_snapshot(
            &mut tx,
            &company(),
            input("dated", "2026-03-01", "THB", Some(2), 12_345),
            &TestClock,
        )
        .await
        .unwrap();
        assert_eq!(dated.evidence().rate_id.as_deref(), Some("company-old"));
        assert_eq!(dated.evidence().usd_units, "412");
        assert!(!dated.evidence().used_latest_fallback);
        // A date before every confirmed quote records the fallback explicitly.
        let fallback = prepare_money_snapshot(
            &mut tx,
            &company(),
            input("future", "2025-01-01", "THB", Some(2), 12_345),
            &TestClock,
        )
        .await
        .unwrap();
        assert_eq!(fallback.evidence().rate_id.as_deref(), Some("company-new"));
        assert_eq!(fallback.evidence().usd_units, "309");
        assert!(fallback.evidence().used_latest_fallback);
        // A Company-configured scale for a metal currency is preserved.
        let xag = prepare_money_snapshot(
            &mut tx,
            &company(),
            input("xag", "2026-01-15", "XAG", Some(4), 12_345),
            &TestClock,
        )
        .await
        .unwrap();
        assert_eq!(xag.evidence().currency_scale, 4);
        assert_eq!(xag.evidence().rate_id.as_deref(), Some("company-xag"));
        assert_eq!(xag.evidence().usd_units, "62");
        // USD needs no manual row or Company scale.
        let usd = prepare_money_snapshot(
            &mut tx,
            &company(),
            input("usd", "2026-01-15", "USD", None, 12_345),
            &TestClock,
        )
        .await
        .unwrap();
        assert_eq!(usd.evidence().currency_scale, 2);
        assert_eq!(usd.evidence().rate_id, None);
        assert_eq!(usd.evidence().usd_units, "12345");
        let retained = usd.persist(&mut tx).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn lock_statements_cover_the_retained_tables_in_order() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE public.company_fx_rates IN ROW EXCLUSIVE MODE")
            .execute(&mut *writer)
            .await
            .unwrap();
        let mut blocked = pool.raw().begin().await.unwrap();
        serial(&mut blocked).await;
        let conflict = lock_money_snapshot_sources(&mut blocked).await;
        assert!(matches!(
            conflict,
            Err(PgTransactionError::Persistence(ref error))
                if error.code == "PRE_SNAPSHOT_LOCK_CONTENDED"
                    && error.sql_state() == Some("55P03")
                    && error.retryable()
        ));
        blocked.rollback().await.unwrap();
        writer.rollback().await.unwrap();
    })
    .await;
}
