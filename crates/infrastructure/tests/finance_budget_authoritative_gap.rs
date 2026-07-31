use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::finance_budget::{BudgetLine, FinanceBudget};
use erpx_infrastructure::{BudgetCommitmentRequest, PersistenceError, SqlxFinanceBudgetStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(principal: &str, holding_id: Uuid, company_id: Uuid, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: principal.into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "finance budget authoritative gap",
    )
    .expect("context")
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn commitment_replay_fails_closed_when_its_outbox_event_is_missing() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant-write migrations");

    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let account_id = Uuid::new_v4();
    let holding_ref = format!("holding-budget-gap-{}", Uuid::new_v4().simple());
    let mut budget = FinanceBudget::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "THB",
        2026,
        vec![BudgetLine::new(account_id, 7, 1000).expect("budget line")],
        context("creator", holding_id, company_id, "budget-gap-create"),
    )
    .expect("budget");
    let store = SqlxFinanceBudgetStore::new(pool.clone(), &holding_ref).expect("store");
    store
        .save_budget(&budget, None)
        .await
        .expect("create budget");
    budget
        .submit(&context(
            "creator",
            holding_id,
            company_id,
            "budget-gap-submit",
        ))
        .expect("submit budget");
    store
        .save_budget(&budget, Some(1))
        .await
        .expect("save submitted budget");
    budget
        .approve(
            &context("approver", holding_id, company_id, "budget-gap-approve"),
            1000,
        )
        .expect("approve budget");
    store
        .save_budget(&budget, Some(2))
        .await
        .expect("save approved budget");

    let request = BudgetCommitmentRequest {
        commitment_id: Uuid::new_v4(),
        budget_id: budget.id,
        account_id,
        period: 7,
        amount_minor: 100,
        source_ref: "purchase-order:budget-gap-1".into(),
        actual_minor: 0,
        idempotency_key: "budget-gap-commitment-1".into(),
    };
    store
        .record_commitment(&request)
        .await
        .expect("record commitment");

    let mut tamper_tx = pool.begin().await.expect("tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper_tx)
        .await
        .expect("tamper scope");
    sqlx::query(
        "DELETE FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1
            AND aggregate_type='finance_budget_commitment'
            AND aggregate_id=$2",
    )
    .bind(&holding_ref)
    .bind(request.commitment_id)
    .execute(&mut *tamper_tx)
    .await
    .expect("remove commitment outbox event");
    tamper_tx.commit().await.expect("tamper commit");

    assert!(matches!(
        store.record_commitment(&request).await,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance budget commitment requires exactly one authoritative outbox event, found 0"
    ));
}
