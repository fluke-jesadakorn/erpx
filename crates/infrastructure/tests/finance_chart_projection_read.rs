//! Focused and disposable verification for the holding-read Finance Chart projection.

use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::finance_chart::{AccountCategory, FinanceAccount, FinanceChart};
use erpx_infrastructure::{
    FinanceChartReadScope, SqlxFinanceChartProjectionStore, SqlxFinanceChartReadStore,
    SqlxFinanceChartStore,
};
use sqlx::PgPool;
use uuid::Uuid;

fn context(holding_id: Uuid, company_id: Uuid, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-read-projector-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        "finance.chart.projection.test",
    )
    .expect("context")
}

#[test]
fn migration_has_normalized_rows_metadata_indexes_and_forced_rls() {
    let sql = include_str!("../../migration/migrations_holding_read/0001_baseline.sql");
    for table in [
        "finance_chart_read_rows",
        "finance_chart_account_read_rows",
        "finance_chart_projection_checkpoints",
        "finance_chart_projection_event_receipts",
    ] {
        assert!(sql.contains(&format!("CREATE TABLE IF NOT EXISTS erpx_holding.{table}")));
        assert!(sql.contains(&format!(
            "ALTER TABLE erpx_holding.{table} FORCE ROW LEVEL SECURITY"
        )));
    }
    for column in [
        "source_contract_version",
        "source_event_version",
        "source_actor_ref",
        "source_actor_kind",
        "source_correlation_id",
        "source_causation_id",
        "source_idempotency_key",
    ] {
        assert!(sql.contains(column), "missing envelope column {column}");
    }
    assert!(sql.contains("UNIQUE (data_profile_key, chart_id, code)"));
    assert!(sql.contains("current_setting('erpx.data_profile_key', true)"));
}

#[tokio::test]
#[ignore = "requires disposable holding write and read PostgreSQL databases"]
async fn live_finance_chart_projection_is_typed_scoped_idempotent_and_rebuildable() {
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

    let data_profile_key = format!("holding-finance-chart-read-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    let create_context = context(holding_id, company_id, "chart-projection-create");
    let chart = FinanceChart::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "Read-side chart",
        vec![
            FinanceAccount::new(
                parent_id,
                "1000",
                "Cash",
                AccountCategory::Asset,
                AccountCategory::Asset.default_normal_balance(),
                None,
            )
            .expect("parent"),
            FinanceAccount::new(
                child_id,
                "1100",
                "Bank",
                AccountCategory::Asset,
                AccountCategory::Asset.default_normal_balance(),
                Some(parent_id),
            )
            .expect("child"),
        ],
        &create_context,
    )
    .expect("chart");
    let source = SqlxFinanceChartStore::new(write_pool.clone(), &data_profile_key).expect("source");
    source
        .save_chart(&chart, &create_context, None)
        .await
        .expect("source chart");

    let projection =
        SqlxFinanceChartProjectionStore::from_pools(&write_pool, &read_pool, &data_profile_key)
            .expect("projection");
    let page = projection
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].envelope.contract_version, 1);
    assert_eq!(page.events[0].envelope.event_version, 1);
    assert!(
        projection
            .project_event(&page.events[0])
            .await
            .expect("project")
            .applied
    );
    assert!(
        !projection
            .project_event(&page.events[0])
            .await
            .expect("idempotent replay")
            .applied
    );

    let read = SqlxFinanceChartReadStore::new(read_pool.clone(), &data_profile_key).expect("read");
    let scope = FinanceChartReadScope {
        holding_id,
        company_id,
    };
    let result = read.list_charts(scope, 10, None).await.expect("read chart");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].accounts.len(), 2);
    assert_eq!(
        result.rows[0].accounts[1].parent_account_id,
        Some(parent_id)
    );
    assert_eq!(result.rows[0].source.contract_version, 1);
    assert_eq!(result.rows[0].source.event_version, 1);
    assert!(
        read.list_charts(
            FinanceChartReadScope {
                holding_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
            },
            10,
            None,
        )
        .await
        .expect("cross scope read")
        .rows
        .is_empty()
    );

    let mut gap = page.events[0].clone();
    gap.envelope.event_id = Uuid::new_v4();
    gap.envelope.aggregate_version = 3;
    assert!(projection.project_event(&gap).await.is_err());

    let (preparation, events_replayed) = projection
        .rebuild_projection_with_source_fence(None, 10)
        .await
        .expect("atomic rebuild");
    assert!(preparation.rows_cleared >= 1);
    assert!(events_replayed >= 1);

    let mut tx = read_pool.begin().await.expect("inspection transaction");
    let forced: Vec<(String, bool)> = sqlx::query_as(
        "SELECT relname, relforcerowsecurity FROM pg_class WHERE oid IN (
            'erpx_holding.finance_chart_read_rows'::regclass,
            'erpx_holding.finance_chart_account_read_rows'::regclass,
            'erpx_holding.finance_chart_projection_checkpoints'::regclass,
            'erpx_holding.finance_chart_projection_event_receipts'::regclass
        ) ORDER BY relname",
    )
    .fetch_all(&mut *tx)
    .await
    .expect("forced RLS metadata");
    assert_eq!(forced.len(), 4);
    assert!(forced.iter().all(|(_, forced)| *forced));
    tx.rollback().await.expect("inspection rollback");
}
