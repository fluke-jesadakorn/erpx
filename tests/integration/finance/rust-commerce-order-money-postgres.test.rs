//! Finance-owned Commerce order-revision Money evidence on migrated
//! PostgreSQL: one serializable unit, 1 + 3N rows, exact scale/rate/USD,
//! fail-closed missing scale/rate and rollback with the owner unit.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_finance::public::commerce_order_money::{
    CommerceOrderMoneyInput, CommerceOrderMoneyLine, lock_commerce_order_money_sources,
    snapshot_commerce_order_revision,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{PgConnection, Row};

const NOW: i64 = 1_800_000_000_000;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
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
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1);\
         INSERT INTO company_money_settings(company_id,default_currency,updated_by,updated_at) \
         VALUES('company','THB','fixture',0) ON CONFLICT(company_id) DO UPDATE SET default_currency='THB';\
         INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) \
         VALUES('company','THB',2,'fixture',0),('company','USD',2,'fixture',0);\
         INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) \
         VALUES('rate-old','company','THB','2026-01-01',30,'fixture',10,'old'),\
         ('rate-new','company','THB','2026-06-01',40,'fixture',20,'new');",
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

fn input(order: &str, version: i64, date: &str, currency: &str) -> CommerceOrderMoneyInput {
    CommerceOrderMoneyInput {
        order_id: order.into(),
        version,
        transaction_date: date.into(),
        currency: currency.into(),
        total: 1_120,
        lines: vec![
            CommerceOrderMoneyLine {
                id: format!("{order}-line-0"),
                unit_price: 500,
                subtotal: 500,
                tax: 35,
            },
            CommerceOrderMoneyLine {
                id: format!("{order}-line-1"),
                unit_price: 500,
                subtotal: 500,
                tax: 35,
            },
        ],
    }
}

fn denied<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        _ => panic!("expected {code}"),
    }
}

#[tokio::test]
async fn order_and_line_revisions_write_exactly_one_plus_three_n_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        lock_commerce_order_money_sources(&mut tx).await.unwrap();
        let rows = snapshot_commerce_order_revision(
            &mut tx,
            &company(),
            &input("order-1", 1, "2026-07-15", "THB"),
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 7);
        let total = sqlx::query(
            "SELECT source_type,source_id,amount_kind,transaction_date::text transaction_date,\
             currency,currency_scale::int currency_scale,amount_units::text amount_units,\
             usd_units::text usd_units,fx_rate_id,used_latest_fallback,captured_at \
             FROM money_transaction_snapshots WHERE company_id='company' \
             AND source_type='commerce_order_revision' AND source_id='order-1:1' \
             AND amount_kind='total'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(total.get::<String, _>("currency"), "THB");
        assert_eq!(total.get::<i32, _>("currency_scale"), 2);
        assert_eq!(total.get::<String, _>("amount_units"), "1120");
        assert_eq!(total.get::<String, _>("usd_units"), "28");
        assert_eq!(total.get::<String, _>("fx_rate_id"), "rate-new");
        assert!(!total.get::<bool, _>("used_latest_fallback"));
        assert_eq!(total.get::<i64, _>("captured_at"), NOW);
        assert_eq!(total.get::<String, _>("transaction_date"), "2026-07-15");
        let lines: Vec<(String, String)> = sqlx::query_as(
            "SELECT source_id,amount_kind FROM money_transaction_snapshots \
             WHERE source_type='commerce_order_line_revision' ORDER BY source_id,amount_kind",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            lines,
            [
                ("order-1-line-0:1".into(), "subtotal".into()),
                ("order-1-line-0:1".into(), "tax".into()),
                ("order-1-line-0:1".into(), "unit_price".into()),
                ("order-1-line-1:1".into(), "subtotal".into()),
                ("order-1-line-1:1".into(), "tax".into()),
                ("order-1-line-1:1".into(), "unit_price".into()),
            ]
        );
        // A later revision is a new immutable source id, not an update.
        snapshot_commerce_order_revision(
            &mut tx,
            &company(),
            &input("order-1", 2, "2026-07-16", "THB"),
            TimestampMillis::new(NOW + 1),
        )
        .await
        .unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM money_transaction_snapshots \
             WHERE source_type LIKE 'commerce_order%'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(count, 14);
        // Reusing an existing Money source id is the original generic
        // command-assertion conflict and never appends a row.
        denied(
            snapshot_commerce_order_revision(
                &mut tx,
                &company(),
                &input("order-1", 2, "2026-07-16", "THB"),
                TimestampMillis::new(NOW + 2),
            )
            .await,
            "CONFLICT",
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM money_transaction_snapshots \
             WHERE source_type LIKE 'commerce_order%'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(count, 14);
        tx.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM money_transaction_snapshots")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}

#[tokio::test]
async fn scale_rate_and_currency_rules_fail_closed_without_partial_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        lock_commerce_order_money_sources(&mut tx).await.unwrap();
        denied(
            snapshot_commerce_order_revision(
                &mut tx,
                &company(),
                &input("order-rate", 1, "2026-07-15", "EUR"),
                TimestampMillis::new(NOW),
            )
            .await,
            "MANUAL_RATE_REQUIRED",
        );
        // XAG has no ISO minor unit, so a company scale is mandatory.
        denied(
            snapshot_commerce_order_revision(
                &mut tx,
                &company(),
                &input("order-xag", 1, "2026-07-15", "XAG"),
                TimestampMillis::new(NOW),
            )
            .await,
            "CURRENCY_SCALE_REQUIRED",
        );
        // A Commerce order amount must use two decimal places; JPY is 0.
        denied(
            snapshot_commerce_order_revision(
                &mut tx,
                &company(),
                &input("order-jpy", 1, "2026-07-15", "JPY"),
                TimestampMillis::new(NOW),
            )
            .await,
            "CURRENCY_SCALE_MISMATCH",
        );
        // USD is the quote currency: no rate row and no company scale needed.
        let usd = snapshot_commerce_order_revision(
            &mut tx,
            &company(),
            &input("order-usd", 1, "2026-07-15", "USD"),
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        assert_eq!(usd.len(), 7);
        assert!(
            usd.iter()
                .all(|row| row.rate_id.is_none() && row.usd_units == row.amount_units)
        );
        // A foreign Company never sees this Company's rates.
        denied(
            snapshot_commerce_order_revision(
                &mut tx,
                &CompanyId::new("other").unwrap(),
                &input("order-foreign", 1, "2026-07-15", "THB"),
                TimestampMillis::new(NOW),
            )
            .await,
            "MANUAL_RATE_REQUIRED",
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM money_transaction_snapshots WHERE source_id LIKE 'order-usd%:1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(count, 7);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn usd_and_effective_dates_select_the_exact_quote() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        lock_commerce_order_money_sources(&mut tx).await.unwrap();
        snapshot_commerce_order_revision(
            &mut tx,
            &company(),
            &input("order-usd", 1, "2026-07-15", "USD"),
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        let row = sqlx::query(
            "SELECT amount_units::text amount_units,usd_units::text usd_units,fx_rate_id \
             FROM money_transaction_snapshots WHERE source_id='order-usd:1' AND amount_kind='total'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("amount_units"), "1120");
        assert_eq!(row.get::<String, _>("usd_units"), "1120");
        assert!(row.get::<Option<String>, _>("fx_rate_id").is_none());
        // The order date picks the effective quote, never the newest one.
        snapshot_commerce_order_revision(
            &mut tx,
            &company(),
            &input("order-old", 1, "2026-03-01", "THB"),
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        let rate = sqlx::query_scalar::<_, String>(
            "SELECT fx_rate_id FROM money_transaction_snapshots \
             WHERE source_id='order-old:1' AND amount_kind='total'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(rate, "rate-old");
        // A date before every confirmed quote uses the latest confirmed quote
        // and records the fallback explicitly.
        snapshot_commerce_order_revision(
            &mut tx,
            &company(),
            &input("order-future", 1, "2025-01-01", "THB"),
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        let fallback = sqlx::query(
            "SELECT fx_rate_id,used_latest_fallback FROM money_transaction_snapshots \
             WHERE source_id='order-future:1' AND amount_kind='total'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(fallback.get::<String, _>("fx_rate_id"), "rate-new");
        assert!(fallback.get::<bool, _>("used_latest_fallback"));
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn retained_order_money_revalidates_late_effects_and_same_unit() {
    use erpx_server_finance::public::commerce_order_money::snapshot_commerce_order_revision_retained;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();serial(&mut tx).await;lock_commerce_order_money_sources(&mut tx).await.unwrap();
        let expected=input("retained-order",1,"2026-07-15","THB");
        let effect=snapshot_commerce_order_revision_retained(&mut tx,&company(),&expected,TimestampMillis::new(NOW)).await.unwrap();
        assert_eq!(effect.input(),&expected);assert_eq!(effect.evidence().len(),7);effect.require_current(&mut tx).await.unwrap();
        // Later real DB work does not invalidate the exact effect by itself.
        sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES('late-company','company','late-company','Late',NULL,'late-company',NULL,'active',1)").execute(&mut *tx).await.unwrap();effect.require_current(&mut tx).await.unwrap();
        // Rates are immutable; a legitimately inserted newer source changes
        // selection and must invalidate the retained effect.
        sqlx::query("SAVEPOINT before_rate_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES('rate-late','company','THB','2026-07-01',41,'fixture',30,'late')").execute(&mut *tx).await.unwrap();
        assert!(effect.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT before_rate_drift").execute(&mut *tx).await.unwrap();effect.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut fresh=pool.raw().begin().await.unwrap();serial(&mut fresh).await;lock_commerce_order_money_sources(&mut fresh).await.unwrap();assert!(effect.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut save=pool.raw().begin().await.unwrap();serial(&mut save).await;sqlx::query("SELECT pg_current_xact_id()").execute(&mut *save).await.unwrap();sqlx::query("SAVEPOINT before_money_locks").execute(&mut *save).await.unwrap();lock_commerce_order_money_sources(&mut save).await.unwrap();let lost=snapshot_commerce_order_revision_retained(&mut save,&company(),&input("lost-order",1,"2026-07-15","THB"),TimestampMillis::new(NOW)).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_money_locks").execute(&mut *save).await.unwrap();assert!(lost.require_current(&mut save).await.is_err());save.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn retained_order_money_rejects_quote_changed_during_its_write() {
    use erpx_server_finance::public::commerce_order_money::snapshot_commerce_order_revision_retained;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        // Trigger is test-owned and transactional; it inserts a legitimate
        // newer immutable FX rate during the actual Money row insertion.
        sqlx::raw_sql("CREATE FUNCTION test_commerce_money_rate_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO public.company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES('rate-during-write','company','THB','2026-07-01',41,'fixture',30,'during write') ON CONFLICT(id) DO NOTHING; RETURN NEW; END $$; CREATE TRIGGER test_commerce_money_rate_drift AFTER INSERT ON public.money_transaction_snapshots FOR EACH ROW EXECUTE FUNCTION test_commerce_money_rate_drift()").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();serial(&mut tx).await;lock_commerce_order_money_sources(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT before_money_write").execute(&mut *tx).await.unwrap();
        let result=snapshot_commerce_order_revision_retained(&mut tx,&company(),&input("during-rate-order",1,"2026-07-15","THB"),TimestampMillis::new(NOW)).await;
        denied(result,"COMMERCE_ORDER_MONEY_CHANGED");
        let fired:i64=sqlx::query_scalar("SELECT count(*) FROM public.company_fx_rates WHERE id='rate-during-write'").fetch_one(&mut *tx).await.unwrap();assert_eq!(fired,1,"the rate-changing trigger must have run");
        sqlx::query("ROLLBACK TO SAVEPOINT before_money_write").execute(&mut *tx).await.unwrap();
        let remaining:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM public.company_fx_rates WHERE id='rate-during-write'),(SELECT count(*) FROM public.money_transaction_snapshots WHERE source_id LIKE 'during-rate-order%')").fetch_one(&mut *tx).await.unwrap();assert_eq!(remaining,(0,0));
        let prepared=erpx_server_finance::public::commerce_order_money::prepare_commerce_order_revision_money(&mut tx,&company(),&input("prepared-rate-order",1,"2026-07-15","THB"),TimestampMillis::new(NOW)).await.unwrap();
        sqlx::query("SAVEPOINT before_prepared_write").execute(&mut *tx).await.unwrap();
        denied(prepared.apply(&mut tx).await,"COMMERCE_ORDER_MONEY_CHANGED");
        let fired:i64=sqlx::query_scalar("SELECT count(*) FROM public.company_fx_rates WHERE id='rate-during-write'").fetch_one(&mut *tx).await.unwrap();assert_eq!(fired,1);
        sqlx::query("ROLLBACK TO SAVEPOINT before_prepared_write").execute(&mut *tx).await.unwrap();
        let remaining:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM public.company_fx_rates WHERE id='rate-during-write'),(SELECT count(*) FROM public.money_transaction_snapshots WHERE source_id LIKE 'prepared-rate-order%')").fetch_one(&mut *tx).await.unwrap();assert_eq!(remaining,(0,0));
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commerce_money_preflight_preserves_quote_and_performs_no_writes() {
    use erpx_server_finance::public::commerce_order_money::prepare_commerce_order_revision_money;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();serial(&mut tx).await;lock_commerce_order_money_sources(&mut tx).await.unwrap();
        denied(prepare_commerce_order_revision_money(&mut tx,&company(),&input("preflight-missing",1,"2026-07-15","EUR"),TimestampMillis::new(NOW)).await,"MANUAL_RATE_REQUIRED");
        denied(prepare_commerce_order_revision_money(&mut tx,&company(),&input("preflight-jpy",1,"2026-07-15","JPY"),TimestampMillis::new(NOW)).await,"CURRENCY_SCALE_MISMATCH");
        let mut over=input("preflight-over",1,"2026-07-15","THB");over.lines=vec![over.lines[0].clone();201];
        denied(prepare_commerce_order_revision_money(&mut tx,&company(),&over,TimestampMillis::new(NOW)).await,"COMMERCE_ORDER_MONEY_INVALID");
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM public.money_transaction_snapshots WHERE source_id LIKE 'preflight-%'").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,0);
        let command=input("preflight-valid",1,"2026-07-15","THB");
        let prepared=prepare_commerce_order_revision_money(&mut tx,&company(),&command,TimestampMillis::new(NOW)).await.unwrap();assert_eq!(prepared.input(),&command);prepared.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT price_drift").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES('rate-preflight-late','company','THB','2026-07-01',41,'fixture',30,'later')").execute(&mut *tx).await.unwrap();assert!(prepared.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT price_drift").execute(&mut *tx).await.unwrap();prepared.require_current(&mut tx).await.unwrap();
        let effect=prepared.apply(&mut tx).await.unwrap();assert_eq!(effect.input(),&command);assert_eq!(effect.evidence().len(),7);effect.require_current(&mut tx).await.unwrap();
        let another=prepare_commerce_order_revision_money(&mut tx,&company(),&input("preflight-other",1,"2026-07-15","THB"),TimestampMillis::new(NOW)).await.unwrap();tx.rollback().await.unwrap();
        let mut fresh=pool.raw().begin().await.unwrap();serial(&mut fresh).await;lock_commerce_order_money_sources(&mut fresh).await.unwrap();assert!(another.apply(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn prepared_order_money_uses_complete_unique_key_and_defers_duplicate_to_apply() {
    use erpx_server_finance::public::commerce_order_money::prepare_commerce_order_revision_money;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;let mut tx=pool.raw().begin().await.unwrap();serial(&mut tx).await;lock_commerce_order_money_sources(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO public.money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) VALUES('company','commerce_fulfillment_line','cross-order:1','total','2026-07-15','THB',2,100,3,'rate-new',false,1)").execute(&mut *tx).await.unwrap();
        let command=input("cross-order",1,"2026-07-15","THB");
        let prepared=prepare_commerce_order_revision_money(&mut tx,&company(),&command,TimestampMillis::new(NOW)).await.unwrap();
        let effect=prepared.apply(&mut tx).await.unwrap();assert_eq!(effect.evidence().len(),7);effect.require_current(&mut tx).await.unwrap();
        let repeated=prepare_commerce_order_revision_money(&mut tx,&company(),&command,TimestampMillis::new(NOW)).await.unwrap();
        denied(repeated.apply(&mut tx).await,"CONFLICT");
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM public.money_transaction_snapshots WHERE source_id='cross-order:1'").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,2);
        tx.rollback().await.unwrap();
    }).await;
}
