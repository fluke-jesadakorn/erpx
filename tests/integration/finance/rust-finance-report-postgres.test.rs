//! Actual native Finance trial balance and statements over leased PostgreSQL,
//! current Account/IAM authority and historical base ledger amounts.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::finance_journal_detail::*;
use erpx_service::finance_journal_detail_http::*;
use erpx_service::finance_journal_review::FinanceJournalEnvironment;
use facts::{Prepared, account, setup};
use std::sync::Arc;
use tower::ServiceExt;

fn service(p: &Prepared) -> FinanceJournalDetailService<facts::TestClock> {
    FinanceJournalDetailService::new(
        p.authorizer.clone(),
        FinanceJournalReadConfiguration::new(
            FinanceJournalEnvironment::Development,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            Arc::new(current_package_catalogue().unwrap()),
        )
        .unwrap(),
        p.system.scenario.clock.clone(),
    )
}

fn owned_maker(
    p: &Prepared,
) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, erpx_server_shared::public::BusinessError>
       + Send
       + 'static {
    let request = p.maker_selection.clone();
    move || Ok(request)
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('equity','company','chart','3000','Opening equity','equity','THB',0,'none','active',1,0,0),\
         ('rent','company','chart','5000','Rent expense','expense','THB',0,'none','active',1,0,0) ON CONFLICT DO NOTHING;\
         UPDATE finance_accounts SET currency='THB' WHERE company_id='company' AND id IN ('cash','sales');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    let journals = [
        ("report-jan", "J-REPORT-JAN", "2026-01-10", "manual", "2026-01-01", "2026-01-31"),
        ("report-feb", "J-REPORT-FEB", "2026-02-10", "manual", "2026-02-01", "2026-02-28"),
        ("report-opening", "J-REPORT-OPEN", "2025-12-31", "manual", "2025-12-01", "2025-12-31"),
        ("report-close", "J-REPORT-CLOSE", "2026-03-31", "fiscal_close", "2026-03-01", "2026-03-31"),
    ];
    for (id, number, date, source, start, end) in journals {
        let period_id = if id == "report-jan" {
            "period".to_owned()
        } else {
            format!("{id}-period")
        };
        sqlx::query(
            "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at)VALUES($1,'company',$2,$3,$4,0) ON CONFLICT DO NOTHING",
        )
        .bind(&period_id)
        .bind(format!("Period {number}"))
        .bind(start)
        .bind(end)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at)VALUES($1,'company','chart',$2,$3,$4,'THB',$5,'Report','draft',1,'fixture-a','fixture-b',0,0,0) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(&period_id)
        .bind(number)
        .bind(date)
        .bind(source)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    let lines = [
        ("report-jan-cash", "report-jan", "cash", 100_000i64, 0i64),
        ("report-jan-sales", "report-jan", "sales", 0, 100_000),
        ("report-feb-rent", "report-feb", "rent", 20_000, 0),
        ("report-feb-cash", "report-feb", "cash", 0, 20_000),
        ("report-opening-cash", "report-opening", "cash", 50_000, 0),
        ("report-opening-equity", "report-opening", "equity", 0, 50_000),
        ("report-close-sales", "report-close", "sales", 100_000, 0),
        ("report-close-equity", "report-close", "equity", 0, 100_000),
    ];
    for (index, (id, journal, account, debit, credit)) in lines.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal)VALUES($1,$2,$3,'Report',$4,$5,$4,$5,'{}',$6)",
        )
        .bind(id)
        .bind(journal)
        .bind(account)
        .bind(debit)
        .bind(credit)
        .bind(index as i64)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE finance_journals SET status='posted' WHERE id IN ('report-jan','report-feb','report-opening','report-close')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn report_reads_trial_balance_and_statements() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        let report = service(&p)
            .finance_report(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                Some("chart".into()),
                Some("period".into()),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(report.data.dates.start_date.as_deref(), Some("2026-01-01"));
        assert_eq!(report.data.dates.end_date.as_deref(), Some("2026-01-31"));
        assert_eq!(report.data.balance_sheet.assets, "1500.00");
        assert_eq!(report.data.balance_sheet.equity, "1500.00");
        assert_eq!(report.data.balance_sheet.recorded_equity, "500.00");
        assert_eq!(report.data.balance_sheet.unclosed_profit, "1000.00");
        assert_eq!(report.data.profit_and_loss.revenue, "1000.00");
        assert_eq!(report.data.profit_and_loss.expenses, "0.00");
        assert_eq!(report.data.profit_and_loss.net_profit, "1000.00");
        assert_eq!(report.data.trial_balance.len(), 2);
        let cash = report
            .data
            .trial_balance
            .iter()
            .find(|row| row["id"] == "cash")
            .unwrap();
        assert_eq!(cash["debit"], "100000");
        assert_eq!(cash["balance"], "100000");

        let february = service(&p)
            .finance_report(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                Some("chart".into()),
                None,
                Some("2026-01-01".into()),
                Some("2026-02-28".into()),
            )
            .await
            .unwrap();
        assert_eq!(february.data.profit_and_loss.revenue, "1000.00");
        assert_eq!(february.data.profit_and_loss.expenses, "200.00");
        assert_eq!(february.data.profit_and_loss.net_profit, "800.00");
        assert_eq!(february.data.balance_sheet.assets, "1300.00");
        assert_eq!(february.data.balance_sheet.equity, "1300.00");

        // Fiscal-close journals are excluded from the P&L but retained in the
        // balance sheet.
        let march = service(&p)
            .finance_report(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                Some("chart".into()),
                None,
                Some("2026-01-01".into()),
                Some("2026-03-31".into()),
            )
            .await
            .unwrap();
        assert_eq!(march.data.profit_and_loss.revenue, "1000.00");
        assert_eq!(march.data.profit_and_loss.expenses, "200.00");
        assert_eq!(march.data.profit_and_loss.net_profit, "800.00");
        assert_eq!(march.data.balance_sheet.assets, "1300.00");
        assert_eq!(march.data.balance_sheet.recorded_equity, "1500.00");
        assert_eq!(march.data.balance_sheet.unclosed_profit, "-200.00");

        // Chart and Company isolation.
        match service(&p)
            .finance_report(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                Some("book-chart".into()),
                None,
                Some("2026-01-01".into()),
                Some("2026-02-28".into()),
            )
            .await
        {
            Ok(empty) => assert!(empty.data.trial_balance.is_empty()),
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert!(matches!(error.kind, erpx_server_shared::public::FailureKind::NotFound));
            }
            other => panic!("foreign chart must be isolated: {other:?}"),
        }
        match service(&p)
            .finance_report(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                Some("chart".into()),
                None,
                Some("2026-03-01".into()),
                Some("2026-02-01".into()),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "FINANCE_REPORT_QUERY_INVALID");
            }
            other => panic!("inverted range must fail: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn report_http_route_accepts_real_session() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        let app = finance_journal_read_router(
            Arc::new(account::fixture::account::policy()),
            account::fixture::account::holding_registry(),
            Arc::new(FinanceJournalReadScopeHeaders),
            vec![FinanceJournalReadTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                service: Arc::new(service(&p)),
            }],
        )
        .unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/reports?chart=chart&periodId=period")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["balanceSheet"]["assets"], "1500.00");
        assert_eq!(value["data"]["profitAndLoss"]["netProfit"], "1000.00");
        assert_eq!(value["data"]["trialBalance"].as_array().unwrap().len(), 2);

        let invalid = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/reports?startDate=2026-03-01&endDate=2026-02-01")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.oneshot(invalid).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    })
    .await;
}
