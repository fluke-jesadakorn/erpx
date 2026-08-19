use chrono::NaiveDate;
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::finance::AccountingPeriod;
use erpx_domain::finance_close::FinanceCloseReview;
use erpx_infrastructure::{PersistenceError, SqlxFinanceCloseStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(holding_id: Uuid, company_id: Uuid, actor: &str) -> CommandContext {
    CommandContext::try_new(
        format!("finance-close-authoritative-{actor}"),
        ActorAttribution {
            principal_ref: actor.into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        "finance.period_close",
    )
    .expect("command context")
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn close_review_replay_fails_closed_when_outbox_cardinality_diverges() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");

    let data_profile_key = format!("holding-close-replay-gap-{}", Uuid::new_v4().simple());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let period_id = Uuid::new_v4();
    let period = AccountingPeriod::new(
        period_id,
        holding_id,
        company_id,
        "2026-01",
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("period start"),
        NaiveDate::from_ymd_opt(2026, 1, 31).expect("period end"),
    )
    .expect("period");
    let opener = context(holding_id, company_id, "opener");
    let review = FinanceCloseReview::open(&period, &opener).expect("open review");

    let mut seed_tx = pool.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *seed_tx)
        .await
        .expect("seed scope");
    sqlx::query(
        "INSERT INTO erpx_holding.finance_periods
            (period_id, holding_id, company_id, data_profile_key, status, version, payload)
         VALUES ($1,$2,$3,$4,'open',1,$5)",
    )
    .bind(period_id)
    .bind(holding_id)
    .bind(company_id)
    .bind(&data_profile_key)
    .bind(serde_json::to_value(&period).expect("period payload"))
    .execute(&mut *seed_tx)
    .await
    .expect("seed period");
    seed_tx.commit().await.expect("commit period");

    let store = SqlxFinanceCloseStore::new(pool.clone(), &data_profile_key).expect("close store");
    store
        .save_review(&review, &period, None)
        .await
        .expect("persist open close review");

    let mut duplicate_event_tx = pool.begin().await.expect("duplicate event transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *duplicate_event_tx)
        .await
        .expect("duplicate event scope");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,'finance_close_review',$2,$3,1,
                 'finance.close_review_saved.v2','{}',now())",
    )
    .bind(Uuid::new_v4())
    .bind(period_id)
    .bind(&data_profile_key)
    .execute(&mut *duplicate_event_tx)
    .await
    .expect("insert duplicate close review event");
    duplicate_event_tx
        .commit()
        .await
        .expect("duplicate event commit");

    assert!(matches!(
        store.save_review(&review, &period, None).await,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance close review version 1 requires exactly one authoritative outbox event, found 2"
    ));

    let mut duplicate_event_cleanup_tx = pool.begin().await.expect("duplicate cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *duplicate_event_cleanup_tx)
        .await
        .expect("duplicate cleanup scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1
            AND aggregate_type='finance_close_review'
            AND aggregate_id=$2
            AND aggregate_version=1
            AND event_type='finance.close_review_saved.v2'",
    )
    .bind(&data_profile_key)
    .bind(period_id)
    .execute(&mut *duplicate_event_cleanup_tx)
    .await
    .expect("remove duplicate close review event");
    duplicate_event_cleanup_tx
        .commit()
        .await
        .expect("duplicate cleanup commit");

    let mut tamper_tx = pool.begin().await.expect("tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tamper_tx)
        .await
        .expect("tamper scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1
            AND aggregate_type='finance_close_review'
            AND aggregate_id=$2
            AND aggregate_version=1
            AND event_type='finance.close_review_saved.v1'",
    )
    .bind(&data_profile_key)
    .bind(period_id)
    .execute(&mut *tamper_tx)
    .await
    .expect("remove close review outbox event");
    tamper_tx.commit().await.expect("tamper commit");

    assert!(matches!(
        store.save_review(&review, &period, None).await,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance close review version 1 requires exactly one authoritative outbox event, found 0"
    ));
}
