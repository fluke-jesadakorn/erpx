use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::finance_budget::{BudgetLine, FinanceBudget};
use erpx_infrastructure::{BudgetCommitmentRequest, SqlxFinanceBudgetStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(principal: &str, holding_id: Uuid, company_id: Uuid) -> CommandContext {
    CommandContext::try_new(
        format!("budget-{principal}-{}", Uuid::new_v4()),
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
        "finance budget persistence",
    )
    .expect("context")
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_budget_persistence_enforces_version_control_and_idempotent_commitments() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant write migration");
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let account_id = Uuid::new_v4();
    let mut budget = FinanceBudget::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "THB",
        2026,
        vec![BudgetLine::new(account_id, 7, 1000).expect("budget line")],
        context("creator", holding_id, company_id),
    )
    .expect("budget");
    let store = SqlxFinanceBudgetStore::new(pool.clone(), "holding-budget-test").expect("store");
    assert_eq!(store.save_budget(&budget, None).await.expect("create"), 1);
    budget
        .submit(&context("creator", holding_id, company_id))
        .expect("submit");
    assert_eq!(
        store
            .save_budget(&budget, Some(1))
            .await
            .expect("submit save"),
        2
    );
    budget
        .approve(&context("approver", holding_id, company_id), 1000)
        .expect("approve");
    assert_eq!(
        store
            .save_budget(&budget, Some(2))
            .await
            .expect("approve save"),
        3
    );
    let request = BudgetCommitmentRequest {
        commitment_id: Uuid::new_v4(),
        budget_id: budget.id,
        account_id,
        period: 7,
        amount_minor: 1000,
        source_ref: "purchase-order:budget-test-1".into(),
        actual_minor: 0,
        idempotency_key: "budget-commitment-1".into(),
    };
    let decision = store
        .record_commitment(&request)
        .await
        .expect("record commitment");
    assert!(decision.allowed);
    assert_eq!(decision.remaining_minor, 0);
    let replay = store
        .record_commitment(&request)
        .await
        .expect("replay commitment");
    assert_eq!(replay, decision);
    let mut changed_replay = request.clone();
    changed_replay.actual_minor = 1;
    assert!(matches!(
        store.record_commitment(&changed_replay).await,
        Err(erpx_infrastructure::PersistenceError::IdempotencyConflict { .. })
    ));
    assert!(
        store
            .record_commitment(&BudgetCommitmentRequest {
                commitment_id: Uuid::new_v4(),
                budget_id: budget.id,
                account_id,
                period: 7,
                amount_minor: 1,
                source_ref: "purchase-order:budget-test-2".into(),
                actual_minor: 0,
                idempotency_key: "budget-commitment-2".into(),
            })
            .await
            .is_err(),
        "budget control must reject an over-committed amount"
    );
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type IN ('finance_budget','finance_budget_commitment')",
    )
    .bind("holding-budget-test")
    .fetch_one(&pool)
    .await
    .expect("outbox count");
    assert_eq!(outbox_count, 4);

    budget
        .lock(&context("approver", holding_id, company_id))
        .expect("lock");
    assert_eq!(
        store
            .save_budget(&budget, Some(3))
            .await
            .expect("lock save"),
        4
    );
    budget
        .close(&context("approver", holding_id, company_id))
        .expect("close");
    assert_eq!(
        store
            .save_budget(&budget, Some(4))
            .await
            .expect("close save"),
        5
    );
    assert_eq!(
        store
            .record_commitment(&request)
            .await
            .expect("closed-budget replay"),
        decision
    );

    let mut bypass = FinanceBudget::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "THB",
        2026,
        vec![BudgetLine::new(account_id, 7, 1000).expect("budget line")],
        context("creator", holding_id, company_id),
    )
    .expect("bypass candidate");
    bypass.status = erpx_domain::finance_budget::BudgetStatus::Approved;
    bypass.approved_by = Some("approver".into());
    bypass.approval_limit_minor = Some(1000);
    assert!(store.save_budget(&bypass, None).await.is_err());

    sqlx::query(
        "UPDATE erpx_tenant.finance_budgets
            SET company_id=$1
          WHERE holding_ref=$2 AND budget_id=$3",
    )
    .bind(Uuid::new_v4())
    .bind("holding-budget-test")
    .bind(budget.id)
    .execute(&pool)
    .await
    .expect("tamper persisted budget column");
    assert!(matches!(
        store.get_budget(budget.id).await,
        Err(erpx_infrastructure::PersistenceError::InvalidData(_))
    ));
    assert!(matches!(
        store.record_commitment(&request).await,
        Err(erpx_infrastructure::PersistenceError::InvalidData(_))
    ));
}
