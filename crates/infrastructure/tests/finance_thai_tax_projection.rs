//! Live SQLx acceptance for the explicit Thai tax source/event/projection path.
//!
//! The test is ignored in the normal suite because it requires the dedicated
//! disposable tenant write and read PostgreSQL databases.

use chrono::{NaiveDate, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::finance::{
    Account, AccountKind, AccountingPeriod, ChartOfAccounts, JournalEntry, JournalLine,
};
use erpx_domain::finance_tax::{ThaiTaxFact, ThaiTaxKind, ThaiTaxReconciliationIssueKind};
use erpx_infrastructure::{SqlxFinanceStore, SqlxFinanceThaiTaxStore};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn context(holding_id: Uuid, company_id: Uuid) -> CommandContext {
    CommandContext::try_new(
        format!("thai-tax-test-{}", Uuid::new_v4()),
        ActorAttribution {
            principal_ref: "thai-tax-projection-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "finance.thai_tax.projection.test",
    )
    .expect("context")
}

fn fact(holding_id: Uuid, company_id: Uuid) -> ThaiTaxFact {
    ThaiTaxFact {
        fact_id: Uuid::new_v4(),
        holding_id,
        company_id,
        journal_id: Uuid::new_v4(),
        line_id: Uuid::new_v4(),
        tax_date: NaiveDate::from_ymd_opt(2026, 7, 10).expect("tax date"),
        tax_period: "2026-07".into(),
        kind: ThaiTaxKind::OutputVat,
        currency: "THB".into(),
        base_minor: 100_000,
        tax_minor: 7_000,
        tax_rate_basis_points: 700,
        tax_identifier: "tax-invoice-live-001".into(),
        context: context(holding_id, company_id),
        recorded_at: Utc::now(),
    }
}

fn finance_context(holding_id: Uuid, company_id: Uuid, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "thai-tax-finance-fixture".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "finance.journal.posting.test",
    )
    .expect("finance context")
}

#[tokio::test]
#[ignore = "requires disposable tenant write and read PostgreSQL databases"]
async fn live_thai_tax_projection_is_idempotent_and_report_scoped() {
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

    let holding_ref = format!("holding-thai-tax-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let fact = fact(holding_id, company_id);
    let event_id = Uuid::new_v4();
    let mut write_tx = write_pool.begin().await.expect("write transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *write_tx)
        .await
        .expect("holding scope");
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_thai_tax_facts
            (fact_id, holding_id, company_id, journal_id, line_id, holding_ref,
             tax_date, tax_period, kind, currency, base_minor, tax_minor,
             tax_rate_basis_points, tax_identifier, context, recorded_at, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
    )
    .bind(fact.fact_id)
    .bind(fact.holding_id)
    .bind(fact.company_id)
    .bind(fact.journal_id)
    .bind(fact.line_id)
    .bind(&holding_ref)
    .bind(fact.tax_date)
    .bind(&fact.tax_period)
    .bind(fact.kind.as_str())
    .bind(&fact.currency)
    .bind(fact.base_minor)
    .bind(fact.tax_minor)
    .bind(i32::try_from(fact.tax_rate_basis_points).expect("tax rate"))
    .bind(&fact.tax_identifier)
    .bind(serde_json::to_value(&fact.context).expect("fact context json"))
    .bind(fact.recorded_at)
    .bind(serde_json::to_value(&fact).expect("fact json"))
    .execute(&mut *write_tx)
    .await
    .expect("authoritative source fact");
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,'finance_thai_tax_fact',$2,$3,1,
                 'finance.thai_tax_fact_recorded.v1',$4,$5)",
    )
    .bind(event_id)
    .bind(fact.fact_id)
    .bind(&holding_ref)
    .bind(json!({"fact": serde_json::to_value(&fact).expect("fact json")}))
    .bind(fact.recorded_at)
    .execute(&mut *write_tx)
    .await
    .expect("source event");
    write_tx.commit().await.expect("source commit");

    let store = SqlxFinanceThaiTaxStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("tax store");
    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(page.events.len(), 1);
    let event = &page.events[0];
    let first = store
        .project_event(
            event.event_id,
            &event.event_type,
            event.aggregate_id,
            event.aggregate_version,
            &event.payload,
            event.occurred_at,
        )
        .await
        .expect("first projection");
    assert!(first.applied);
    let replay = store
        .project_event(
            event.event_id,
            &event.event_type,
            event.aggregate_id,
            event.aggregate_version,
            &event.payload,
            event.occurred_at,
        )
        .await
        .expect("idempotent replay");
    assert!(!replay.applied);
    let reconciled = store
        .reconcile_projection()
        .await
        .expect("source/read reconciliation");
    assert_eq!(reconciled.source_events, 1);
    assert_eq!(reconciled.projected_rows, 1);
    assert_eq!(reconciled.mismatched_rows, 0);
    assert!(reconciled.is_reconciled());
    let report = store
        .list_report(holding_id, company_id, "2026-07", "THB", 10)
        .await
        .expect("report");
    assert_eq!(report.report.fact_count, 1);
    assert_eq!(report.report.output_vat_minor, 7_000);

    let mut payload_drift_tx = read_pool.begin().await.expect("payload drift transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *payload_drift_tx)
        .await
        .expect("payload drift scope");
    sqlx::query(
        "UPDATE erpx_tenant.finance_thai_tax_fact_read_rows
            SET tax_minor=tax_minor+1
          WHERE holding_ref=$1 AND fact_id=$2",
    )
    .bind(&holding_ref)
    .bind(fact.fact_id)
    .execute(&mut *payload_drift_tx)
    .await
    .expect("tamper projected tax amount");
    payload_drift_tx
        .commit()
        .await
        .expect("payload drift commit");
    let payload_drifted = store.reconcile_projection().await;
    assert!(matches!(
        payload_drifted,
        Err(erpx_infrastructure::PersistenceError::InvalidData(message))
            if message == "tax amount 7001 does not equal the calculated amount 7000"
    ));

    let mut restore_payload_tx = read_pool
        .begin()
        .await
        .expect("restore payload transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *restore_payload_tx)
        .await
        .expect("restore payload scope");
    sqlx::query(
        "UPDATE erpx_tenant.finance_thai_tax_fact_read_rows
            SET tax_minor=tax_minor-1
          WHERE holding_ref=$1 AND fact_id=$2",
    )
    .bind(&holding_ref)
    .bind(fact.fact_id)
    .execute(&mut *restore_payload_tx)
    .await
    .expect("restore projected tax amount");
    restore_payload_tx
        .commit()
        .await
        .expect("restore payload commit");

    let mut drift_tx = read_pool.begin().await.expect("drift transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *drift_tx)
        .await
        .expect("read holding scope");
    sqlx::query(
        "DELETE FROM erpx_tenant.finance_thai_tax_fact_read_rows
          WHERE holding_ref=$1 AND fact_id=$2",
    )
    .bind(&holding_ref)
    .bind(fact.fact_id)
    .execute(&mut *drift_tx)
    .await
    .expect("remove projected row for drift proof");
    drift_tx.commit().await.expect("drift commit");

    let drifted = store
        .reconcile_projection()
        .await
        .expect("drift reconciliation");
    assert_eq!(drifted.source_events, 1);
    assert_eq!(drifted.projected_rows, 0);
    assert_eq!(drifted.mismatched_rows, 1);
    assert!(!drifted.is_reconciled());
}

#[tokio::test]
#[ignore = "requires disposable tenant write and read PostgreSQL databases"]
async fn live_thai_tax_fact_is_bound_to_authoritative_journal_line_and_currency() {
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

    let holding_ref = format!("holding-thai-tax-authority-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let debit_account_id = Uuid::new_v4();
    let credit_account_id = Uuid::new_v4();
    let chart_id = Uuid::new_v4();
    let period = AccountingPeriod::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "2026-07",
        NaiveDate::from_ymd_opt(2026, 7, 1).expect("period start"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("period end"),
    )
    .expect("period");
    let chart = ChartOfAccounts::new(
        chart_id,
        holding_id,
        company_id,
        "Thai tax authority fixture chart",
        "THB",
        vec![
            Account {
                id: debit_account_id,
                code: "1000".into(),
                name: "Receivable".into(),
                kind: AccountKind::Asset,
                parent_id: None,
                active: true,
            },
            Account {
                id: credit_account_id,
                code: "4000".into(),
                name: "Revenue".into(),
                kind: AccountKind::Revenue,
                parent_id: None,
                active: true,
            },
        ],
    )
    .expect("chart");
    let finance_store = SqlxFinanceStore::new(write_pool.clone(), read_pool.clone(), &holding_ref)
        .expect("finance store");
    finance_store
        .save_period(&period, None)
        .await
        .expect("period source");
    finance_store
        .save_chart(&chart, None)
        .await
        .expect("chart source");

    let taxable_line = JournalLine::new(
        Uuid::new_v4(),
        debit_account_id,
        100_000,
        0,
        Some("taxable receivable".into()),
    )
    .expect("taxable line");
    let balancing_line = JournalLine::new(
        Uuid::new_v4(),
        credit_account_id,
        0,
        100_000,
        Some("taxable revenue".into()),
    )
    .expect("balancing line");
    let journal = JournalEntry::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        period.id,
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("journal date"),
        "Thai statutory tax authority fixture",
        vec![taxable_line.clone(), balancing_line],
        finance_context(
            holding_id,
            company_id,
            &format!("thai-tax-journal-{}", Uuid::new_v4()),
        ),
    )
    .expect("journal");
    let posted = finance_store
        .post_journal(journal, &chart, &period, Utc::now())
        .await
        .expect("posted journal")
        .journal;
    let tax_store = SqlxFinanceThaiTaxStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("tax store");
    let mut tax_fact = ThaiTaxFact {
        fact_id: Uuid::new_v4(),
        holding_id,
        company_id,
        journal_id: posted.id(),
        line_id: taxable_line.id,
        tax_date: posted.entry_date(),
        tax_period: "2026-07".into(),
        kind: ThaiTaxKind::OutputVat,
        currency: "THB".into(),
        base_minor: 100_000,
        tax_minor: 7_000,
        tax_rate_basis_points: 700,
        tax_identifier: "tax-invoice-authoritative-001".into(),
        context: context(holding_id, company_id),
        recorded_at: Utc::now(),
    };
    let first = tax_store.record_fact(&tax_fact).await.expect("tax fact");
    assert!(!first.replayed);
    let replay = tax_store.record_fact(&tax_fact).await.expect("tax replay");
    assert!(replay.replayed);
    let authoritative_fact = tax_fact.clone();

    tax_fact.fact_id = Uuid::new_v4();
    tax_fact.context = context(holding_id, company_id);
    tax_fact.base_minor = 100_001;
    tax_fact.tax_minor = 7_000;
    assert!(matches!(
        tax_store.record_fact(&tax_fact).await,
        Err(erpx_infrastructure::PersistenceError::InvalidData(message))
            if message == "Thai tax base cannot exceed its immutable posted journal line"
    ));

    tax_fact.fact_id = Uuid::new_v4();
    tax_fact.context = context(holding_id, company_id);
    tax_fact.base_minor = 100_000;
    tax_fact.tax_minor = 7_000;
    tax_fact.currency = "USD".into();
    assert!(matches!(
        tax_store.record_fact(&tax_fact).await,
        Err(erpx_infrastructure::PersistenceError::InvalidData(message))
            if message == "Thai tax fact currency does not match the immutable posted journal"
    ));

    let mut inspection = write_pool.begin().await.expect("tax inspection");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *inspection)
        .await
        .expect("tax inspection scope");
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.finance_thai_tax_facts
             WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type='finance_thai_tax_fact')",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *inspection)
    .await
    .expect("tax counts");
    inspection.commit().await.expect("tax inspection commit");
    assert_eq!(counts, (1, 1));

    let page = tax_store
        .read_source_event_page(None, 10)
        .await
        .expect("tax source page");
    for event in &page.events {
        tax_store
            .project_event(
                event.event_id,
                &event.event_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
                event.occurred_at,
            )
            .await
            .expect("project authoritative tax event");
    }
    let statutory = tax_store
        .reconcile_statutory_report(holding_id, company_id, "2026-07", "THB")
        .await
        .expect("statutory report reconciliation");
    assert!(statutory.is_reconciled());
    assert_eq!(statutory.source_fact_count, 1);
    assert_eq!(statutory.projected_fact_count, 1);
    assert_eq!(statutory.source_totals, statutory.projected_totals);
    let all_statutory = tax_store
        .reconcile_statutory_reports()
        .await
        .expect("all statutory report reconciliation");
    assert!(all_statutory.is_reconciled());
    assert_eq!(all_statutory.reports.len(), 1);

    let mut event_drift_tx = write_pool.begin().await.expect("event drift transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *event_drift_tx)
        .await
        .expect("event drift scope");
    sqlx::query(
        "UPDATE erpx_tenant.outbox_events
            SET event_type='finance.thai_tax_fact_tampered.v1'
          WHERE holding_ref=$1 AND aggregate_id=$2
            AND aggregate_type='finance_thai_tax_fact'",
    )
    .bind(&holding_ref)
    .bind(authoritative_fact.fact_id)
    .execute(&mut *event_drift_tx)
    .await
    .expect("tamper tax source event type");
    event_drift_tx.commit().await.expect("event drift commit");
    assert!(matches!(
        tax_store
            .reconcile_statutory_report(holding_id, company_id, "2026-07", "THB")
            .await,
        Err(erpx_infrastructure::PersistenceError::InvalidData(message))
            if message == "Thai tax source event type is invalid"
    ));

    let mut restore_event_tx = write_pool.begin().await.expect("restore event transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *restore_event_tx)
        .await
        .expect("restore event scope");
    sqlx::query(
        "UPDATE erpx_tenant.outbox_events
            SET event_type='finance.thai_tax_fact_recorded.v1'
          WHERE holding_ref=$1 AND aggregate_id=$2
            AND aggregate_type='finance_thai_tax_fact'",
    )
    .bind(&holding_ref)
    .bind(authoritative_fact.fact_id)
    .execute(&mut *restore_event_tx)
    .await
    .expect("restore tax source event type");
    restore_event_tx
        .commit()
        .await
        .expect("restore event commit");

    let mut drift_tx = read_pool
        .begin()
        .await
        .expect("statutory drift transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *drift_tx)
        .await
        .expect("statutory drift scope");
    sqlx::query(
        "UPDATE erpx_tenant.finance_thai_tax_fact_read_rows
            SET tax_identifier='tampered-statutory-identifier'
          WHERE holding_ref=$1 AND fact_id=$2",
    )
    .bind(&holding_ref)
    .bind(authoritative_fact.fact_id)
    .execute(&mut *drift_tx)
    .await
    .expect("tamper statutory read row");
    drift_tx.commit().await.expect("statutory drift commit");
    let drifted = tax_store
        .reconcile_statutory_report(holding_id, company_id, "2026-07", "THB")
        .await
        .expect("statutory drift report");
    assert!(!drifted.is_reconciled());
    assert_eq!(
        drifted.issues[0].kind,
        ThaiTaxReconciliationIssueKind::FactMismatch
    );

    tax_store
        .clear_projection()
        .await
        .expect("clear read projection for rebuild");
    let missing = tax_store
        .reconcile_statutory_report(holding_id, company_id, "2026-07", "THB")
        .await
        .expect("missing projection report");
    assert!(!missing.is_reconciled());
    assert_eq!(
        missing.issues[0].kind,
        ThaiTaxReconciliationIssueKind::MissingProjectedFact
    );
    let (_, events_replayed) = tax_store
        .rebuild_projection_with_source_fence(None, 10)
        .await
        .expect("atomic rebuild read projection");
    assert!(events_replayed >= 1);
    assert!(
        tax_store
            .reconcile_statutory_report(holding_id, company_id, "2026-07", "THB")
            .await
            .expect("reconciled rebuilt report")
            .is_reconciled()
    );
}
