//! Actual native standalone customer-receipt detail, export and issue over
//! leased PostgreSQL, current Account/IAM authority and immutable issued copies.
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
         UPDATE finance_accounts SET currency='THB' WHERE company_id='company' AND id='sales';\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('receipt-customer','company','customer','RCT-C','Receipt customer','THB','active',1,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,base_total_minor,base_outstanding_minor)VALUES\
         ('receipt-invoice','company','receipt-customer','receivable','INV-RCT-1','2026-01-02','2026-01-31','THB',10000,0,10000,0,'ar','sales','posted',1,'fixture-a',0,0,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at)VALUES\
         ('receipt-journal','company','chart','period','J-RCT-1','2026-01-02','THB','manual','Receipt journal','posted',1,'fixture-a','fixture-b',0,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,journal_id,version,created_by,created_at,updated_at)VALUES\
         ('receipt-payment','company','receipt-customer','receipt','RCPT-RCT-1','2026-01-02','THB',10000,0,'settled','receipt-journal',1,'fixture-a',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor,created_by,created_at)VALUES\
         ('receipt-allocation','company','receipt-payment','receipt-invoice',10000,10000,10000,0,'fixture-a',0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,journal_id,version,created_by,created_at,updated_at)VALUES\
         ('pending-receipt-payment','company','receipt-customer','receipt','RCPT-PENDING-1','2026-01-02','THB',1000,1000,'released','receipt-journal',1,'fixture-a',0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn sales_receipt_issue_read_and_export() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let key = "receipt-issue-0001";
        let issued = service(&p)
            .issue_sales_receipt(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                "receipt-payment".into(),
            )
            .await
            .unwrap();
        assert_eq!(issued.data.id, "customer-receipt-receipt-payment");

        let read = service(&p)
            .sales_receipt(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                issued.data.id.clone(),
            )
            .await
            .unwrap()
            .expect("receipt is visible");
        assert_eq!(read.data.status, "issued");
        assert_eq!(read.data.snapshot["schema"], "finance.customer-receipt.v1");
        assert_eq!(read.data.snapshot["documentNumber"], "RC-2026-000001");
        assert_eq!(read.data.snapshot["companyName"], "Synthetic Acme");
        assert_eq!(read.data.snapshot["customerName"], "Receipt customer");
        assert_eq!(read.data.snapshot["amountMinor"], "10000");
        assert_eq!(read.data.snapshot["bankReference"], "RCPT-RCT-1");
        assert_eq!(read.data.snapshot["journalId"], "receipt-journal");
        assert_eq!(read.data.snapshot["allocations"][0]["invoiceNumber"], "INV-RCT-1");
        assert_eq!(read.data.snapshot["allocations"][0]["amountMinor"], "10000");
        assert_eq!(read.data.sha256.len(), 64);

        let export = service(&p)
            .export_sales_receipt(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                issued.data.id.clone(),
            )
            .await
            .unwrap()
            .expect("export is visible");
        assert_eq!(export.data.document_number, "RC-2026-000001");
        assert_eq!(export.data.sha256, read.data.sha256);
        assert!(export.data.content.contains("Receipt RC-2026-000001"));
        assert!(export.data.content.contains("Receipt customer"));
        assert!(export.data.content.contains("INV-RCT-1"));
        assert!(export.data.content.contains("Received THB 100.00"));

        // Exact replay returns the retained receipt without a second copy.
        let replay = service(&p)
            .issue_sales_receipt(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                "receipt-payment".into(),
            )
            .await
            .unwrap();
        assert_eq!(replay.data.id, issued.data.id);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_customer_receipts WHERE company_id='company' AND payment_id='receipt-payment'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 1);
        let commands: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id=$1 AND operation='finance.customer-receipt.issue'",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(commands, 1);

        // A branch-scoped read does not disclose a receipt without the original
        // invoice fulfillment lineage.
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let scoped = erpx_server_finance::public::sales_receipts::read_customer_receipt(
            &mut tx,
            "company",
            Some("branch"),
            None,
            "customer-receipt-receipt-payment",
        )
        .await
        .unwrap();
        assert!(scoped.is_none());
        let foreign = erpx_server_finance::public::sales_receipts::read_customer_receipt(
            &mut tx,
            "another-company",
            None,
            None,
            "customer-receipt-receipt-payment",
        )
        .await
        .unwrap();
        assert!(foreign.is_none());
        tx.commit().await.unwrap();

        // A payment without a posted, settled journal cannot issue a receipt.
        match service(&p)
            .issue_sales_receipt(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("receipt-pending-0001").unwrap(),
                "pending-receipt-payment".into(),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "POSTED_RECEIPT_REQUIRED");
            }
            other => panic!("unposted payment must fail: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn sales_receipt_http_routes_accept_real_session_and_command_key() {
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
        let key = "receipt-http-0001";
        let issue = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/sales/receipts/receipt-payment/issue")
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
            .body(Body::from("{}"))
            .unwrap();
        let response = app.clone().oneshot(issue).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        assert_eq!(value["data"]["id"], "customer-receipt-receipt-payment");

        let read = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/sales/receipts/customer-receipt-receipt-payment")
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
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["snapshot"]["documentNumber"], "RC-2026-000001");

        let export = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/sales/receipts/customer-receipt-receipt-payment/export")
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
        let response = app.oneshot(export).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
        assert!(response.headers().get("x-document-sha256").is_some());
    })
    .await;
}
