//! Native Finance period close over leased PostgreSQL, current Account/IAM
//! authority, immutable close checks/reconciliations, independent-person
//! controls and the caller-owned command receipt.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use erpx_server_finance::public::period_close::*;
use erpx_server_finance::public::sales_invoice_create::SalesInvoiceCommandActor;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey};
use facts::{account, setup};

const NOW: i64 = 1_800_000_000_000;

fn actor(user: &str, assignment: &str) -> SalesInvoiceCommandActor {
    SalesInvoiceCommandActor {
        user_id: user.into(),
        assignment_id: assignment.into(),
        role: "custom_finance_review".into(),
        session_id: format!("session-{user}"),
        revision: 7,
        assignment_version: 3,
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        scope_kind: "holding".into(),
        scope_node_id: "company".into(),
        iam_json: "{}".into(),
    }
}

fn actor_for(company: &str) -> SalesInvoiceCommandActor {
    let mut actor = actor("fixture-a", "finance-fixture-a");
    actor.company_id = company.into();
    actor
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('close-ar','company','chart','1100','Trade receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('close-ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0),\
         ('close-bank','company','chart','1000','Bank','asset','THB',1,'bank','active',1,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,status,version,imported_by,created_at)VALUES\
         ('close-statement','company','close-bank','ST-CLOSE','2026-01-31',0,0,1000000,0,'close-hash','imported',1,'fixture-a',0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn begin(
    pool: &DatabasePool,
    actor: &SalesInvoiceCommandActor,
    key: &str,
) -> Result<PeriodCloseResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.raw().begin().await.unwrap();
    let result = begin_period_close(
        &mut tx,
        actor,
        &CommandKey::new(key).unwrap(),
        "period",
        NOW,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn cancel(
    pool: &DatabasePool,
    actor: &SalesInvoiceCommandActor,
    key: &str,
    input: &PeriodCloseCancelInput,
    can_approve: bool,
) -> Result<PeriodCloseResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.raw().begin().await.unwrap();
    let result = cancel_period_close(
        &mut tx,
        actor,
        &CommandKey::new(key).unwrap(),
        "period",
        input,
        can_approve,
        NOW,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn close(
    pool: &DatabasePool,
    actor: &SalesInvoiceCommandActor,
    key: &str,
    input: &PeriodCloseProofInput,
    can_approve: bool,
) -> Result<PeriodCloseResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.raw().begin().await.unwrap();
    let result = close_period(
        &mut tx,
        actor,
        &CommandKey::new(key).unwrap(),
        "period",
        input,
        can_approve,
        NOW,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn reopen(
    pool: &DatabasePool,
    actor: &SalesInvoiceCommandActor,
    key: &str,
    input: &PeriodCloseProofInput,
    can_approve: bool,
) -> Result<PeriodCloseResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.raw().begin().await.unwrap();
    let result = reopen_period(
        &mut tx,
        actor,
        &CommandKey::new(key).unwrap(),
        "period",
        input,
        can_approve,
        NOW,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

fn proof(version: i64) -> PeriodCloseProofInput {
    PeriodCloseProofInput {
        proof: "Reviewed close workpapers".into(),
        expected_version: Some(version),
        attachment_set_ids: Vec::new(),
    }
}

fn assert_business<T: std::fmt::Debug>(
    result: Result<T, PgTransactionError<BusinessError>>,
    code: &str,
) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

#[tokio::test]
async fn period_close_begin_close_reopen_and_read_retain_evidence() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let maker = actor("fixture-a", "finance-fixture-a");
        let reviewer = actor("fixture-b", "finance-fixture-b");

        let begun = begin(pool, &maker, "close-begin-1").await.unwrap();
        assert_eq!(begun.state, "closing");
        assert_eq!(begun.version, 1);
        assert!(!begun.reconciliation_hash.is_empty());
        assert_eq!(begun.reconciliations.len(), 3);

        let workspace = {
            let mut c = pool.raw().acquire().await.unwrap();
            read_period_close(&mut c, &maker, "period").await.unwrap()
        };
        assert_eq!(workspace.period["state"], "closing");
        assert_eq!(workspace.period["close_version"], 1);
        assert_eq!(workspace.blockers, 0);
        assert!(workspace.reconciliation_ready);
        assert!(workspace.reconciliations.iter().all(|row| row.difference_minor == 0));

        let stored: (i64, String, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM finance_close_checks WHERE company_id='company' AND period_id='period'),\
             COALESCE((SELECT reconciliation_hash FROM finance_periods WHERE id='period'),''),\
             (SELECT close_version FROM finance_periods WHERE id='period')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored.0, 5);
        assert_eq!(stored.1, begun.reconciliation_hash);
        assert_eq!(stored.2, 1);

        let closed = close(pool, &reviewer, "close-approve-1", &proof(1), true)
            .await
            .unwrap();
        assert_eq!(closed.state, "closed");
        assert_eq!(closed.version, 2);
        let (state, closed_by): (String, String) = sqlx::query_as(
            "SELECT state,COALESCE(closed_by,'') FROM finance_periods WHERE id='period'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(state, "closed");
        assert_eq!(closed_by, "fixture-b");

        let reopened = reopen(pool, &maker, "close-reopen-1", &proof(2), true)
            .await
            .unwrap();
        assert_eq!(reopened.state, "open");
        assert_eq!(reopened.version, 3);
        let (state, version): (String, i64) = sqlx::query_as(
            "SELECT state,close_version FROM finance_periods WHERE id='period'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(state, "open");
        assert_eq!(version, 3);
    })
    .await;
}

#[tokio::test]
async fn period_close_rejects_unmatched_bank_and_wrong_version() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        sqlx::query(
            "INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status) \
             VALUES('close-unmatched','close-statement','company','2026-01-15','TX-1','Unmatched',100,'THB','unmatched')",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let maker = actor("fixture-a", "finance-fixture-a");
        assert_business(begin(pool, &maker, "close-begin-blocked").await, "CLOSE_NOT_READY");

        sqlx::query("DELETE FROM finance_bank_transactions WHERE id='close-unmatched'")
            .execute(pool.raw())
            .await
            .unwrap();
        let begun = begin(pool, &maker, "close-begin-2").await.unwrap();
        assert_eq!(begun.version, 1);
        assert_business(
            cancel(
                pool,
                &maker,
                "close-cancel-wrong",
                &PeriodCloseCancelInput {
                    expected_version: 9,
                    reason: "Wrong version".into(),
                },
                false,
            )
            .await,
            "VERSION_CONFLICT",
        );
        let cancelled = cancel(
            pool,
            &maker,
            "close-cancel-1",
            &PeriodCloseCancelInput {
                expected_version: 1,
                reason: "Correct the workpapers".into(),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(cancelled.state, "open");
        assert_eq!(cancelled.version, 2);
    })
    .await;
}

#[tokio::test]
async fn period_close_requires_independent_approver_and_stale_evidence() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let maker = actor("fixture-a", "finance-fixture-a");
        let reviewer = actor("fixture-b", "finance-fixture-b");
        let begun = begin(pool, &maker, "close-begin-3").await.unwrap();
        assert_eq!(begun.version, 1);
        assert_business(
            close(pool, &maker, "close-self", &proof(1), true).await,
            "CLOSE_APPROVAL_SEPARATION",
        );
        sqlx::query(
            "INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,status,version,imported_by,created_at) \
             VALUES('close-statement-2','company','close-bank','ST-CLOSE-2','2026-01-31',0,250,1000000,250,'close-hash-2','imported',1,'fixture-a',1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_business(
            close(pool, &reviewer, "close-stale", &proof(1), true).await,
            "CLOSE_RECONCILIATION_STALE",
        );
        let cancelled = cancel(
            pool,
            &maker,
            "close-cancel-2",
            &PeriodCloseCancelInput {
                expected_version: 1,
                reason: "Reconcile the changed bank evidence".into(),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(cancelled.state, "open");
    })
    .await;
}

#[tokio::test]
async fn period_close_replays_command_and_isolates_company() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let maker = actor("fixture-a", "finance-fixture-a");
        let first = begin(pool, &maker, "close-replay-1").await.unwrap();
        let replay = begin(pool, &maker, "close-replay-1").await.unwrap();
        assert_eq!(first.reconciliation_hash, replay.reconciliation_hash);
        assert_eq!(first.version, replay.version);
        let checks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM finance_close_checks WHERE company_id='company' AND period_id='period'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(checks, 5);
        assert_business(
            cancel(
                pool,
                &maker,
                "close-replay-1",
                &PeriodCloseCancelInput {
                    expected_version: 1,
                    reason: "Reuse the begin command key".into(),
                },
                false,
            )
            .await,
            "IDEMPOTENCY_CONFLICT",
        );
        let other = actor_for("company-2");
        assert_business(begin(pool, &other, "close-other-company").await, "PERIOD_NOT_FOUND");
    })
    .await;
}
