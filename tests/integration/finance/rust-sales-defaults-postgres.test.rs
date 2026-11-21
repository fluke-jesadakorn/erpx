//! Actual native Finance sales-defaults workspace, prepare and independent
//! decision over leased PostgreSQL and the caller's command receipts.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_finance::public::sales_defaults::{
    SalesDefaultsDecisionInput, SalesDefaultsPrepareInput,
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

fn maker(p: &Prepared) -> HoldingWorkspaceRequest {
    p.maker_selection.clone()
}

fn reviewer(p: &Prepared) -> HoldingWorkspaceRequest {
    p.reviewer_selection.clone()
}

fn owned_maker(
    p: &Prepared,
) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, erpx_server_shared::public::BusinessError>
       + Send
       + 'static {
    let request = maker(p);
    move || Ok(request)
}

fn owned_reviewer(
    p: &Prepared,
) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, erpx_server_shared::public::BusinessError>
       + Send
       + 'static {
    let request = reviewer(p);
    move || Ok(request)
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('ar','company','chart','1100','Trade receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('vat','company','chart','2200','Output VAT','liability','THB',0,'none','active',1,0,0) ON CONFLICT DO NOTHING;\
         UPDATE finance_accounts SET currency='THB' WHERE company_id='company' AND id='sales';\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('defaults-customer','company','customer','DEF-C','Defaults customer','THB','active',1,0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn prepare_input() -> SalesDefaultsPrepareInput {
    SalesDefaultsPrepareInput {
        chart_id: "chart".into(),
        currency: "THB".into(),
        receivable_account_id: "ar".into(),
        revenue_account_id: "sales".into(),
        tax_account_id: Some("vat".into()),
        reason: "Approved sales accounts for native defaults".into(),
    }
}

fn decision(action: &str, version: i64) -> SalesDefaultsDecisionInput {
    SalesDefaultsDecisionInput {
        expected_version: version,
        action: action.into(),
        reason: "Independent review of the native sales accounts".into(),
    }
}

#[tokio::test]
async fn sales_defaults_prepare_and_independent_approval() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let workspace = service(&p)
            .sales_defaults_workspace("acme.example.test", p.maker.clone(), owned_maker(&p))
            .await
            .unwrap();
        assert!(!workspace.data.options.charts.is_empty());
        assert!(workspace
            .data
            .options
            .accounts
            .iter()
            .any(|row| row["id"] == "ar"));
        assert!(workspace
            .data
            .options
            .parties
            .iter()
            .any(|row| row["id"] == "defaults-customer"));
        assert!(workspace.data.editions.is_empty());
        assert!(workspace.data.usable_defaults.is_empty());

        let key = "sales-defaults-0001";
        let prepared = service(&p)
            .prepare_sales_defaults(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                prepare_input(),
            )
            .await
            .unwrap();
        assert_eq!(prepared.data.id, format!("sales-defaults-{key}"));
        assert_eq!(prepared.data.status, "draft");
        assert_eq!(prepared.data.version, 1);

        let replay = service(&p)
            .prepare_sales_defaults(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                prepare_input(),
            )
            .await
            .unwrap();
        assert_eq!(replay.data.id, prepared.data.id);
        let drafts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_sales_defaults WHERE company_id='company' AND status='draft'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(drafts, 1);

        // The creator cannot approve their own edition.
        match service(&p)
            .decide_sales_defaults(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-defaults-self-0001").unwrap(),
                prepared.data.id.clone(),
                decision("approve", 1),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "INDEPENDENT_APPROVER_REQUIRED");
            }
            other => panic!("self approval must fail: {other:?}"),
        }

        let approved = service(&p)
            .decide_sales_defaults(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("sales-defaults-approve-0001").unwrap(),
                prepared.data.id.clone(),
                decision("approve", 1),
            )
            .await
            .unwrap();
        assert_eq!(approved.data.status, "approved");
        assert_eq!(approved.data.version, 2);

        let after = service(&p)
            .sales_defaults_workspace("acme.example.test", p.maker.clone(), owned_maker(&p))
            .await
            .unwrap();
        assert_eq!(after.data.usable_defaults, vec![prepared.data.id.clone()]);
        assert!(after.data.issues.is_empty());
        let edition = after
            .data
            .editions
            .iter()
            .find(|row| row["id"] == prepared.data.id.as_str())
            .unwrap();
        assert_eq!(edition["status"], "approved");
        assert_eq!(edition["reviewed_by"], "fixture-b");

        // A newer approved edition supersedes the previous one for the currency.
        let second = service(&p)
            .prepare_sales_defaults(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-defaults-0002").unwrap(),
                prepare_input(),
            )
            .await
            .unwrap();
        service(&p)
            .decide_sales_defaults(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("sales-defaults-approve-0002").unwrap(),
                second.data.id.clone(),
                decision("approve", 1),
            )
            .await
            .unwrap();
        let final_state = service(&p)
            .sales_defaults_workspace("acme.example.test", p.maker.clone(), owned_maker(&p))
            .await
            .unwrap();
        assert_eq!(final_state.data.usable_defaults, vec![second.data.id.clone()]);
        let old = final_state
            .data
            .editions
            .iter()
            .find(|row| row["id"] == prepared.data.id.as_str())
            .unwrap();
        assert_eq!(old["status"], "superseded");

        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE operation IN ('finance.sales-defaults.prepare','finance.sales-defaults.decide') AND actor_id IN ('fixture-a','fixture-b')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 4);
    })
    .await;
}

#[tokio::test]
async fn sales_defaults_reject_branch_scope_duplicate_accounts_and_stale_version() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        let mut branch = maker(&p);
        branch.branch_slug = Some("branch".into());
        match service(&p)
            .prepare_sales_defaults(
                "acme.example.test",
                p.maker.clone(),
                move || Ok(branch.clone()),
                CommandKey::new("sales-defaults-scope-0001").unwrap(),
                prepare_input(),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert!(matches!(
                    error.kind,
                    erpx_server_shared::public::FailureKind::Conflict
                        | erpx_server_shared::public::FailureKind::Forbidden
                        | erpx_server_shared::public::FailureKind::NotFound
                ));
            }
            other => panic!("branch scope must fail: {other:?}"),
        }
        let mut duplicate = prepare_input();
        duplicate.revenue_account_id = "ar".into();
        match service(&p)
            .prepare_sales_defaults(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-defaults-duplicate-0001").unwrap(),
                duplicate,
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SALES_ACCOUNTS_INVALID");
            }
            other => panic!("duplicate accounts must fail: {other:?}"),
        }
        let prepared = service(&p)
            .prepare_sales_defaults(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-defaults-stale-0001").unwrap(),
                prepare_input(),
            )
            .await
            .unwrap();
        match service(&p)
            .decide_sales_defaults(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("sales-defaults-stale-0002").unwrap(),
                prepared.data.id,
                decision("approve", 9),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "VERSION_CONFLICT");
            }
            other => panic!("stale version must fail: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn sales_defaults_http_routes_accept_real_session_and_command_key() {
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
        let read = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/sales/defaults")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
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
        assert!(!value["data"]["options"]["charts"].as_array().unwrap().is_empty());

        let key = "sales-defaults-http-0001";
        let prepare = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/sales/defaults")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", key)
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(serde_json::to_vec(&prepare_input()).unwrap()))
            .unwrap();
        let response = app.clone().oneshot(prepare).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        assert_eq!(value["data"]["id"], format!("sales-defaults-{key}"));
        assert_eq!(value["data"]["status"], "draft");

        let decide = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/holdings/acme/finance/sales/defaults/{}/decide",
                value["data"]["id"].as_str().unwrap()
            ))
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.reviewer_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", "sales-defaults-http-0002")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-b")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(serde_json::to_vec(&decision("approve", 1)).unwrap()))
            .unwrap();
        let response = app.oneshot(decide).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    })
    .await;
}
