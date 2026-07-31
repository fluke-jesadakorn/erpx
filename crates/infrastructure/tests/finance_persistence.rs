//! Integration evidence for journal posting against the normalized write-side
//! Finance Chart of Accounts source.

use chrono::{NaiveDate, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::finance::{
    Account, AccountKind, AccountingPeriod, ChartOfAccounts, JournalEntry, JournalLine,
};
use erpx_domain::finance_chart::{AccountCategory, AccountStatus, FinanceAccount, FinanceChart};
use erpx_infrastructure::{PersistenceError, SqlxFinanceChartStore, SqlxFinanceStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(holding_id: Uuid, company_id: Uuid, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-posting-test".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "finance.journal.posting.test",
    )
    .expect("command context")
}

fn legacy_account(id: Uuid, code: &str, kind: AccountKind) -> Account {
    Account {
        id,
        code: code.into(),
        name: format!("{code} account"),
        kind,
        parent_id: None,
        active: true,
    }
}

fn legacy_chart(
    chart_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    accounts: Vec<Account>,
) -> ChartOfAccounts {
    ChartOfAccounts::new(
        chart_id,
        holding_id,
        company_id,
        "Caller chart is compatibility input",
        "THB",
        accounts,
    )
    .expect("legacy chart")
}

fn normalized_account(id: Uuid, code: &str, category: AccountCategory) -> FinanceAccount {
    FinanceAccount::new(
        id,
        code,
        format!("{code} account"),
        category,
        category.default_normal_balance(),
        None,
    )
    .expect("normalized account")
}

fn normalized_chart(
    chart_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    accounts: Vec<FinanceAccount>,
    command_context: &CommandContext,
) -> FinanceChart {
    FinanceChart::new(
        chart_id,
        holding_id,
        company_id,
        "Authoritative normalized chart",
        accounts,
        command_context,
    )
    .expect("normalized chart")
}

fn make_period(holding_id: Uuid, company_id: Uuid) -> AccountingPeriod {
    AccountingPeriod::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "2026-07",
        NaiveDate::from_ymd_opt(2026, 7, 1).expect("period start"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("period end"),
    )
    .expect("period")
}

fn journal(
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    debit_account_id: Uuid,
    credit_account_id: Uuid,
    key: &str,
) -> JournalEntry {
    journal_on_date(
        holding_id,
        company_id,
        period_id,
        debit_account_id,
        credit_account_id,
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("journal date"),
        key,
    )
}

fn journal_on_date(
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    debit_account_id: Uuid,
    credit_account_id: Uuid,
    entry_date: NaiveDate,
    key: &str,
) -> JournalEntry {
    JournalEntry::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        period_id,
        entry_date,
        "Authoritative chart posting",
        vec![
            JournalLine::new(Uuid::new_v4(), debit_account_id, 1_000, 0, None).expect("debit line"),
            JournalLine::new(Uuid::new_v4(), credit_account_id, 0, 1_000, None)
                .expect("credit line"),
        ],
        context(holding_id, company_id, key),
    )
    .expect("journal")
}

async fn counts(pool: &PgPool, holding_ref: &str) -> (i64, i64, i64, i64) {
    let mut tx = pool.begin().await.expect("counts transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let counts = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.finance_posted_journals
              WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.finance_posted_journal_account_usage
              WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type='finance_journal'),
            (SELECT count(*) FROM erpx_tenant.finance_command_idempotency
              WHERE holding_ref=$1)",
    )
    .bind(holding_ref)
    .fetch_one(&mut *tx)
    .await
    .expect("posting counts");
    tx.commit().await.expect("counts commit");
    counts
}

async fn save_normalized_chart(
    pool: &PgPool,
    holding_ref: &str,
    chart: &FinanceChart,
    command_context: &CommandContext,
) {
    SqlxFinanceChartStore::new(pool.clone(), holding_ref)
        .expect("chart store")
        .save_chart(chart, command_context, None)
        .await
        .expect("normalized chart source");
}

async fn save_period(pool: &PgPool, holding_ref: &str, value: &AccountingPeriod) {
    SqlxFinanceStore::new(pool.clone(), pool.clone(), holding_ref)
        .expect("finance store")
        .save_period(value, None)
        .await
        .expect("period source");
}

async fn install_rejecting_usage_trigger(pool: &PgPool) -> (String, String) {
    let suffix = Uuid::new_v4().simple().to_string();
    let function_name = format!("finance_test_reject_usage_{suffix}");
    let trigger_name = format!("finance_test_reject_usage_trigger_{suffix}");
    sqlx::query(&format!(
        "CREATE FUNCTION erpx_tenant.\"{function_name}\"() RETURNS trigger
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test usage rejection'; END; $$"
    ))
    .execute(pool)
    .await
    .expect("rejecting test function");
    sqlx::query(&format!(
        "CREATE TRIGGER \"{trigger_name}\"
         BEFORE INSERT ON erpx_tenant.finance_posted_journal_account_usage
         FOR EACH ROW EXECUTE FUNCTION erpx_tenant.\"{function_name}\"()"
    ))
    .execute(pool)
    .await
    .expect("rejecting test trigger");
    (function_name, trigger_name)
}

async fn remove_rejecting_usage_trigger(pool: &PgPool, function_name: &str, trigger_name: &str) {
    sqlx::query(&format!(
        "DROP TRIGGER \"{trigger_name}\"
         ON erpx_tenant.finance_posted_journal_account_usage"
    ))
    .execute(pool)
    .await
    .expect("drop rejecting test trigger");
    sqlx::query(&format!("DROP FUNCTION erpx_tenant.\"{function_name}\"()"))
        .execute(pool)
        .await
        .expect("drop rejecting test function");
}

#[test]
fn migration_declares_immutable_usage_facts_and_scope() {
    let sql = include_str!(
        "../../migration/migrations_tenant_write/0043_finance_journal_chart_usage.sql"
    );
    for required in [
        "finance_posted_journal_account_usage",
        "FOREIGN KEY (holding_ref, chart_id, account_id)",
        "FORCE ROW LEVEL SECURITY",
        "Finance journal account usage facts are immutable",
        "posted/used Finance accounts are structurally immutable",
    ] {
        assert!(
            sql.contains(required),
            "missing migration contract: {required}"
        );
    }
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_journal_posting_is_authoritative_scoped_idempotent_and_atomic() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant-write migrations");

    let holding_ref = format!("holding-finance-journal-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let debit_id = Uuid::new_v4();
    let credit_id = Uuid::new_v4();
    let chart_id = Uuid::new_v4();
    let period = make_period(holding_id, company_id);
    save_period(&pool, &holding_ref, &period).await;
    let chart_create_context = context(holding_id, company_id, "chart-create");
    let normalized = normalized_chart(
        chart_id,
        holding_id,
        company_id,
        vec![
            normalized_account(debit_id, "1000", AccountCategory::Asset),
            normalized_account(credit_id, "4000", AccountCategory::Revenue),
        ],
        &chart_create_context,
    );
    save_normalized_chart(&pool, &holding_ref, &normalized, &chart_create_context).await;
    let legacy = legacy_chart(
        chart_id,
        holding_id,
        company_id,
        vec![
            legacy_account(debit_id, "1000", AccountKind::Asset),
            legacy_account(credit_id, "4000", AccountKind::Revenue),
        ],
    );
    let store = SqlxFinanceStore::new(pool.clone(), pool.clone(), &holding_ref).expect("store");

    let mut forged_period = period.clone();
    forged_period.starts_on = NaiveDate::from_ymd_opt(2026, 1, 1).expect("forged period start");
    forged_period.ends_on = NaiveDate::from_ymd_opt(2026, 12, 31).expect("forged period end");
    let mut wrong_identity_period = period.clone();
    wrong_identity_period.company_id = Uuid::new_v4();
    assert!(matches!(
        store.save_period(&wrong_identity_period, Some(1)).await,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance accounting period identity is immutable"
    ));
    let before_forged_post = counts(&pool, &holding_ref).await;
    let forged_result = store
        .post_journal(
            journal_on_date(
                holding_id,
                company_id,
                period.id,
                debit_id,
                credit_id,
                NaiveDate::from_ymd_opt(2026, 8, 10).expect("outside authoritative period"),
                "journal-forged-period",
            ),
            &legacy,
            &forged_period,
            Utc::now(),
        )
        .await;
    assert!(matches!(
        forged_result,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance accounting period does not match authoritative tenant write state"
    ));
    assert_eq!(
        counts(&pool, &holding_ref).await,
        before_forged_post,
        "a period representation mismatch must not emit a journal, usage fact, event, or idempotency row"
    );

    let entry = journal(
        holding_id,
        company_id,
        period.id,
        debit_id,
        credit_id,
        "journal-valid",
    );
    let posted_at = Utc::now();
    let first = store
        .post_journal(entry.clone(), &legacy, &period, posted_at)
        .await
        .expect("authoritative post");
    assert!(!first.replayed);
    assert_eq!(counts(&pool, &holding_ref).await, (1, 2, 1, 1));
    let replay = store
        .post_journal(entry.clone(), &legacy, &period, posted_at)
        .await
        .expect("idempotent replay");
    assert!(replay.replayed);
    assert_eq!(replay.journal, first.journal);
    assert_eq!(counts(&pool, &holding_ref).await, (1, 2, 1, 1));

    let mut usage_guard_tx = pool.begin().await.expect("usage guard transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *usage_guard_tx)
        .await
        .expect("usage guard scope");
    let structural_update = sqlx::query(
        "UPDATE erpx_tenant.finance_chart_accounts
            SET account_name='Used account must remain immutable'
          WHERE holding_ref=$1 AND chart_id=$2 AND account_id=$3",
    )
    .bind(&holding_ref)
    .bind(chart_id)
    .bind(debit_id)
    .execute(&mut *usage_guard_tx)
    .await;
    assert!(
        structural_update.is_err(),
        "posted account structural changes must be rejected"
    );
    usage_guard_tx
        .rollback()
        .await
        .expect("usage guard rollback");

    let missing_chart = legacy_chart(
        Uuid::new_v4(),
        holding_id,
        company_id,
        legacy.accounts.clone(),
    );
    let before_missing_chart = counts(&pool, &holding_ref).await;
    let missing_chart_result = store
        .post_journal(
            journal(
                holding_id,
                company_id,
                period.id,
                debit_id,
                credit_id,
                "journal-missing-chart",
            ),
            &missing_chart,
            &period,
            Utc::now(),
        )
        .await;
    assert!(matches!(
        missing_chart_result,
        Err(PersistenceError::NotFound {
            entity: "authoritative Finance chart",
            ..
        })
    ));
    assert_eq!(counts(&pool, &holding_ref).await, before_missing_chart);

    let inactive_company_id = Uuid::new_v4();
    let inactive_period = make_period(holding_id, inactive_company_id);
    save_period(&pool, &holding_ref, &inactive_period).await;
    let inactive_chart_id = Uuid::new_v4();
    let mut inactive_account =
        normalized_account(Uuid::new_v4(), "2100", AccountCategory::Liability);
    inactive_account.status = AccountStatus::Inactive;
    let inactive_credit_id = Uuid::new_v4();
    let inactive_credit = normalized_account(inactive_credit_id, "4100", AccountCategory::Revenue);
    let inactive_context = context(holding_id, inactive_company_id, "chart-inactive");
    let inactive_chart = normalized_chart(
        inactive_chart_id,
        holding_id,
        inactive_company_id,
        vec![inactive_account.clone(), inactive_credit],
        &inactive_context,
    );
    save_normalized_chart(&pool, &holding_ref, &inactive_chart, &inactive_context).await;
    let inactive_legacy = legacy_chart(
        inactive_chart_id,
        holding_id,
        inactive_company_id,
        vec![
            legacy_account(inactive_account.id, "2100", AccountKind::Liability),
            legacy_account(inactive_credit_id, "4100", AccountKind::Revenue),
        ],
    );
    let before_inactive = counts(&pool, &holding_ref).await;
    let inactive_result = store
        .post_journal(
            journal(
                holding_id,
                inactive_company_id,
                inactive_period.id,
                inactive_account.id,
                inactive_credit_id,
                "journal-inactive-account",
            ),
            &inactive_legacy,
            &inactive_period,
            Utc::now(),
        )
        .await;
    assert!(matches!(
        inactive_result,
        Err(PersistenceError::InvalidData(message))
            if message == "authoritative Finance account is inactive"
    ));
    assert_eq!(counts(&pool, &holding_ref).await, before_inactive);

    let mismatched_chart_id = Uuid::new_v4();
    let other_company_id = Uuid::new_v4();
    let mismatch_debit_id = Uuid::new_v4();
    let mismatch_credit_id = Uuid::new_v4();
    let mismatch_context = context(holding_id, other_company_id, "chart-scope-mismatch");
    let mismatched_source = normalized_chart(
        mismatched_chart_id,
        holding_id,
        other_company_id,
        vec![
            normalized_account(mismatch_debit_id, "1200", AccountCategory::Asset),
            normalized_account(mismatch_credit_id, "4200", AccountCategory::Revenue),
        ],
        &mismatch_context,
    );
    save_normalized_chart(&pool, &holding_ref, &mismatched_source, &mismatch_context).await;
    let mismatched_legacy = legacy_chart(
        mismatched_chart_id,
        holding_id,
        other_company_id,
        vec![
            legacy_account(mismatch_debit_id, "1200", AccountKind::Asset),
            legacy_account(mismatch_credit_id, "4200", AccountKind::Revenue),
        ],
    );
    let before_scope = counts(&pool, &holding_ref).await;
    let scope_result = store
        .post_journal(
            journal(
                holding_id,
                company_id,
                period.id,
                mismatch_debit_id,
                mismatch_credit_id,
                "journal-scope-mismatch",
            ),
            &mismatched_legacy,
            &period,
            Utc::now(),
        )
        .await;
    assert!(matches!(
        scope_result,
        Err(PersistenceError::InvalidData(message))
            if message == "authoritative Finance chart scope does not match the journal"
    ));
    assert_eq!(counts(&pool, &holding_ref).await, before_scope);

    let missing_account_id = Uuid::new_v4();
    let before_missing_account = counts(&pool, &holding_ref).await;
    let missing_account_result = store
        .post_journal(
            journal(
                holding_id,
                company_id,
                period.id,
                missing_account_id,
                credit_id,
                "journal-missing-account",
            ),
            &legacy_chart(
                chart_id,
                holding_id,
                company_id,
                vec![
                    legacy_account(missing_account_id, "9999", AccountKind::Asset),
                    legacy_account(credit_id, "4000", AccountKind::Revenue),
                ],
            ),
            &period,
            Utc::now(),
        )
        .await;
    assert!(matches!(
        missing_account_result,
        Err(PersistenceError::NotFound {
            entity: "authoritative Finance account",
            id
        }) if id == missing_account_id
    ));
    assert_eq!(counts(&pool, &holding_ref).await, before_missing_account);

    let (function_name, trigger_name) = install_rejecting_usage_trigger(&pool).await;
    let before_rollback = counts(&pool, &holding_ref).await;
    let rollback_result = store
        .post_journal(
            journal(
                holding_id,
                company_id,
                period.id,
                debit_id,
                credit_id,
                "journal-atomic-rollback",
            ),
            &legacy,
            &period,
            Utc::now(),
        )
        .await;
    assert!(
        rollback_result.is_err(),
        "usage rejection must fail posting"
    );
    assert_eq!(counts(&pool, &holding_ref).await, before_rollback);
    remove_rejecting_usage_trigger(&pool, &function_name, &trigger_name).await;
}

#[tokio::test]
#[ignore = "requires disposable tenant PostgreSQL write and read databases"]
async fn live_finance_journal_projection_replay_integrity_is_fail_closed() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_tenant_write(&write_pool)
        .await
        .expect("tenant-write migrations");
    erpx_migration::migrate_tenant_read(&read_pool)
        .await
        .expect("tenant-read migrations");

    let holding_ref = format!("holding-finance-projection-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let debit_id = Uuid::new_v4();
    let credit_id = Uuid::new_v4();
    let chart_id = Uuid::new_v4();
    let period = make_period(holding_id, company_id);
    save_period(&write_pool, &holding_ref, &period).await;
    let chart_context = context(holding_id, company_id, "projection-chart");
    let normalized = normalized_chart(
        chart_id,
        holding_id,
        company_id,
        vec![
            normalized_account(debit_id, "1000", AccountCategory::Asset),
            normalized_account(credit_id, "4000", AccountCategory::Revenue),
        ],
        &chart_context,
    );
    save_normalized_chart(&write_pool, &holding_ref, &normalized, &chart_context).await;
    let legacy = legacy_chart(
        chart_id,
        holding_id,
        company_id,
        vec![
            legacy_account(debit_id, "1000", AccountKind::Asset),
            legacy_account(credit_id, "4000", AccountKind::Revenue),
        ],
    );
    let write_store = SqlxFinanceStore::new(write_pool.clone(), read_pool.clone(), &holding_ref)
        .expect("finance store");
    write_store
        .post_journal(
            journal(
                holding_id,
                company_id,
                period.id,
                debit_id,
                credit_id,
                "projection-replay-integrity",
            ),
            &legacy,
            &period,
            Utc::now(),
        )
        .await
        .expect("authoritative journal post");

    let event = write_store
        .read_source_event_page(None, 10)
        .await
        .expect("journal source event page")
        .events
        .into_iter()
        .next()
        .expect("journal source event");
    let projected = write_store
        .project_event(
            event.event_id,
            event.aggregate_id,
            event.aggregate_version,
            &event.payload,
        )
        .await
        .expect("initial projection");
    assert!(projected.applied);

    let reconciled = write_store
        .reconcile_projection()
        .await
        .expect("source/read reconciliation");
    assert_eq!(reconciled.source_events, 1);
    assert_eq!(reconciled.projected_rows, 1);
    assert_eq!(reconciled.mismatched_rows, 0);
    assert!(reconciled.is_reconciled());

    let replay = write_store
        .project_event(
            event.event_id,
            event.aggregate_id,
            event.aggregate_version,
            &event.payload,
        )
        .await
        .expect("exact event replay");
    assert!(!replay.applied, "exact replay must be idempotent");

    let conflicting_event = write_store
        .project_event(
            Uuid::new_v4(),
            event.aggregate_id,
            event.aggregate_version,
            &event.payload,
        )
        .await;
    assert!(matches!(
        conflicting_event,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance projection event is not present in the authoritative outbox"
    ));

    let mut conflicting_payload = event.payload.clone();
    conflicting_payload["posted_journal"]["entry"]["description"] =
        serde_json::Value::String("tampered projection payload".into());
    let conflicting_payload = write_store
        .project_event(
            event.event_id,
            event.aggregate_id,
            event.aggregate_version,
            &conflicting_payload,
        )
        .await;
    assert!(matches!(
        conflicting_payload,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance projection event does not match its authoritative outbox row"
    ));

    let mut inspection = read_pool.begin().await.expect("projection inspection");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *inspection)
        .await
        .expect("projection inspection scope");
    let persisted: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.finance_posted_journals
             WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.finance_projection_checkpoints
             WHERE holding_ref=$1)",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *inspection)
    .await
    .expect("projection remains intact");
    assert_eq!(persisted, (1, 1));
    inspection
        .commit()
        .await
        .expect("projection inspection commit");

    let mut drift_tx = read_pool
        .begin()
        .await
        .expect("projection drift transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *drift_tx)
        .await
        .expect("projection drift scope");
    sqlx::query(
        "UPDATE erpx_tenant.finance_posted_journals
            SET payload='{}'::jsonb
          WHERE holding_ref=$1 AND journal_id=$2",
    )
    .bind(&holding_ref)
    .bind(event.aggregate_id)
    .execute(&mut *drift_tx)
    .await
    .expect("tamper disposable projection");
    drift_tx.commit().await.expect("projection drift commit");

    let drifted = write_store
        .reconcile_projection()
        .await
        .expect("reconcile tampered projection");
    assert_eq!(drifted.mismatched_rows, 1);
    assert!(!drifted.is_reconciled());

    let (preparation, replayed) = write_store
        .rebuild_projection_with_source_fence(Some(1), 1)
        .await
        .expect("fenced replacement rebuild");
    assert_eq!(preparation.rows_cleared, 1);
    assert_eq!(preparation.checkpoints_cleared, 1);
    assert_eq!(replayed, 1);
    let repaired = write_store
        .reconcile_projection()
        .await
        .expect("reconcile repaired projection");
    assert!(repaired.is_reconciled());

    let mut malformed_source = write_pool
        .begin()
        .await
        .expect("malformed source transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *malformed_source)
        .await
        .expect("malformed source scope");
    sqlx::query(
        r#"INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,'finance_journal',$2,$3,1,
                 'finance.journal_posted.v1','{"posted_journal":{}}'::jsonb,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::from_u128(u128::MAX))
    .bind(&holding_ref)
    .execute(&mut *malformed_source)
    .await
    .expect("malformed late source event");
    malformed_source
        .commit()
        .await
        .expect("malformed source commit");
    let failed = write_store
        .rebuild_projection_with_source_fence(Some(2), 1)
        .await;
    assert!(failed.is_err(), "malformed source must fail rebuild");

    let mut rollback_inspection = read_pool.begin().await.expect("rollback inspection");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *rollback_inspection)
        .await
        .expect("rollback inspection scope");
    let after_failure: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.finance_posted_journals
             WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.finance_projection_checkpoints
             WHERE holding_ref=$1)",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *rollback_inspection)
    .await
    .expect("rollback projection counts");
    rollback_inspection
        .commit()
        .await
        .expect("rollback inspection commit");
    assert_eq!(after_failure, (1, 1));
}
