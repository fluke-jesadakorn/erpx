//! Disposable PostgreSQL evidence for normalized Chart of Accounts source
//! persistence. The ignored test requires a dedicated holding-write database.

use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::finance_chart::{AccountCategory, AccountStatus, FinanceAccount, FinanceChart};
use erpx_infrastructure::{PersistenceError, SqlxFinanceChartStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(holding_id: Uuid, company_id: Uuid, key: &str, expected: Option<u64>) -> CommandContext {
    let mut context = CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-controller".into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        "finance.chart.persistence.test",
    )
    .expect("command context");
    context.expected_aggregate_version = expected;
    context.audit.module = Some("finance".into());
    context.subscription_id = Some(Uuid::new_v4());
    context
}

fn account(
    id: Uuid,
    code: &str,
    category: AccountCategory,
    parent_id: Option<Uuid>,
) -> FinanceAccount {
    FinanceAccount::new(
        id,
        code,
        format!("{code} account"),
        category,
        category.default_normal_balance(),
        parent_id,
    )
    .expect("account")
}

#[test]
fn migration_defines_normalized_forced_rls_and_source_guards() {
    let sql = include_str!("../../migration/migrations_holding_write/0001_baseline.sql");
    for required in [
        "finance_chart_of_accounts",
        "finance_chart_accounts",
        "finance_chart_command_idempotency",
        "FORCE ROW LEVEL SECURITY",
        "UNIQUE (data_profile_key, company_id, account_code)",
        "DEFERRABLE INITIALLY DEFERRED",
        "posted/used Finance accounts are structurally immutable",
        "Finance account hierarchy contains a cycle",
    ] {
        assert!(
            sql.contains(required),
            "missing migration contract: {required}"
        );
    }
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn live_chart_source_is_normalized_idempotent_scoped_and_usage_safe() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");

    let data_profile_key = format!("holding-finance-chart-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    let create_context = context(holding_id, company_id, "chart-create", None);
    let mut chart = FinanceChart::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "Corporate chart",
        vec![
            account(parent_id, "1000", AccountCategory::Asset, None),
            account(child_id, "1100", AccountCategory::Asset, Some(parent_id)),
        ],
        &create_context,
    )
    .expect("chart");
    let store =
        SqlxFinanceChartStore::new(pool.clone(), &data_profile_key).expect("Finance chart store");

    let first = store
        .save_chart(&chart, &create_context, None)
        .await
        .expect("first write");
    assert_eq!(first.version, 1);
    assert!(!first.replayed);
    let replay = store
        .save_chart(&chart, &create_context, None)
        .await
        .expect("exact replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, first.event_id);

    let add_context = context(holding_id, company_id, "chart-add", Some(1));
    chart
        .add_account(
            account(Uuid::new_v4(), "2000", AccountCategory::Liability, None),
            &add_context,
        )
        .expect("add account");
    store
        .save_chart(&chart, &add_context, Some(1))
        .await
        .expect("versioned chart write");
    assert_eq!(
        store.load_chart(chart.id).await.expect("load"),
        Some(chart.clone())
    );

    // A restart must never promote a partially updated normalized account set
    // as the authoritative chart. The header and every account row share the
    // aggregate version; repair the row and prove the read recovers cleanly.
    let mut stale_tx = pool.begin().await.expect("stale account transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *stale_tx)
        .await
        .expect("stale account scope");
    sqlx::query(
        "UPDATE erpx_holding.finance_chart_accounts
            SET aggregate_version=$3
          WHERE data_profile_key=$1 AND chart_id=$2 AND account_id=$4",
    )
    .bind(&data_profile_key)
    .bind(chart.id)
    .bind(1_i64)
    .bind(parent_id)
    .execute(&mut *stale_tx)
    .await
    .expect("create stale normalized account");
    stale_tx.commit().await.expect("commit stale account");
    let stale_error = store
        .load_chart(chart.id)
        .await
        .expect_err("stale normalized account must fail closed");
    assert!(
        stale_error.to_string().contains("stale_rows=1"),
        "stale chart read should expose bounded reconciliation evidence: {stale_error}"
    );
    let mut stale_verify_tx = pool.begin().await.expect("stale verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *stale_verify_tx)
        .await
        .expect("stale verification scope");
    let unchanged: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT version FROM erpx_holding.finance_chart_of_accounts
              WHERE data_profile_key=$1 AND chart_id=$2),
            (SELECT count(*) FROM erpx_holding.finance_chart_accounts
              WHERE data_profile_key=$1 AND chart_id=$2),
            (SELECT count(*) FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='finance_chart_of_accounts')",
    )
    .bind(&data_profile_key)
    .bind(chart.id)
    .fetch_one(&mut *stale_verify_tx)
    .await
    .expect("stale verification state");
    assert_eq!(
        unchanged,
        (2, 3, 2),
        "failed read must not append or mutate"
    );
    stale_verify_tx
        .rollback()
        .await
        .expect("rollback stale verification");

    let mut restore_tx = pool.begin().await.expect("restore account transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *restore_tx)
        .await
        .expect("restore account scope");
    sqlx::query(
        "UPDATE erpx_holding.finance_chart_accounts
            SET aggregate_version=$3
          WHERE data_profile_key=$1 AND chart_id=$2 AND account_id=$4",
    )
    .bind(&data_profile_key)
    .bind(chart.id)
    .bind(i64::try_from(chart.version).expect("chart version in BIGINT"))
    .bind(parent_id)
    .execute(&mut *restore_tx)
    .await
    .expect("restore normalized account");
    restore_tx.commit().await.expect("commit account repair");
    assert_eq!(
        store.load_chart(chart.id).await.expect("repaired load"),
        Some(chart.clone())
    );

    let mut tx = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_holding.finance_chart_of_accounts WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.finance_chart_accounts WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.finance_chart_command_idempotency WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='finance_chart_of_accounts')",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *tx)
    .await
    .expect("normalized counts");
    assert_eq!(counts, (1, 3, 2, 2));
    let envelope: (
        i32,
        i32,
        serde_json::Value,
        serde_json::Value,
        Uuid,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT contract_version, event_version, active_scope, actor,
                    correlation_id, idempotency_key
               FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND event_id=$2",
    )
    .bind(&data_profile_key)
    .bind(first.event_id)
    .fetch_one(&mut *tx)
    .await
    .expect("complete outbox envelope");
    assert_eq!(envelope.0, 1);
    assert_eq!(envelope.1, 1);
    assert_eq!(
        envelope.2,
        serde_json::to_value(&create_context.active_scope).expect("scope JSON")
    );
    assert_eq!(
        envelope.3,
        serde_json::to_value(&create_context.actor).expect("actor JSON")
    );
    assert_eq!(envelope.4, create_context.correlation_id);
    assert_eq!(
        envelope.5.as_deref(),
        Some(create_context.idempotency_key.as_str())
    );
    let forced: Vec<(String, bool)> = sqlx::query_as(
        "SELECT relname, relforcerowsecurity
           FROM pg_class
          WHERE oid IN (
            'erpx_holding.finance_chart_of_accounts'::regclass,
            'erpx_holding.finance_chart_accounts'::regclass,
            'erpx_holding.finance_chart_command_idempotency'::regclass
          )
          ORDER BY relname",
    )
    .fetch_all(&mut *tx)
    .await
    .expect("forced RLS metadata");
    assert_eq!(forced.len(), 3);
    assert!(forced.iter().all(|(_, enabled)| *enabled));

    sqlx::query(
        "INSERT INTO erpx_holding.finance_posted_journals
            (journal_id, holding_id, company_id, period_id, data_profile_key,
             version, idempotency_key, posted_at, payload, currency, total_minor)
         VALUES ($1,$2,$3,$4,$5,1,$6,now(),$7,'USD',100)",
    )
    .bind(Uuid::new_v4())
    .bind(holding_id)
    .bind(company_id)
    .bind(Uuid::new_v4())
    .bind(&data_profile_key)
    .bind(format!("posted-{}", Uuid::new_v4()))
    .bind(serde_json::json!({
        "entry": {
            "lines": [{
                "id": Uuid::new_v4(),
                "account_id": parent_id,
                "debit_minor": 100,
                "credit_minor": 0
            }]
        },
        "posted_at": "2026-07-25T00:00:00Z"
    }))
    .execute(&mut *tx)
    .await
    .expect("authoritative posted usage fixture");
    tx.commit().await.expect("inspection commit");

    let structural_context = context(holding_id, company_id, "chart-rename-used", Some(2));
    let mut structural_candidate = chart.clone();
    structural_candidate
        .accounts
        .iter_mut()
        .find(|account| account.id == parent_id)
        .expect("used account")
        .name = "Mutated posted account".into();
    structural_candidate.version = 3;
    structural_candidate.last_command.command_id = structural_context.command_id;
    structural_candidate.last_command.idempotency_key = structural_context.idempotency_key.clone();
    structural_candidate.last_command.actor_ref = structural_context.actor.principal_ref.clone();
    structural_candidate.last_command.correlation_id = structural_context.correlation_id;
    assert!(matches!(
        store
            .save_chart(&structural_candidate, &structural_context, Some(2))
            .await,
        Err(PersistenceError::InvalidData(message))
            if message.contains("structurally immutable")
    ));

    let lifecycle_context = context(holding_id, company_id, "chart-deactivate-used", Some(2));
    chart
        .set_account_status(parent_id, AccountStatus::Inactive, &lifecycle_context)
        .expect("used account lifecycle remains available");
    store
        .save_chart(&chart, &lifecycle_context, Some(2))
        .await
        .expect("deactivate posted account");

    let rls_role = format!("erpx_fin_chart_rls_{}", Uuid::new_v4().simple());
    let mut wrong_scope = pool.begin().await.expect("wrong scope transaction");
    sqlx::query(&format!("CREATE ROLE \"{rls_role}\" NOLOGIN"))
        .execute(&mut *wrong_scope)
        .await
        .expect("RLS role");
    sqlx::query(&format!(
        "GRANT USAGE ON SCHEMA erpx_holding TO \"{rls_role}\""
    ))
    .execute(&mut *wrong_scope)
    .await
    .expect("RLS schema grant");
    sqlx::query(&format!(
        "GRANT SELECT ON erpx_holding.finance_chart_of_accounts,
            erpx_holding.finance_chart_accounts TO \"{rls_role}\""
    ))
    .execute(&mut *wrong_scope)
    .await
    .expect("RLS table grants");
    sqlx::query(&format!("SET LOCAL ROLE \"{rls_role}\""))
        .execute(&mut *wrong_scope)
        .await
        .expect("set RLS role");
    sqlx::query("SELECT set_config('erpx.data_profile_key', 'other-holding', true)")
        .execute(&mut *wrong_scope)
        .await
        .expect("wrong holding setting");
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.finance_chart_accounts WHERE chart_id=$1",
    )
    .bind(chart.id)
    .fetch_one(&mut *wrong_scope)
    .await
    .expect("cross-holding RLS read");
    assert_eq!(visible, 0);
    wrong_scope
        .rollback()
        .await
        .expect("rollback temporary RLS role");
}
