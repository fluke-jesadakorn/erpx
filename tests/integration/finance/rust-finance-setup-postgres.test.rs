//! Actual native Finance chart-of-accounts management over leased PostgreSQL,
//! current Account/IAM authority and the caller's command receipts.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_finance::public::finance_setup::{
    AccountImportRow, ChangeAccountStatusInput, CreateAccountInput, CreateChartInput,
    CreatePeriodInput, ImportAccountsInput,
};
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{CommandKey, CompanyId, HoldingId};
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
         ('ar','company','chart','1100','Trade receivables','asset','THB',1,'receivable','active',1,0,0) ON CONFLICT DO NOTHING;\
         UPDATE finance_accounts SET currency='THB' WHERE company_id='company' AND id IN ('sales','cash','book-sales','book-cash');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('setup-customer','company','customer','SET-C','Setup customer','THB','active',1,0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn chart_accounts_read_and_chart_create_replay() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let read = service(&p)
            .finance_chart_accounts("acme.example.test", p.maker.clone(), owned_maker(&p), "chart".into())
            .await
            .unwrap();
        assert_eq!(read.data.chart["id"], "chart");
        assert!(read.data.accounts.iter().any(|row| row["code"] == "1100"));
        assert!(read.data.periods.iter().any(|row| row["id"] == "period"));

        let key = "setup-chart-0001";
        let created = service(&p)
            .create_chart(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                CreateChartInput {
                    name: "Native setup chart".into(),
                    base_currency: "THB".into(),
                    accounting_basis: "IFRS".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(created.data.id, format!("chart-{key}"));
        assert_eq!(created.data.version, 1);
        let row = sqlx::query("SELECT name,base_currency,accounting_basis,status,created_by FROM finance_charts WHERE id=$1")
            .bind(&created.data.id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        use sqlx::Row;
        assert_eq!(row.try_get::<String, _>("name").unwrap(), "Native setup chart");
        assert_eq!(row.try_get::<String, _>("base_currency").unwrap(), "THB");
        assert_eq!(row.try_get::<String, _>("accounting_basis").unwrap(), "IFRS");
        assert_eq!(row.try_get::<String, _>("status").unwrap(), "active");
        assert_eq!(row.try_get::<String, _>("created_by").unwrap(), "fixture-a");
        let receipt: String = sqlx::query_scalar(
            "SELECT operation FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt, "finance.chart.create");
        let audit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id=$1 AND action='finance.chart.create'",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, 1);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id=$1 AND event_type='finance.chart.create'",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 1);

        let replay = service(&p)
            .create_chart(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                CreateChartInput {
                    name: "Native setup chart".into(),
                    base_currency: "THB".into(),
                    accounting_basis: "IFRS".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(replay.data.id, created.data.id);
        match service(&p)
            .create_chart(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-chart-0002").unwrap(),
                CreateChartInput {
                    name: "Native setup chart".into(),
                    base_currency: "THB".into(),
                    accounting_basis: "IFRS".into(),
                },
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "CHART_EXISTS");
            }
            other => panic!("duplicate chart must conflict: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn account_create_import_and_status_guards() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let account_input = |code: &str, control: &str| CreateAccountInput {
            chart_id: "chart".into(),
            code: code.into(),
            name: format!("Native {code}"),
            category: "asset".into(),
            parent_id: None,
            currency: None,
            is_control: false,
            control_kind: control.into(),
        };
        let created = service(&p)
            .create_account(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-account-0001").unwrap(),
                "chart".into(),
                account_input("1300", "none"),
            )
            .await
            .unwrap();
        assert_eq!(created.data.id, "acct-setup-account-0001");
        assert_eq!(created.data.currency, "THB");
        assert!(!created.data.is_control);
        match service(&p)
            .create_account(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-account-0002").unwrap(),
                "chart".into(),
                account_input("1300", "none"),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "ACCOUNT_CODE_EXISTS");
            }
            other => panic!("duplicate account code must conflict: {other:?}"),
        }
        let control = service(&p)
            .create_account(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-account-control-0001").unwrap(),
                "chart".into(),
                account_input("1150", "receivable"),
            )
            .await
            .unwrap();
        assert!(control.data.is_control);
        assert_eq!(control.data.control_kind, "receivable");

        let imported = service(&p)
            .import_accounts(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-import-0001").unwrap(),
                ImportAccountsInput {
                    chart_id: "chart".into(),
                    rows: vec![
                        AccountImportRow {
                            code: "1400".into(),
                            name: "Imported prepaid".into(),
                            category: "asset".into(),
                            currency: None,
                            control_kind: None,
                        },
                        AccountImportRow {
                            code: "2400".into(),
                            name: "Imported payable".into(),
                            category: "liability".into(),
                            currency: None,
                            control_kind: Some("none".into()),
                        },
                    ],
                },
            )
            .await
            .unwrap();
        assert_eq!(imported.data.count, 2);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_accounts WHERE company_id='company' AND chart_id='chart' AND code IN ('1400','2400')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 2);

        let status = |status: &str, version: i64| ChangeAccountStatusInput {
            status: status.into(),
            expected_version: version,
            reason: "Native account lifecycle review".into(),
        };
        let inactive = service(&p)
            .change_account_status(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-status-0001").unwrap(),
                "chart".into(),
                created.data.id.clone(),
                status("inactive", 1),
            )
            .await
            .unwrap();
        assert_eq!(inactive.data.status, "inactive");
        assert_eq!(inactive.data.version, 2);
        match service(&p)
            .change_account_status(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-status-0002").unwrap(),
                "chart".into(),
                created.data.id.clone(),
                status("inactive", 2),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "ACCOUNT_STATUS_UNCHANGED");
            }
            other => panic!("unchanged status must conflict: {other:?}"),
        }
        match service(&p)
            .change_account_status(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-status-0003").unwrap(),
                "chart".into(),
                created.data.id.clone(),
                status("active", 1),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "VERSION_CONFLICT");
            }
            other => panic!("stale status must conflict: {other:?}"),
        }
        let active = service(&p)
            .change_account_status(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-status-0004").unwrap(),
                "chart".into(),
                created.data.id.clone(),
                status("active", 2),
            )
            .await
            .unwrap();
        assert_eq!(active.data.status, "active");
        assert_eq!(active.data.version, 3);
    })
    .await;
}

#[tokio::test]
async fn account_inactive_rejects_draft_journals_and_open_invoices() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let created = service(&p)
            .create_account(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-inactive-account-0001").unwrap(),
                "chart".into(),
                CreateAccountInput {
                    chart_id: "chart".into(),
                    code: "1300".into(),
                    name: "Native 1300".into(),
                    category: "asset".into(),
                    parent_id: None,
                    currency: None,
                    is_control: false,
                    control_kind: "none".into(),
                },
            )
            .await
            .unwrap();
        let account_id = created.data.id;
        sqlx::query(
            "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,memo,status,version,created_by,created_at,updated_at)VALUES('setup-draft-journal','company','chart','period','J-SETUP-DRAFT','2026-01-02','THB','manual','Draft','draft',1,'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal)VALUES('setup-draft-line','setup-draft-journal',$1,'Draft',100,0,100,0,'{}',0)",
        )
        .bind(&account_id)
        .execute(pool.raw())
        .await
        .unwrap();
        let status = |status: &str, version: i64| ChangeAccountStatusInput {
            status: status.into(),
            expected_version: version,
            reason: "Native account lifecycle review".into(),
        };
        match service(&p)
            .change_account_status(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-inactive-draft-0001").unwrap(),
                "chart".into(),
                account_id.clone(),
                status("inactive", 1),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "ACCOUNT_HAS_DRAFT_JOURNAL");
            }
            other => panic!("draft journal must block inactivity: {other:?}"),
        }

        sqlx::query("UPDATE finance_journals SET status='posted',posted_by='fixture-b',posted_at=0 WHERE id='setup-draft-journal'")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,base_total_minor,base_outstanding_minor)VALUES('setup-open-invoice','company','setup-customer','receivable','INV-SETUP-OPEN','2026-01-02','2026-01-31','THB',1000,0,1000,1000,$1,'sales','posted',1,'fixture-a',0,0,1000,1000)",
        )
        .bind(&account_id)
        .execute(pool.raw())
        .await
        .unwrap();
        match service(&p)
            .change_account_status(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-inactive-open-0001").unwrap(),
                "chart".into(),
                account_id,
                status("inactive", 1),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "ACCOUNT_HAS_OPEN_INVOICES");
            }
            other => panic!("open invoice must block inactivity: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn period_create_and_http_setup_routes() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        let created = service(&p)
            .create_period(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-period-0001").unwrap(),
                CreatePeriodInput {
                    name: "Native February".into(),
                    start_date: "2026-02-01".into(),
                    end_date: "2026-02-28".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(created.data.id, "period-setup-period-0001");
        assert_eq!(created.data.state, "open");
        assert_eq!(created.data.version, 0);
        match service(&p)
            .create_period(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-period-0002").unwrap(),
                CreatePeriodInput {
                    name: "Native February again".into(),
                    start_date: "2026-02-01".into(),
                    end_date: "2026-02-28".into(),
                },
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "PERIOD_EXISTS");
            }
            other => panic!("duplicate period must conflict: {other:?}"),
        }
        match service(&p)
            .create_period(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("setup-period-0003").unwrap(),
                CreatePeriodInput {
                    name: "Invalid period".into(),
                    start_date: "2026-03-31".into(),
                    end_date: "2026-03-01".into(),
                },
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "INVALID_PERIOD");
            }
            other => panic!("invalid period must fail: {other:?}"),
        }

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
        let read = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/charts/chart/accounts")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(read).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["chart"]["id"], "chart");

        let chart = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/charts")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", "setup-http-chart-0001")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(r#"{"name":"HTTP setup chart","baseCurrency":"THB","accountingBasis":"IFRS"}"#))
            .unwrap();
        let response = app.clone().oneshot(chart).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let account = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/charts/chart/accounts")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", "setup-http-account-0001")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(r#"{"chartId":"chart","code":"1800","name":"HTTP account","category":"asset","controlKind":"none"}"#))
            .unwrap();
        let response = app.clone().oneshot(account).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let period = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/periods")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", "setup-http-period-0001")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(r#"{"name":"HTTP March","startDate":"2026-03-01","endDate":"2026-03-31"}"#))
            .unwrap();
        let response = app.clone().oneshot(period).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    })
    .await;
}
