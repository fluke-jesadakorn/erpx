//! Routed Finance consolidation acceptance over immutable journal projections.

use chrono::{NaiveDate, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::finance::{
    Account, AccountKind, AccountingPeriod, ChartOfAccounts, ConsolidationEliminationEntry,
    ConsolidationEliminationLine, ConsolidationFxRate, ConsolidationFxRateSet,
    ConsolidationOwnership, ConsolidationOwnershipInterest, JournalEntry, JournalLine,
};
use erpx_infrastructure::SqlxFinanceStore;
use sqlx::PgPool;
use uuid::Uuid;

fn context(holding_id: Uuid, company_id: Uuid, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-consolidation-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        "finance.consolidation.test",
    )
    .expect("context")
}

fn chart(holding_id: Uuid, company_id: Uuid, chart_id: Uuid) -> ChartOfAccounts {
    let cash = Uuid::from_u128(101);
    let sales = Uuid::from_u128(102);
    ChartOfAccounts::new(
        chart_id,
        holding_id,
        company_id,
        "Consolidated chart",
        "THB",
        vec![
            Account {
                id: cash,
                code: "1000".into(),
                name: "Cash".into(),
                kind: AccountKind::Asset,
                parent_id: None,
                active: true,
            },
            Account {
                id: sales,
                code: "4000".into(),
                name: "Sales".into(),
                kind: AccountKind::Revenue,
                parent_id: None,
                active: true,
            },
        ],
    )
    .expect("chart")
}

fn period(holding_id: Uuid, company_id: Uuid) -> AccountingPeriod {
    AccountingPeriod::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "2026-07",
        NaiveDate::from_ymd_opt(2026, 7, 1).expect("start"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("end"),
    )
    .expect("period")
}

fn journal(
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    amount: i64,
    key: &str,
) -> JournalEntry {
    JournalEntry::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        period_id,
        NaiveDate::from_ymd_opt(2026, 7, 15).expect("date"),
        "Consolidation acceptance journal",
        vec![
            JournalLine::new(Uuid::new_v4(), Uuid::from_u128(101), amount, 0, None).expect("debit"),
            JournalLine::new(Uuid::new_v4(), Uuid::from_u128(102), 0, amount, None)
                .expect("credit"),
        ],
        context(holding_id, company_id, key),
    )
    .expect("journal")
}

#[tokio::test]
#[ignore = "requires disposable holding PostgreSQL write and read databases"]
async fn live_consolidated_trial_balance_replays_and_aggregates_two_companies() {
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

    let data_profile_key = format!("holding-finance-consolidation-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let first_company = Uuid::new_v4();
    let second_company = Uuid::new_v4();
    let first_period = period(holding_id, first_company);
    let second_period = period(holding_id, second_company);
    let first_chart = chart(holding_id, first_company, Uuid::new_v4());
    let second_chart = chart(holding_id, second_company, Uuid::new_v4());
    let write_store =
        SqlxFinanceStore::new(write_pool.clone(), read_pool.clone(), &data_profile_key)
            .expect("write store");
    write_store
        .save_period(&first_period, None)
        .await
        .expect("first period");
    write_store
        .save_period(&second_period, None)
        .await
        .expect("second period");
    write_store
        .save_chart(&first_chart, None)
        .await
        .expect("first chart");
    write_store
        .save_chart(&second_chart, None)
        .await
        .expect("second chart");
    write_store
        .post_journal(
            journal(
                holding_id,
                first_company,
                first_period.id,
                100,
                "consolidation-company-one",
            ),
            &first_chart,
            &first_period,
            Utc::now(),
        )
        .await
        .expect("first journal");
    write_store
        .post_journal(
            journal(
                holding_id,
                second_company,
                second_period.id,
                50,
                "consolidation-company-two",
            ),
            &second_chart,
            &second_period,
            Utc::now(),
        )
        .await
        .expect("second journal");

    let mut cursor = None;
    loop {
        let page = write_store
            .read_source_event_page(cursor.as_ref(), 10)
            .await
            .expect("source page");
        for event in &page.events {
            write_store
                .project_event(
                    event.event_id,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )
                .await
                .expect("journal projection");
        }
        if !page.has_more {
            break;
        }
        cursor = page.next_cursor;
    }

    let read_store = SqlxFinanceStore::new(write_pool.clone(), read_pool, &data_profile_key)
        .expect("read store");
    let (report, source_version) = read_store
        .consolidated_trial_balance(&[first_chart, second_chart])
        .await
        .expect("consolidated report");
    assert_eq!(report.company_ids.len(), 2);
    assert_eq!(report.total_debit_minor, 150);
    assert_eq!(report.total_credit_minor, 150);
    assert!(source_version > 0);
}

#[tokio::test]
#[ignore = "requires disposable holding PostgreSQL write and read databases"]
async fn live_financial_consolidation_publishes_fx_ownership_and_elimination_evidence() {
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

    let data_profile_key = format!("holding-finance-report-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let parent_company = Uuid::new_v4();
    let subsidiary_company = Uuid::new_v4();
    let parent_period = period(holding_id, parent_company);
    let subsidiary_period = period(holding_id, subsidiary_company);
    let parent_chart = chart(holding_id, parent_company, Uuid::new_v4());
    let mut subsidiary_chart = chart(holding_id, subsidiary_company, Uuid::new_v4());
    subsidiary_chart.currency = "USD".into();
    let write_store =
        SqlxFinanceStore::new(write_pool.clone(), read_pool.clone(), &data_profile_key)
            .expect("write store");
    write_store
        .save_period(&parent_period, None)
        .await
        .expect("parent period");
    write_store
        .save_period(&subsidiary_period, None)
        .await
        .expect("subsidiary period");
    write_store
        .save_chart(&parent_chart, None)
        .await
        .expect("parent chart");
    write_store
        .save_chart(&subsidiary_chart, None)
        .await
        .expect("subsidiary chart");
    write_store
        .post_journal(
            journal(
                holding_id,
                parent_company,
                parent_period.id,
                100,
                "financial-report-parent",
            ),
            &parent_chart,
            &parent_period,
            Utc::now(),
        )
        .await
        .expect("parent journal");
    write_store
        .post_journal(
            journal(
                holding_id,
                subsidiary_company,
                subsidiary_period.id,
                50,
                "financial-report-subsidiary",
            ),
            &subsidiary_chart,
            &subsidiary_period,
            Utc::now(),
        )
        .await
        .expect("subsidiary journal");

    let mut cursor = None;
    loop {
        let page = write_store
            .read_source_event_page(cursor.as_ref(), 10)
            .await
            .expect("source page");
        for event in &page.events {
            write_store
                .project_event(
                    event.event_id,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )
                .await
                .expect("journal projection");
        }
        if !page.has_more {
            break;
        }
        cursor = page.next_cursor;
    }

    let ownership = ConsolidationOwnership::new(
        parent_company,
        vec![
            ConsolidationOwnershipInterest::new(
                parent_company,
                10_000,
                true,
                NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
                "ownership-ledger:parent",
            )
            .expect("parent ownership"),
            ConsolidationOwnershipInterest::new(
                subsidiary_company,
                6_000,
                true,
                NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
                "ownership-ledger:subsidiary",
            )
            .expect("subsidiary ownership"),
        ],
    )
    .expect("ownership");
    let rates = ConsolidationFxRateSet::new(vec![
        ConsolidationFxRate::new(
            "USD",
            "THB",
            NaiveDate::from_ymd_opt(2026, 7, 15).expect("date"),
            2,
            1,
            "central-bank-observation:2026-07-15",
        )
        .expect("external rate fact"),
    ])
    .expect("rate set");
    let eliminations = vec![
        ConsolidationEliminationEntry::new(
            Uuid::new_v4(),
            "THB",
            "intercompany balance elimination",
            vec![
                ConsolidationEliminationLine::new("1000", "Cash", AccountKind::Asset, 10, 0)
                    .expect("cash elimination"),
                ConsolidationEliminationLine::new("4000", "Sales", AccountKind::Revenue, 0, 10)
                    .expect("sales elimination"),
            ],
        )
        .expect("elimination"),
    ];
    let read_store = SqlxFinanceStore::new(write_pool.clone(), read_pool, &data_profile_key)
        .expect("read store");
    let report = read_store
        .consolidated_financial_report(
            Uuid::new_v4(),
            &[parent_chart, subsidiary_chart],
            "THB",
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
            ownership,
            rates,
            eliminations,
        )
        .await
        .expect("financial report");
    assert_eq!(report.total_debit_minor, 190);
    assert_eq!(report.non_controlling_interest_minor, 40);
}
