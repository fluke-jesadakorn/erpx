//! Disposable acceptance coverage for G8-FIN-020.
//!
//! The adapter is included by path so this focused test remains independently
//! runnable. The small scoped-transaction shim mirrors the existing private
//! infrastructure helper and does not bypass the real SQLx boundary.

use chrono::Utc;
use erpx_infrastructure::PersistenceError;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

mod finance_persistence {
    use super::*;

    pub(crate) async fn begin_scoped<'a>(
        pool: &'a PgPool,
        holding_ref: &str,
    ) -> Result<Transaction<'a, Postgres>, PersistenceError> {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
            .bind(holding_ref)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}

#[path = "../src/finance_close_control_evidence.rs"]
mod adapter;

use adapter::{FinanceCloseControlEvidenceSourceRecord, SqlxFinanceCloseControlEvidenceStore};

fn record(
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    source_aggregate_id: Uuid,
    source_version: u64,
    evidence_ref: &str,
) -> FinanceCloseControlEvidenceSourceRecord {
    FinanceCloseControlEvidenceSourceRecord::new(
        holding_id,
        company_id,
        period_id,
        "bank_reconciliation",
        "finance_bank_reconciliation",
        source_aggregate_id,
        source_version,
        evidence_ref,
        true,
        Utc::now(),
        serde_json::json!({
            "source_type": "finance_bank_reconciliation",
            "aggregate_id": source_aggregate_id,
            "source_version": source_version,
            "evidence": evidence_ref,
        }),
    )
    .expect("valid control evidence")
}

async fn outbox_events(pool: &PgPool, holding_ref: &str) -> Vec<(String, i64, Value)> {
    let mut tx = pool.begin().await.expect("outbox verification transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("outbox tenant scope");
    let rows = sqlx::query(
        "SELECT event_type, aggregate_version, payload
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type='finance_close_control_evidence'
         ORDER BY event_id",
    )
    .bind(holding_ref)
    .fetch_all(&mut *tx)
    .await
    .expect("source evidence outbox rows");
    let events = rows
        .into_iter()
        .map(|row| {
            (
                row.try_get("event_type").expect("event type"),
                row.try_get("aggregate_version").expect("event version"),
                row.try_get("payload").expect("event payload"),
            )
        })
        .collect();
    tx.commit().await.expect("commit outbox verification");
    events
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_source_evidence_is_scoped_replayed_and_closed_immutable() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant write migration");

    let holding_ref = format!("holding-close-evidence-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let period_id = Uuid::new_v4();
    let source_aggregate_id = Uuid::new_v4();
    let mut tx = pool.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("tenant scope");
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_periods
            (period_id, holding_id, company_id, holding_ref, status, version, payload)
         VALUES ($1,$2,$3,$4,'locked',1,'{}')",
    )
    .bind(period_id)
    .bind(holding_id)
    .bind(company_id)
    .bind(&holding_ref)
    .execute(&mut *tx)
    .await
    .expect("seed period");
    tx.commit().await.expect("commit period");

    let store = SqlxFinanceCloseControlEvidenceStore::new(pool.clone(), &holding_ref)
        .expect("evidence store");
    assert_eq!(store.holding_ref(), holding_ref);
    let first = record(
        holding_id,
        company_id,
        period_id,
        source_aggregate_id,
        1,
        "evidence-1",
    );
    let first_receipt = store.publish(&first).await.expect("publish evidence");
    assert!(!first_receipt.replayed);
    let events = outbox_events(&pool, &holding_ref).await;
    assert_eq!(events.len(), 1, "first source publication emits one event");
    assert_eq!(events[0].0, "finance.close_control_evidence_published.v1");
    assert_eq!(events[0].1, 1);
    assert_eq!(
        events[0].2["aggregate_type"],
        "finance_close_control_evidence"
    );
    assert_eq!(events[0].2["version"], 1);
    assert_eq!(events[0].2["aggregate"]["source"]["version"], 1);
    assert_eq!(
        events[0].2["aggregate"]["control"]["code"],
        "bank_reconciliation"
    );
    let replay_receipt = store.publish(&first).await.expect("replay evidence");
    assert!(replay_receipt.replayed);
    assert_eq!(outbox_events(&pool, &holding_ref).await.len(), 1);

    let updated = record(
        holding_id,
        company_id,
        period_id,
        source_aggregate_id,
        2,
        "evidence-2",
    );
    let update_receipt = store.publish(&updated).await.expect("update evidence");
    assert!(!update_receipt.replayed);
    let events = outbox_events(&pool, &holding_ref).await;
    assert_eq!(
        events.len(),
        2,
        "new source version emits one additional event"
    );
    assert_eq!(events.iter().filter(|event| event.1 == 2).count(), 1);
    let stale = record(
        holding_id,
        company_id,
        period_id,
        source_aggregate_id,
        1,
        "evidence-stale",
    );
    assert!(
        store.publish(&stale).await.is_err(),
        "stale source evidence must not overwrite a newer version"
    );
    let conflicting_same_version = record(
        holding_id,
        company_id,
        period_id,
        source_aggregate_id,
        2,
        "evidence-conflict",
    );
    assert!(
        store.publish(&conflicting_same_version).await.is_err(),
        "a source version must remain bound to exactly one evidence payload"
    );
    assert_eq!(outbox_events(&pool, &holding_ref).await.len(), 2);

    let changed_source = record(
        holding_id,
        company_id,
        period_id,
        Uuid::new_v4(),
        3,
        "evidence-3",
    );
    assert!(
        store.publish(&changed_source).await.is_err(),
        "one control cannot be rebound to a different source aggregate"
    );

    let bad_company = record(
        holding_id,
        Uuid::new_v4(),
        period_id,
        source_aggregate_id,
        3,
        "evidence-bad-company",
    );
    assert!(
        store.publish(&bad_company).await.is_err(),
        "company identity must match the authoritative period"
    );
    assert_eq!(outbox_events(&pool, &holding_ref).await.len(), 2);

    let mut close_tx = pool.begin().await.expect("closed review transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *close_tx)
        .await
        .expect("closed review tenant scope");
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_close_reviews
            (holding_ref, holding_id, company_id, period_id, status, version,
             idempotency_key, opened_by, payload)
         VALUES ($1,$2,$3,$4,'closed',1,$5,'close-test','{}')",
    )
    .bind(&holding_ref)
    .bind(holding_id)
    .bind(company_id)
    .bind(period_id)
    .bind(format!("close-{}", Uuid::new_v4()))
    .execute(&mut *close_tx)
    .await
    .expect("seed closed review");
    close_tx.commit().await.expect("commit closed review");
    assert!(
        store.publish(&updated).await.is_err(),
        "closed close reviews must reject evidence changes"
    );
    assert_eq!(outbox_events(&pool, &holding_ref).await.len(), 2);
}
