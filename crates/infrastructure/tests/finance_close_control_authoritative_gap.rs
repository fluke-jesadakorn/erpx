use chrono::Utc;
use erpx_infrastructure::{
    FinanceCloseControlEvidenceSourceRecord, PersistenceError, SqlxFinanceCloseControlEvidenceStore,
};
use sqlx::PgPool;
use uuid::Uuid;

fn record(
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    source_aggregate_id: Uuid,
) -> FinanceCloseControlEvidenceSourceRecord {
    FinanceCloseControlEvidenceSourceRecord::new(
        holding_id,
        company_id,
        period_id,
        "bank_reconciliation",
        "finance_bank_reconciliation",
        source_aggregate_id,
        1,
        "close-control-evidence-1",
        true,
        Utc::now(),
        serde_json::json!({
            "source_type": "finance_bank_reconciliation",
            "aggregate_id": source_aggregate_id,
            "source_version": 1,
        }),
    )
    .expect("valid close-control evidence")
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn close_control_replay_fails_closed_when_its_outbox_event_is_missing() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");

    let data_profile_key = format!("holding-close-control-gap-{}", Uuid::new_v4().simple());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let period_id = Uuid::new_v4();
    let source_aggregate_id = Uuid::new_v4();
    let mut seed_tx = pool.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *seed_tx)
        .await
        .expect("seed scope");
    sqlx::query(
        "INSERT INTO erpx_holding.finance_periods
            (period_id, holding_id, company_id, data_profile_key, status, version, payload)
         VALUES ($1,$2,$3,$4,'locked',1,'{}')",
    )
    .bind(period_id)
    .bind(holding_id)
    .bind(company_id)
    .bind(&data_profile_key)
    .execute(&mut *seed_tx)
    .await
    .expect("seed period");
    seed_tx.commit().await.expect("commit period");

    let store = SqlxFinanceCloseControlEvidenceStore::new(pool.clone(), &data_profile_key)
        .expect("close-control evidence store");
    let evidence = record(holding_id, company_id, period_id, source_aggregate_id);
    store.publish(&evidence).await.expect("publish evidence");

    let mut tamper_tx = pool.begin().await.expect("tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tamper_tx)
        .await
        .expect("tamper scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1
            AND aggregate_type='finance_close_control_evidence'
            AND aggregate_version=1
            AND event_type='finance.close_control_evidence_published.v1'",
    )
    .bind(&data_profile_key)
    .execute(&mut *tamper_tx)
    .await
    .expect("remove close-control outbox event");
    tamper_tx.commit().await.expect("tamper commit");

    assert!(matches!(
        store.publish(&evidence).await,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance close control source version 1 requires exactly one authoritative outbox event, found 0"
    ));
}
