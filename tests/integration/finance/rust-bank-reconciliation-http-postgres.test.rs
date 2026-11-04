//! Native Finance bank HTTP import/reconcile under the actual Account session.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::account_holding_bindings::{
    ConfiguredAccountHolding, ConfiguredHoldingRegistry,
};
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::finance_bank_http::{FinanceBankTarget, finance_bank_router};
use erpx_service::finance_journal_create::FinanceJournalCreateConfiguration;
use erpx_service::finance_journal_review::FinanceJournalEnvironment;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use std::sync::Arc;
use tower::ServiceExt;

async fn seed(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('bank','company','chart','1000','Bank','asset','THB',1,'bank','active',1,0,0),\
         ('ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('bank-supplier','company','supplier','SUP-B','Synthetic Supplier','THB','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,status,created_by,posted_by,posted_at,created_at,updated_at)VALUES\
         ('bank-journal','company','chart','period','J-BANK','2026-01-01','THB','posted','fixture-a','fixture-a',0,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,version,created_by,created_at,updated_at)VALUES\
         ('bank-payment','company','bank-supplier','receipt','RC-BANK','2026-01-01','THB',1500,1500,'bank','released','bank-journal',3,'fixture-a',0,0)\
         ON CONFLICT DO NOTHING;",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
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
    let configuration = FinanceJournalCreateConfiguration::new(
        FinanceJournalEnvironment::Development,
        HoldingId::new("h001").unwrap(),
        CompanyId::new("company").unwrap(),
        Arc::new(current_package_catalogue().unwrap()),
    )
    .unwrap();
    finance_bank_router(
        Arc::new(policy),
        Arc::new(registry),
        vec![FinanceBankTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            authorizer: p.authorizer.clone(),
            configuration,
            clock: p.system.scenario.clock.clone(),
        }],
    )
    .unwrap()
}

#[tokio::test]
async fn bank_http_imports_and_reconciles_under_normal_finance_session() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        seed(&p).await;
        let app = router(&p);
        let cookie = format!("__Host-erpx-product={}", p.maker_token.expose());
        let import = r#"{"bankAccount":"bank","statementNumber":"ST-HTTP","statementDate":"2026-01-31","opening":"0.00","closing":"15.00","closingFxRate":"1.0","sourceHash":"http-source","rows":[{"externalId":"HTTP-1","transactionDate":"2026-01-05","description":"Settlement","amount":"15.00","currency":"THB"}]}"#;
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/bank/statements/import")
            .header("host", "acme.example.test")
            .header("cookie", &cookie)
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "bank-http-import")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(import))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        assert_eq!(value["data"]["id"], "statement-bank-http-import");
        assert_eq!(value["data"]["imported"], 1);
        assert_eq!(value["data"]["status"], "imported");

        let reconcile = r#"{"expectedVersion":1,"matchedType":"payment","matchedId":"bank-payment","proof":"Reviewed the statement row against the released receipt.","attachmentSetIds":[]}"#;
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/bank/transactions/bankrow-bank-http-import-0/reconcile")
            .header("host", "acme.example.test")
            .header("cookie", &cookie)
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "bank-http-reconcile")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(reconcile))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["data"]["status"], "matched");
        assert_eq!(value["data"]["version"], 2);
        let matched: (String, String, i64) = sqlx::query_as(
            "SELECT match_status,matched_id,version FROM finance_bank_transactions WHERE id='bankrow-bank-http-import-0'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(matched, ("matched".into(), "bank-payment".into(), 2));
    })
    .await;
}
