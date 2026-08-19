//! Disposable two-database acceptance for the Finance close replacement-read
//! rollback boundary.

use chrono::{NaiveDate, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::finance::{AccountingPeriod, PeriodStatus};
use erpx_domain::finance_close::FinanceCloseReview;
use erpx_infrastructure::{
    FinanceCloseReadAvailability, FinanceCloseReadFreshness, FinanceCloseReadResult,
    FinanceCloseReadScope, SqlxFinanceCloseProjectionStore, SqlxFinanceCloseReadAdapter,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

fn context(holding_id: Uuid, company_id: Uuid, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-close-projection-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        "finance.close.projection.test",
    )
    .expect("command context")
}

fn event_payload(review: &FinanceCloseReview, version: u64) -> Value {
    serde_json::json!({
        "version": version,
        "aggregate": serde_json::to_value(review).expect("review payload")
    })
}

async fn insert_source_event(
    pool: &PgPool,
    data_profile_key: &str,
    event_id: Uuid,
    period_id: Uuid,
    version: u64,
    payload: Value,
) {
    let mut tx = pool.begin().await.expect("source event transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("source holding scope");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,'finance_close_review',$2,$3,$4,
                 'finance.close_review_saved.v1',$5,now())",
    )
    .bind(event_id)
    .bind(period_id)
    .bind(data_profile_key)
    .bind(i64::try_from(version).expect("source version"))
    .bind(payload)
    .execute(&mut *tx)
    .await
    .expect("source event");
    tx.commit().await.expect("source event commit");
}

async fn projected_state(
    pool: &PgPool,
    data_profile_key: &str,
    period_id: Uuid,
) -> ((i64, Uuid, String), i64) {
    let mut tx = pool.begin().await.expect("read state transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("read holding scope");
    let row = sqlx::query_as(
        "SELECT source_version, source_event_id, status
         FROM erpx_holding.finance_close_review_read_rows
         WHERE data_profile_key=$1 AND period_id=$2",
    )
    .bind(data_profile_key)
    .bind(period_id)
    .fetch_one(&mut *tx)
    .await
    .expect("projected row");
    let checkpoint_count = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_holding.finance_close_projection_checkpoints
         WHERE data_profile_key=$1 AND period_id=$2",
    )
    .bind(data_profile_key)
    .bind(period_id)
    .fetch_one(&mut *tx)
    .await
    .expect("projected checkpoint");
    tx.commit().await.expect("read state commit");
    (row, checkpoint_count)
}

#[tokio::test]
#[ignore = "requires disposable holding write and read PostgreSQL databases"]
async fn live_finance_close_rebuild_rolls_back_after_late_source_failure() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding-write migrations");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding-read migrations");

    let data_profile_key = format!("holding-finance-close-rebuild-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let period_id = Uuid::new_v4();
    let mut period = AccountingPeriod::new(
        period_id,
        holding_id,
        company_id,
        "2026-07",
        NaiveDate::from_ymd_opt(2026, 7, 1).expect("start date"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("end date"),
    )
    .expect("accounting period");
    period
        .lock(context(holding_id, company_id, "period-lock"), Utc::now())
        .expect("lock accounting period");
    assert_eq!(period.status, PeriodStatus::Locked);
    let review = FinanceCloseReview::open(&period, &context(holding_id, company_id, "review-open"))
        .expect("open close review");
    insert_source_event(
        &write_pool,
        &data_profile_key,
        Uuid::new_v4(),
        period_id,
        1,
        event_payload(&review, 1),
    )
    .await;

    let store =
        SqlxFinanceCloseProjectionStore::from_pools(&write_pool, &read_pool, &data_profile_key)
            .expect("projection store");
    let (preparation, replayed) = store
        .rebuild_projection_with_source_fence(Some(1), 1)
        .await
        .expect("initial replacement rebuild");
    assert_eq!(preparation.rows_cleared, 0);
    assert_eq!(replayed, 1);

    let (before, before_checkpoint_count) =
        projected_state(&read_pool, &data_profile_key, period_id).await;
    assert_eq!(before_checkpoint_count, 1);

    let read_adapter = SqlxFinanceCloseReadAdapter::new(read_pool.clone(), &data_profile_key)
        .expect("close read adapter");
    let read_result = read_adapter
        .read_review(
            period_id,
            FinanceCloseReadScope::new(holding_id, company_id).expect("read scope"),
            Utc::now(),
        )
        .await
        .expect("projected close read");
    let FinanceCloseReadResult::Present(read_view) = read_result else {
        panic!("projected close review must be present");
    };
    assert_eq!(read_view.review.version, 1);
    assert_eq!(read_view.metadata.source_version, Some(1));
    assert_eq!(
        read_view.metadata.availability,
        FinanceCloseReadAvailability::Present
    );
    assert_eq!(
        read_view.metadata.freshness,
        FinanceCloseReadFreshness::Fresh
    );
    assert!(read_view.report.statement.is_none());

    let list_page = read_adapter
        .list_reviews(
            FinanceCloseReadScope::new(holding_id, company_id).expect("list scope"),
            25,
            None,
            Utc::now(),
        )
        .await
        .expect("projected close review list");
    assert_eq!(list_page.rows.len(), 1);
    assert!(!list_page.has_more);
    assert_eq!(list_page.rows[0].period_id, period_id);
    assert_eq!(list_page.rows[0].source_version, 1);
    assert_eq!(
        list_page.rows[0].freshness,
        FinanceCloseReadFreshness::Fresh
    );
    assert!(
        read_adapter
            .list_reviews(
                FinanceCloseReadScope::new(holding_id, Uuid::new_v4()).expect("sibling list scope"),
                25,
                None,
                Utc::now(),
            )
            .await
            .expect("sibling scope list")
            .rows
            .is_empty()
    );

    // The source count and fence are valid, but a late event cannot be decoded
    // as a FinanceCloseReview. It must roll back the clear and version-one
    // replay instead of leaving an empty or partial read model.
    insert_source_event(
        &write_pool,
        &data_profile_key,
        Uuid::new_v4(),
        period_id,
        2,
        serde_json::json!({"version": 2, "aggregate": {}}),
    )
    .await;
    let failed = store.rebuild_projection_with_source_fence(Some(2), 1).await;
    assert!(
        failed.is_err(),
        "malformed late source event must fail rebuild"
    );

    let (after, after_checkpoint_count) =
        projected_state(&read_pool, &data_profile_key, period_id).await;
    assert_eq!(
        after, before,
        "failed rebuild must preserve prior read state"
    );
    assert_eq!(after_checkpoint_count, 1);
}
