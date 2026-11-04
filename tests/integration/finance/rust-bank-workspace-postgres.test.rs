//! Native Finance bank workspace read over leased PostgreSQL: exact Company,
//! chart, cursor, unmatched-payment and selected-movement evidence.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_finance::public::FinanceActor;
use erpx_server_finance::public::bank_workspace::{BankWorkspaceQuery, read_bank_workspace};
use erpx_server_identity_access::public::account_holding_bindings::{
    ConfiguredAccountHolding, ConfiguredHoldingRegistry,
};
use erpx_server_shared::public::{AssignmentId, CompanyId, HoldingId, UserId};
use erpx_service::finance_bank_workspace::{
    BankWorkspaceScopeHeaders, FinanceBankWorkspaceConfiguration, FinanceBankWorkspaceService,
    FinanceBankWorkspaceTarget, finance_bank_workspace_router,
};
use std::collections::BTreeSet;
use std::sync::Arc;
use tower::ServiceExt;

fn actor(user: &str) -> FinanceActor {
    FinanceActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new("bank-workspace-fixture").unwrap(),
    }
}

async fn seed(p: &review::Prepared) {
    let pool = &p.system.holdings[0];
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('bank','company','chart','1000','Operating bank','asset','THB',1,'bank','active',1,0,0),\
         ('idle-bank','company','chart','1001','Idle bank','asset','THB',1,'bank','inactive',1,0,0),\
         ('receivable','company','chart','1100','Trade receivable','asset','THB',1,'receivable','active',1,0,0),\
         ('book-bank','company','book-chart','1000','Secondary bank','asset','THB',1,'bank','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,status,created_by,posted_by,posted_at,created_at,updated_at)VALUES\
         ('bank-journal','company','chart','period','J-BANK-WS','2026-01-01','THB','posted','fixture-a','fixture-a',0,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,version,created_by,created_at,updated_at)VALUES\
         ('payment-unmatched-released','company','receipt','RC-WS-1','2026-01-20','THB',1500,1500,'bank','released','bank-journal',2,'fixture-a',0,0),\
         ('payment-unmatched-settled','company','payment','PM-WS-2','2026-01-19','THB',900,0,'bank','settled','bank-journal',2,'fixture-a',0,0),\
         ('payment-matched','company','receipt','RC-WS-3','2026-01-18','THB',1500,1500,'bank','released','bank-journal',2,'fixture-a',0,0),\
         ('payment-draft','company','receipt','RC-WS-4','2026-01-17','THB',1500,1500,'bank','draft','bank-journal',2,'fixture-a',0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,version,imported_by,created_at)\
         SELECT 'stmt-'||lpad(g::text,3,'0'),'company','bank','ST-'||g,to_char(date '2026-01-01' + g,'YYYY-MM-DD'),0,0,'ws-hash-'||g,1,'fixture-a',0 \
         FROM generate_series(1,35) g;\
         INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,version)VALUES\
         ('txn-matched','stmt-001','company','2026-02-01','EXT-MATCHED','Matched movement',1500,'THB','matched',2);\
         INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,version)\
         SELECT 'txn-'||lpad(g::text,3,'0'),'stmt-'||lpad(g::text,3,'0'),'company',to_char(date '2026-01-01' + g,'YYYY-MM-DD'),'EXT-'||g,'Workspace movement '||g,1000+g,'THB','unmatched',1 \
         FROM generate_series(1,35) g;\
         UPDATE finance_bank_transactions SET matched_type='payment',matched_id='payment-matched' WHERE id='txn-matched';\
         INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,version,imported_by,created_at)VALUES\
         ('book-stmt','company','book-bank','ST-BOOK','2026-01-15',0,0,'ws-hash-book',1,'fixture-a',0);\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES\
         ('foreign-chart','foreign','Foreign ledger','THB','fixture-a',0,0);\
         INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('foreign-bank','foreign','foreign-chart','1000','Foreign bank','asset','THB',1,'bank','active',1,0,0);\
         INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,version,imported_by,created_at)VALUES\
         ('foreign-stmt','foreign','foreign-bank','ST-FOREIGN','2026-01-31',0,0,'ws-hash-foreign',1,'fixture-a',0);\
         INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,version)VALUES\
         ('foreign-txn','foreign-stmt','foreign','2026-01-31','EXT-FOREIGN','Foreign movement',100,'THB','unmatched',1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    // The reviewed secondary book is committed after the seeded journals so
    // the deferred primary-book projection does not invent secondary journals.
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('book-idle-bank','company','book-chart','1001','Secondary idle bank','asset','THB',1,'bank','active',1,0,0),\
         ('book-receivable','company','book-chart','1100','Secondary receivable','asset','THB',1,'receivable','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,status,version,prepared_by,reviewed_by,reason,opening_json,opening_hash,opening_period_id,created_at,reviewed_at)VALUES\
         ('ws-rule','company','chart','book-chart','2026-01-01','draft',1,'fixture-a','fixture-b','Synthetic approved secondary book','{}','ws-opening-hash','period',0,0);\
         INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id)VALUES\
         ('ws-rule','cash','book-cash'),('ws-rule','sales','book-sales'),\
         ('ws-rule','bank','book-bank'),('ws-rule','idle-bank','book-idle-bank'),\
         ('ws-rule','receivable','book-receivable');\
         UPDATE finance_book_rules SET status='approved' WHERE id='ws-rule'",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn ids(rows: &[serde_json::Value]) -> BTreeSet<String> {
    rows.iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn owner_read(p: &review::Prepared, query: &BankWorkspaceQuery) -> serde_json::Value {
    let pool = &p.system.holdings[0];
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    let data = read_bank_workspace(&mut tx, "company", query)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    serde_json::to_value(data).unwrap()
}

#[tokio::test]
async fn reads_company_chart_cursor_payments_and_selected_transaction() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        seed(&p).await;

        let first = owner_read(
            &p,
            &BankWorkspaceQuery {
                chart: Some("chart".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            first["statementPage"]["items"].as_array().unwrap().len(),
            30
        );
        assert_eq!(first["statementPage"]["hasMore"], true);
        assert_eq!(
            first["transactionPage"]["items"].as_array().unwrap().len(),
            30
        );
        assert_eq!(first["accounts"].as_array().unwrap().len(), 0);
        assert_eq!(first["payments"].as_array().unwrap().len(), 0);
        assert!(first["selectedTransaction"].is_null());
        let statements = first["statements"].as_array().unwrap();
        assert!(statements.iter().all(|row| row["company_id"] == "company"));
        assert!(statements.iter().all(|row| row["base_currency"] == "THB"));

        let statement_cursor = first["statementPage"]["nextCursor"]
            .as_str()
            .unwrap()
            .to_owned();
        let transaction_cursor = first["transactionPage"]["nextCursor"]
            .as_str()
            .unwrap()
            .to_owned();
        let second = owner_read(
            &p,
            &BankWorkspaceQuery {
                chart: Some("chart".into()),
                statement_cursor: Some(statement_cursor),
                transaction_cursor: Some(transaction_cursor),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(second["statements"].as_array().unwrap().len(), 5);
        assert_eq!(second["transactions"].as_array().unwrap().len(), 6);
        assert_eq!(second["statementPage"]["hasMore"], false);
        assert!(second["statementPage"]["previousCursor"].as_str().is_some());
        assert!(
            second["transactionPage"]["previousCursor"]
                .as_str()
                .is_some()
        );

        let matched = owner_read(
            &p,
            &BankWorkspaceQuery {
                chart: Some("chart".into()),
                matched_transaction_id: Some("txn-010".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(matched["selectedTransaction"]["id"], "txn-010");
        assert_eq!(matched["selectedTransaction"]["statement_number"], "ST-10");
        assert_eq!(matched["selectedTransaction"]["bank_account"], "bank");
        let payments = matched["payments"].as_array().unwrap();
        let payment_ids = ids(payments);
        assert_eq!(
            payment_ids,
            ["payment-unmatched-released", "payment-unmatched-settled"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );

        let accounts = owner_read(
            &p,
            &BankWorkspaceQuery {
                new: Some("1".into()),
                chart: Some("chart".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            ids(accounts["accounts"].as_array().unwrap()),
            ["bank", "cash"].into_iter().map(str::to_owned).collect()
        );

        let book = owner_read(
            &p,
            &BankWorkspaceQuery {
                chart: Some("book-chart".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(ids(book["statements"].as_array().unwrap()).len(), 1);
        assert_eq!(book["statements"][0]["id"], "book-stmt");

        let foreign = owner_read(
            &p,
            &BankWorkspaceQuery {
                chart: Some("foreign-chart".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(foreign["statements"].as_array().unwrap().len(), 0);
        assert_eq!(foreign["transactions"].as_array().unwrap().len(), 0);

        let printing = owner_read(
            &p,
            &BankWorkspaceQuery {
                print: Some("1".into()),
                chart: Some("chart".into()),
                ..Default::default()
            },
        )
        .await;
        assert!(printing["statementPage"].is_null());
        assert!(printing["transactionPage"].is_null());
        assert_eq!(printing["statements"].as_array().unwrap().len(), 35);
        assert_eq!(printing["transactions"].as_array().unwrap().len(), 36);

        let missing = {
            let pool = &p.system.holdings[0];
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            read_bank_workspace(
                &mut tx,
                "company",
                &BankWorkspaceQuery {
                    matched_transaction_id: Some("missing-movement".into()),
                    ..Default::default()
                },
            )
            .await
        };
        match missing {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "BANK_TRANSACTION_NOT_FOUND")
            }
            Err(other) => panic!("expected BANK_TRANSACTION_NOT_FOUND, got {other:?}"),
            Ok(_) => panic!("expected BANK_TRANSACTION_NOT_FOUND, got success"),
        }
    })
    .await;
}

fn service(p: &review::Prepared) -> FinanceBankWorkspaceService<review::TestClock> {
    FinanceBankWorkspaceService::new(
        p.authorizer.clone(),
        FinanceBankWorkspaceConfiguration::new(
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            Arc::new(current_package_catalogue().unwrap()),
        )
        .unwrap(),
        p.system.scenario.clock.clone(),
    )
}

#[tokio::test]
async fn denies_without_the_current_banking_purchase() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let selection = p.maker_selection.clone();
        let result = service(&p)
            .read(
                "acme.example.test",
                p.maker.clone(),
                move || Ok(selection),
                BankWorkspaceQuery::default(),
            )
            .await;
        match result {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "PACKAGE_REQUIRED")
            }
            Err(other) => panic!("expected PACKAGE_REQUIRED, got {other:?}"),
            Ok(_) => panic!("expected PACKAGE_REQUIRED, got success"),
        }
    })
    .await;
}

#[tokio::test]
async fn denies_a_changed_selected_assignment() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        let selection = p.reviewer_selection.clone();
        let result = service(&p)
            .read(
                "acme.example.test",
                p.maker.clone(),
                move || Ok(selection),
                BankWorkspaceQuery::default(),
            )
            .await;
        match result {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert!(matches!(
                    error.code,
                    "WORK_CONTEXT_CHANGED" | "WORK_ASSIGNMENT_CHANGED" | "FORBIDDEN"
                ))
            }
            Err(other) => panic!("expected a current-authority denial, got {other:?}"),
            Ok(_) => panic!("expected a current-authority denial, got success"),
        }
    })
    .await;
}

fn router(p: &review::Prepared) -> axum::Router {
    let policy = review::account::fixture::account::policy();
    let registry = ConfiguredHoldingRegistry::new(
        &policy,
        vec![
            ConfiguredAccountHolding {
                holding_id: HoldingId::new("h001").unwrap(),
                slug: "acme".into(),
                write_binding: "HOLDING_H001_DB".into(),
                read_binding: "HOLDING_H001_READ_DB".into(),
                files_binding: "HOLDING_H001_FILES".into(),
            },
            ConfiguredAccountHolding {
                holding_id: HoldingId::new("h002").unwrap(),
                slug: "northstar".into(),
                write_binding: "HOLDING_H002_DB".into(),
                read_binding: "HOLDING_H002_READ_DB".into(),
                files_binding: "HOLDING_H002_FILES".into(),
            },
        ],
    )
    .unwrap();
    finance_bank_workspace_router(
        Arc::new(policy),
        Arc::new(registry),
        Arc::new(BankWorkspaceScopeHeaders),
        vec![FinanceBankWorkspaceTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            service: Arc::new(service(p)),
        }],
    )
    .unwrap()
}

fn get(path: &str, cookie: &str, company: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("host", "acme.example.test")
        .header("cookie", cookie)
        .header("origin", "https://acme.example.test")
        .header("x-erpx-context-company", company)
        .header("x-erpx-context-assignment", "finance-fixture-a")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "3")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn bank_workspace_http_reads_under_a_normal_finance_session() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        seed(&p).await;
        let app = router(&p);
        let cookie = format!("__Host-erpx-product={}", p.maker_token.expose());
        let request = get(
            "/api/v1/holdings/acme/finance/bank?chart=chart&view=unmatched&match=txn-010",
            &cookie,
            "company",
        );
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        for (name, value) in [
            ("cache-control", "private, no-store"),
            ("referrer-policy", "no-referrer"),
            ("x-content-type-options", "nosniff"),
        ] {
            assert_eq!(response.headers().get(name).unwrap(), value);
        }
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["statements"].as_array().unwrap().len(), 30);
        assert_eq!(value["data"]["payments"].as_array().unwrap().len(), 2);
        assert_eq!(value["data"]["selectedTransaction"]["id"], "txn-010");

        let response = app
            .clone()
            .oneshot(get(
                "/api/v1/holdings/acme/finance/bank?match=missing-movement",
                &cookie,
                "company",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "BANK_TRANSACTION_NOT_FOUND");

        let response = app
            .oneshot(get(
                "/api/v1/holdings/acme/finance/bank?chart=chart",
                &cookie,
                "foreign",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    })
    .await;
}
