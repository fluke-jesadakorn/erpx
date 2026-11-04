//! Native generic payment create HTTP ingress, independent of Field receipt
//! configuration, under a normal Account session and Finance banking purchase.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::account_holding_bindings::{
    ConfiguredAccountHolding, ConfiguredHoldingRegistry,
};
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::finance_generic_payment_http::{GenericPaymentTarget, finance_generic_payment_router};
use erpx_service::finance_journal_create::FinanceJournalCreateConfiguration;
use erpx_service::finance_journal_review::FinanceJournalEnvironment;
use std::sync::Arc;
use tower::ServiceExt;

async fn seed(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('bank','company','chart','1000','Bank','asset','THB',1,'bank','active',1,0,0),\
         ('ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('generic-supplier','company','supplier','SUP-G','Synthetic Supplier','THB','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,source_type)VALUES\
         ('generic-bill','company','generic-supplier','payable','BILL-G','2026-01-02','2026-01-31','THB',10000,0,10000,10000,10000,10000,'bank','ap','posted',1,'fixture-a',0,0,'supplier_bill')\
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
    finance_generic_payment_router(
        Arc::new(policy),
        Arc::new(registry),
        Arc::new(erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders),
        vec![GenericPaymentTarget {
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
async fn generic_payment_http_creates_a_draft_without_field_receipt_configuration() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        seed(&p).await;
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/payments/generic")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "generic-payment-http")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"kind":"payment","partyId":"generic-supplier","paymentNumber":"PAY-G-1","paymentDate":"2026-01-10","currency":"THB","amount":"100.00","bankAccount":"bank","settlementInvoiceId":"generic-bill"}"#,
            ))
            .unwrap();
        let response = router(&p).oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        assert_eq!(value["data"]["status"], "draft");
        let row: (String, String, String, i64, String) = sqlx::query_as(
            "SELECT kind,party_id,bank_account,amount_minor,status FROM finance_payments WHERE company_id='company' AND payment_number='PAY-G-1'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(row, ("payment".into(), "generic-supplier".into(), "bank".into(), 10000, "draft".into()));
        let (audit, outbox): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*)::bigint FROM audit_events WHERE command_id='generic-payment-http'),\
             (SELECT count(*)::bigint FROM outbox_events WHERE command_id='generic-payment-http')",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!((audit, outbox), (1, 1));
    })
    .await;
}
