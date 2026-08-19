//! Disposable schema acceptance for G8-FIN-019.
//!
//! This test verifies the additive migration, forced RLS, authoritative
//! close-write path, and closed-review/evidence database immutability defense.

use chrono::{NaiveDate, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::finance::{
    Account, AccountKind, AccountingPeriod, ChartOfAccounts, JournalEntry, JournalLine,
    PeriodStatus,
};
use erpx_domain::finance_close::{
    CloseControlFlag, CloseInput, CloseJournalFact, CloseJournalLine, FinanceCloseReview,
};
use erpx_infrastructure::{
    FinanceCloseControlEvidenceSourceRecord, SqlxFinanceCloseControlEvidenceStore,
};
use sqlx::PgPool;
use uuid::Uuid;

fn command_context(holding_id: Uuid, company_id: Uuid, actor: &str) -> CommandContext {
    CommandContext::try_new(
        format!("finance-close-{actor}"),
        ActorAttribution {
            principal_ref: actor.to_owned(),
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
async fn live_finance_close_review_schema_is_scoped_and_closed_rows_are_immutable() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-close-schema-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let period_id = Uuid::new_v4();
    let closed_target_period_id = Uuid::new_v4();
    let mut tx = pool.begin().await.expect("transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    sqlx::query(
        "INSERT INTO erpx_holding.finance_close_control_evidence
            (data_profile_key, holding_id, company_id, period_id, control_code,
             source_type, source_aggregate_id, evidence_ref, resolved,
             observed_at, payload)
         VALUES ($1,$2,$3,$4,'bank_reconciliation','test_source',$5,$6,true,now(),'{}')",
    )
    .bind(&data_profile_key)
    .bind(holding_id)
    .bind(company_id)
    .bind(period_id)
    .bind(Uuid::new_v4())
    .bind(format!("evidence-{}", Uuid::new_v4()))
    .execute(&mut *tx)
    .await
    .expect("seed control evidence");
    sqlx::query(
        "INSERT INTO erpx_holding.finance_close_reviews
            (data_profile_key, holding_id, company_id, period_id, status, version,
             idempotency_key, opened_by, payload)
         VALUES ($1,$2,$3,$4,'closed',1,$5,'closer',$6)",
    )
    .bind(&data_profile_key)
    .bind(holding_id)
    .bind(company_id)
    .bind(period_id)
    .bind(format!("schema-{}", Uuid::new_v4()))
    .bind(serde_json::json!({"status": "closed"}))
    .execute(&mut *tx)
    .await
    .expect("seed closed review");
    sqlx::query(
        "INSERT INTO erpx_holding.finance_close_reviews
            (data_profile_key, holding_id, company_id, period_id, status, version,
             idempotency_key, opened_by, payload)
         VALUES ($1,$2,$3,$4,'closed',1,$5,'closer',$6)",
    )
    .bind(&data_profile_key)
    .bind(holding_id)
    .bind(company_id)
    .bind(closed_target_period_id)
    .bind(format!("schema-target-{}", Uuid::new_v4()))
    .bind(serde_json::json!({"status": "closed"}))
    .execute(&mut *tx)
    .await
    .expect("seed closed target review");
    let update = sqlx::query(
        "UPDATE erpx_holding.finance_close_reviews
         SET payload='{}'::jsonb WHERE data_profile_key=$1 AND period_id=$2",
    )
    .bind(&data_profile_key)
    .bind(period_id)
    .execute(&mut *tx)
    .await;
    assert!(update.is_err(), "closed review input must be immutable");
    let evidence_update = sqlx::query(
        "UPDATE erpx_holding.finance_close_control_evidence
         SET resolved=false WHERE data_profile_key=$1 AND period_id=$2",
    )
    .bind(&data_profile_key)
    .bind(period_id)
    .execute(&mut *tx)
    .await;
    assert!(
        evidence_update.is_err(),
        "close control evidence must be immutable after close"
    );
    let moved_evidence = sqlx::query(
        "UPDATE erpx_holding.finance_close_control_evidence
         SET period_id=$3 WHERE data_profile_key=$1 AND period_id=$2",
    )
    .bind(&data_profile_key)
    .bind(period_id)
    .bind(closed_target_period_id)
    .execute(&mut *tx)
    .await;
    assert!(
        moved_evidence.is_err(),
        "evidence cannot be moved into a closed period"
    );
    tx.rollback().await.expect("rollback disposable seed");

    let forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname='erpx_holding' AND c.relname='finance_close_reviews'",
    )
    .fetch_one(&pool)
    .await
    .expect("RLS metadata");
    assert!(forced);
    let evidence_forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname='erpx_holding' AND c.relname='finance_close_control_evidence'",
    )
    .fetch_one(&pool)
    .await
    .expect("control evidence RLS metadata");
    assert!(evidence_forced);
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn live_finance_close_persistence_requires_authoritative_facts_and_controls() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-close-authority-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let period_id = Uuid::new_v4();
    let debit_account = Uuid::new_v4();
    let credit_account = Uuid::new_v4();
    let fact_id = Uuid::new_v4();
    let evidence_source_aggregate_id = Uuid::new_v4();
    let opener = command_context(holding_id, company_id, "opener");
    let reviewer = command_context(holding_id, company_id, "reviewer");
    let closer = command_context(holding_id, company_id, "closer");
    let mut period = AccountingPeriod::new(
        period_id,
        holding_id,
        company_id,
        "2026-01",
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("start date"),
        NaiveDate::from_ymd_opt(2026, 1, 31).expect("end date"),
    )
    .expect("period");

    let chart = ChartOfAccounts::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "Close test chart",
        "USD",
        vec![
            Account {
                id: debit_account,
                code: "1000".into(),
                name: "Cash".into(),
                kind: AccountKind::Asset,
                parent_id: None,
                active: true,
            },
            Account {
                id: credit_account,
                code: "4000".into(),
                name: "Revenue".into(),
                kind: AccountKind::Revenue,
                parent_id: None,
                active: true,
            },
        ],
    )
    .expect("chart");
    let journal = JournalEntry::new(
        fact_id,
        holding_id,
        company_id,
        period_id,
        NaiveDate::from_ymd_opt(2026, 1, 15).expect("entry date"),
        "authoritative close test",
        vec![
            JournalLine::new(Uuid::new_v4(), debit_account, 100, 0, None).expect("debit line"),
            JournalLine::new(Uuid::new_v4(), credit_account, 0, 100, None).expect("credit line"),
        ],
        command_context(holding_id, company_id, "poster"),
    )
    .expect("journal");
    let posted = journal
        .post(&chart, &period, Utc::now())
        .expect("posted journal");
    period
        .lock(opener.clone(), Utc::now())
        .expect("lock period");
    assert_eq!(period.status, PeriodStatus::Locked);
    let posted_payload = serde_json::to_value(&posted).expect("posted payload");

    let mut tx = pool.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let period_payload = serde_json::to_value(&period).expect("period payload");
    sqlx::query(
        "INSERT INTO erpx_holding.finance_periods
            (period_id, holding_id, company_id, data_profile_key, status, version, payload)
         VALUES ($1,$2,$3,$4,'locked',1,$5)",
    )
    .bind(period_id)
    .bind(holding_id)
    .bind(company_id)
    .bind(&data_profile_key)
    .bind(&period_payload)
    .execute(&mut *tx)
    .await
    .expect("seed authoritative period");
    sqlx::query(
        "INSERT INTO erpx_holding.finance_posted_journals
            (journal_id, holding_id, company_id, period_id, data_profile_key, version,
             idempotency_key, posted_at, currency, total_minor, payload)
         VALUES ($1,$2,$3,$4,$5,1,$6,$7,'USD',100,$8)",
    )
    .bind(fact_id)
    .bind(holding_id)
    .bind(company_id)
    .bind(period_id)
    .bind(&data_profile_key)
    .bind(posted.idempotency_key())
    .bind(posted.posted_at())
    .bind(&posted_payload)
    .execute(&mut *tx)
    .await
    .expect("seed posted journal");
    tx.commit().await.expect("commit seed");

    let evidence_store = SqlxFinanceCloseControlEvidenceStore::new(pool.clone(), &data_profile_key)
        .expect("control evidence store");
    evidence_store
        .publish(
            &FinanceCloseControlEvidenceSourceRecord::new(
                holding_id,
                company_id,
                period_id,
                "bank_reconciliation",
                "test_source",
                evidence_source_aggregate_id,
                1,
                format!("evidence-{}", Uuid::new_v4()),
                true,
                Utc::now(),
                serde_json::json!({}),
            )
            .expect("authoritative control evidence"),
        )
        .await
        .expect("publish authoritative control");

    let store = erpx_infrastructure::SqlxFinanceCloseStore::new(pool.clone(), &data_profile_key)
        .expect("close store");
    let mut fabricated_period = period.clone();
    fabricated_period.label = "fabricated-period".into();
    let fabricated_review =
        FinanceCloseReview::open(&fabricated_period, &opener).expect("fabricated review");
    assert!(
        store
            .save_review(&fabricated_review, &fabricated_period, None)
            .await
            .is_err(),
        "persistence must reload the authoritative accounting period"
    );
    let mut review = FinanceCloseReview::open(&period, &opener).expect("open review");
    let open_receipt = store
        .save_review(&review, &period, None)
        .await
        .expect("persist open review");
    assert_eq!(open_receipt.version, 1);

    let mut incomplete_review =
        FinanceCloseReview::open(&period, &opener).expect("open incomplete review candidate");
    incomplete_review
        .submit_for_review(
            &period,
            &command_context(holding_id, company_id, "incomplete-reviewer"),
            CloseInput::new(
                Vec::new(),
                vec![CloseControlFlag::new("bank_reconciliation", true).expect("close control")],
            )
            .expect("incomplete close input"),
        )
        .expect("domain accepts incomplete candidate before persistence");
    let incomplete_error = store
        .save_review(&incomplete_review, &period, Some(1))
        .await
        .expect_err("persistence must reject an omitted authoritative journal");
    assert!(
        incomplete_error.to_string().contains("unsubmitted=1"),
        "reconciliation error must retain bounded source evidence: {incomplete_error}"
    );

    let fact = CloseJournalFact::new(
        fact_id,
        holding_id,
        company_id,
        period_id,
        NaiveDate::from_ymd_opt(2026, 1, 15).expect("fact date"),
        vec![
            CloseJournalLine::new(debit_account, 100, 0).expect("close debit"),
            CloseJournalLine::new(credit_account, 0, 100).expect("close credit"),
        ],
    )
    .expect("close fact");
    review
        .submit_for_review(
            &period,
            &reviewer,
            CloseInput::new(
                vec![fact],
                vec![CloseControlFlag::new("bank_reconciliation", true).expect("close control")],
            )
            .expect("close input"),
        )
        .expect("submit review");

    // A row can satisfy the close flag comparison while still carrying a
    // forged source envelope. The close write must reconcile the row to its
    // source-owned outbox event before changing the review snapshot or
    // appending its outbox event.
    let mut tamper_tx = pool.begin().await.expect("tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tamper_tx)
        .await
        .expect("tamper holding scope");
    sqlx::query(
        "UPDATE erpx_holding.finance_close_control_evidence
         SET source_aggregate_id=$5
         WHERE data_profile_key=$1 AND holding_id=$2 AND company_id=$3 AND period_id=$4
           AND control_code='bank_reconciliation'",
    )
    .bind(&data_profile_key)
    .bind(holding_id)
    .bind(company_id)
    .bind(period_id)
    .bind(Uuid::new_v4())
    .execute(&mut *tamper_tx)
    .await
    .expect("tamper source envelope");
    tamper_tx.commit().await.expect("commit source tamper");

    let tampered_error = store
        .save_review(&review, &period, Some(1))
        .await
        .expect_err("close persistence must reject forged evidence metadata");
    assert!(
        tampered_error
            .to_string()
            .contains("source event does not reconcile"),
        "tampered evidence must fail closed: {tampered_error}"
    );

    let mut verify_tx = pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify_tx)
        .await
        .expect("verification holding scope");
    let (stored_version, review_outbox_count): (i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT version FROM erpx_holding.finance_close_reviews
              WHERE data_profile_key=$1 AND period_id=$2),
             (SELECT COUNT(*) FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='finance_close_review')",
    )
    .bind(&data_profile_key)
    .bind(period_id)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("verify rollback state");
    assert_eq!(stored_version, 1, "review mutation must roll back");
    assert_eq!(review_outbox_count, 1, "review outbox must not be appended");
    verify_tx.rollback().await.expect("rollback verification");

    let mut restore_tx = pool.begin().await.expect("restore transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *restore_tx)
        .await
        .expect("restore holding scope");
    sqlx::query(
        "UPDATE erpx_holding.finance_close_control_evidence
         SET source_aggregate_id=$5
         WHERE data_profile_key=$1 AND holding_id=$2 AND company_id=$3 AND period_id=$4
           AND control_code='bank_reconciliation'",
    )
    .bind(&data_profile_key)
    .bind(holding_id)
    .bind(company_id)
    .bind(period_id)
    .bind(evidence_source_aggregate_id)
    .execute(&mut *restore_tx)
    .await
    .expect("restore source envelope");
    restore_tx.commit().await.expect("commit source restore");

    let review_receipt = store
        .save_review(&review, &period, Some(1))
        .await
        .expect("persist reviewed input");
    assert_eq!(review_receipt.version, 2);

    review.close(&period, &closer).expect("close review");
    let close_receipt = store
        .save_review(&review, &period, Some(2))
        .await
        .expect("persist closed review");
    assert_eq!(close_receipt.version, 3);

    let synthetic_period = AccountingPeriod::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "2026-02",
        NaiveDate::from_ymd_opt(2026, 2, 1).expect("start date"),
        NaiveDate::from_ymd_opt(2026, 2, 28).expect("end date"),
    )
    .expect("synthetic period");
    let mut synthetic_seed = pool.begin().await.expect("synthetic period transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *synthetic_seed)
        .await
        .expect("synthetic holding scope");
    let synthetic_period_payload =
        serde_json::to_value(&synthetic_period).expect("synthetic period payload");
    sqlx::query(
        "INSERT INTO erpx_holding.finance_periods
            (period_id, holding_id, company_id, data_profile_key, status, version, payload)
         VALUES ($1,$2,$3,$4,'open',1,$5)",
    )
    .bind(synthetic_period.id)
    .bind(holding_id)
    .bind(company_id)
    .bind(&data_profile_key)
    .bind(&synthetic_period_payload)
    .execute(&mut *synthetic_seed)
    .await
    .expect("seed synthetic period");
    synthetic_seed
        .commit()
        .await
        .expect("commit synthetic period");
    let synthetic_review =
        FinanceCloseReview::open(&synthetic_period, &opener).expect("open synthetic review");
    let synthetic_receipt = store
        .save_review(&synthetic_review, &synthetic_period, None)
        .await
        .expect("persist synthetic open review");
    assert_eq!(synthetic_receipt.version, 1);
    let mut synthetic_review = synthetic_review;
    synthetic_review
        .submit_for_review(
            &synthetic_period,
            &reviewer,
            CloseInput::new(
                vec![
                    CloseJournalFact::new(
                        Uuid::new_v4(),
                        holding_id,
                        company_id,
                        synthetic_period.id,
                        NaiveDate::from_ymd_opt(2026, 2, 15).expect("synthetic fact date"),
                        vec![
                            CloseJournalLine::new(debit_account, 100, 0).expect("synthetic debit"),
                            CloseJournalLine::new(credit_account, 0, 100)
                                .expect("synthetic credit"),
                        ],
                    )
                    .expect("synthetic fact"),
                ],
                vec![],
            )
            .expect("synthetic input"),
        )
        .expect("domain accepts candidate facts before persistence");
    assert!(
        store
            .save_review(&synthetic_review, &synthetic_period, Some(1))
            .await
            .is_err(),
        "persistence must reject synthetic close facts"
    );
}
