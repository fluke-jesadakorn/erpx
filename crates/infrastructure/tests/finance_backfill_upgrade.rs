use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const LEGACY_TENANT_MIGRATIONS: &[&str] = &[
    include_str!("../../migration/migrations_tenant_write/0001_tenant_write.sql"),
    include_str!("../../migration/migrations_tenant_write/0012_operations.sql"),
    include_str!("../../migration/migrations_tenant_write/0017_finance.sql"),
    include_str!("../../migration/migrations_tenant_write/0018_marketing.sql"),
    include_str!("../../migration/migrations_tenant_write/0019_sales.sql"),
    include_str!("../../migration/migrations_tenant_write/0020_scm.sql"),
    include_str!("../../migration/migrations_tenant_write/0021_asset_maintenance.sql"),
    include_str!("../../migration/migrations_tenant_write/0023_finance_subledgers.sql"),
    include_str!("../../migration/migrations_tenant_write/0026_marketing_delivery.sql"),
    include_str!(
        "../../migration/migrations_tenant_write/0027_marketing_provider_callback_inbox.sql"
    ),
    include_str!("../../migration/migrations_tenant_write/0028_finance_banking.sql"),
    include_str!("../../migration/migrations_tenant_write/0030_finance_bank_matching.sql"),
];

const MATCH_FACTS_MIGRATION: &str = include_str!(
    "../../migration/migrations_tenant_write/0031_finance_posted_journal_match_facts.sql"
);
const BACKFILL_MIGRATION: &str = include_str!(
    "../../migration/migrations_tenant_write/0032_finance_backfill_posted_journal_match_facts.sql"
);

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn legacy_match_fact_backfill_is_safe_for_credit_malformed_overflow_and_partial_rows() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    for migration in LEGACY_TENANT_MIGRATIONS {
        sqlx::raw_sql(migration)
            .execute(&pool)
            .await
            .expect("legacy migration");
    }
    let holding_ref = "holding-backfill-test";
    let holding_id = Uuid::from_u128(0x10000000000000000000000000000001);
    let company_id = Uuid::from_u128(0x20000000000000000000000000000001);
    let period_id = Uuid::from_u128(0x30000000000000000000000000000001);
    let rows = [
        (
            Uuid::from_u128(1),
            "legacy-valid",
            json!({"entry":{"lines":[
                {"debit_minor":600,"credit_minor":0},
                {"debit_minor":0,"credit_minor":600},
                {"debit_minor":400,"credit_minor":0},
                {"debit_minor":0,"credit_minor":400}
            ]}}),
        ),
        (
            Uuid::from_u128(2),
            "legacy-malformed",
            json!({"entry":{"lines":[{"debit_minor":"oops","credit_minor":0}]}}),
        ),
        (
            Uuid::from_u128(3),
            "legacy-overflow",
            json!({"entry":{"lines":[{"debit_minor":"99999999999999999999","credit_minor":0}]}}),
        ),
        (
            Uuid::from_u128(4),
            "legacy-partial-currency",
            json!({"entry":{"lines":[
                {"debit_minor":250,"credit_minor":0},
                {"debit_minor":0,"credit_minor":250}
            ]}}),
        ),
        (
            Uuid::from_u128(5),
            "legacy-partial-total",
            json!({"entry":{"lines":[
                {"debit_minor":700,"credit_minor":0},
                {"debit_minor":0,"credit_minor":700},
                {"debit_minor":300,"credit_minor":0},
                {"debit_minor":0,"credit_minor":300}
            ]}}),
        ),
        (
            Uuid::from_u128(6),
            "legacy-unbalanced",
            json!({"entry":{"lines":[
                {"debit_minor":100,"credit_minor":0},
                {"debit_minor":0,"credit_minor":40}
            ]}}),
        ),
    ];
    let mut tx = pool.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    sqlx::query(
        r#"DO $$
           BEGIN
               IF EXISTS (
                   SELECT 1
                   FROM pg_trigger
                   WHERE tgrelid = 'erpx_tenant.finance_posted_journals'::regclass
                     AND tgname = 'finance_posted_journals_immutable'
               ) THEN
                   EXECUTE 'ALTER TABLE erpx_tenant.finance_posted_journals
                            DISABLE TRIGGER finance_posted_journals_immutable';
               END IF;
           END
           $$;"#,
    )
    .execute(&mut *tx)
    .await
    .expect("disable an already-installed legacy trigger");
    for (journal_id, idempotency_key, payload) in rows {
        sqlx::query(
            "INSERT INTO erpx_tenant.finance_posted_journals
                (journal_id, holding_id, company_id, period_id, holding_ref, version,
                 idempotency_key, posted_at, payload)
             VALUES ($1,$2,$3,$4,$5,1,$6,now(),$7)",
        )
        .bind(journal_id)
        .bind(holding_id)
        .bind(company_id)
        .bind(period_id)
        .bind(holding_ref)
        .bind(idempotency_key)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .expect("legacy journal");
    }
    tx.commit().await.expect("seed commit");
    sqlx::raw_sql(MATCH_FACTS_MIGRATION)
        .execute(&pool)
        .await
        .expect("match facts migration");

    let mut tx = pool.begin().await.expect("partial-fact transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    sqlx::query(
        "ALTER TABLE erpx_tenant.finance_posted_journals
             DISABLE TRIGGER finance_posted_journals_immutable",
    )
    .execute(&mut *tx)
    .await
    .expect("disable test trigger");
    sqlx::query(
        "UPDATE erpx_tenant.finance_posted_journals
         SET currency='USD' WHERE journal_id=$1",
    )
    .bind(Uuid::from_u128(4))
    .execute(&mut *tx)
    .await
    .expect("partial currency");
    sqlx::query(
        "UPDATE erpx_tenant.finance_posted_journals
         SET total_minor=123 WHERE journal_id=$1",
    )
    .bind(Uuid::from_u128(5))
    .execute(&mut *tx)
    .await
    .expect("partial total");
    sqlx::query(
        "ALTER TABLE erpx_tenant.finance_posted_journals
             ENABLE TRIGGER finance_posted_journals_immutable",
    )
    .execute(&mut *tx)
    .await
    .expect("restore test trigger");
    tx.commit().await.expect("partial commit");

    sqlx::raw_sql(BACKFILL_MIGRATION)
        .execute(&pool)
        .await
        .expect("backfill migration");
    let mut tx = pool.begin().await.expect("read transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let facts: Vec<(Uuid, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT journal_id, currency, total_minor
         FROM erpx_tenant.finance_posted_journals
         WHERE journal_id IN ($1, $2, $3, $4, $5, $6)
         ORDER BY journal_id",
    )
    .bind(Uuid::from_u128(1))
    .bind(Uuid::from_u128(2))
    .bind(Uuid::from_u128(3))
    .bind(Uuid::from_u128(4))
    .bind(Uuid::from_u128(5))
    .bind(Uuid::from_u128(6))
    .fetch_all(&mut *tx)
    .await
    .expect("facts");
    assert_eq!(
        facts,
        vec![
            (Uuid::from_u128(1), None, Some(1000)),
            (Uuid::from_u128(2), None, None),
            (Uuid::from_u128(3), None, None),
            (Uuid::from_u128(4), Some("USD".into()), Some(250)),
            (Uuid::from_u128(5), None, Some(123)),
            (Uuid::from_u128(6), None, None),
        ]
    );
    let (force, trigger_enabled): (bool, String) = sqlx::query_as(
        "SELECT relforcerowsecurity,
                (SELECT tgenabled::text FROM pg_trigger
                 WHERE tgname='finance_posted_journals_immutable')
         FROM pg_class JOIN pg_namespace ON pg_namespace.oid=pg_class.relnamespace
         WHERE relname='finance_posted_journals' AND nspname='erpx_tenant'",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("security state");
    assert!(force);
    assert_eq!(trigger_enabled, "O");
    tx.commit().await.expect("read commit");
}
