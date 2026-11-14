//! Actual native standalone sales-invoice create over leased PostgreSQL,
//! current Account/IAM authority, approved sales defaults and the caller's
//! command receipt.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_finance::public::sales_invoice_create::{
    SalesInvoiceCreateInput, SalesInvoiceCreateLine,
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

fn selection(p: &Prepared) -> HoldingWorkspaceRequest {
    p.maker_selection.clone()
}

fn owned_selection(
    p: &Prepared,
) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, erpx_server_shared::public::BusinessError>
       + Send
       + 'static {
    let request = selection(p);
    move || Ok(request)
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('ar','company','chart','1100','Trade receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('vat','company','chart','2200','Output VAT','liability','THB',0,'none','active',1,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id,status,reason,created_by,reviewed_by,review_reason,version,created_at,updated_at)VALUES\
         ('sales-defaults-1','company','chart','THB','ar','sales','vat','approved','Approved sales accounts for native invoice creation','fixture-a','fixture-b','Independent review of the sales accounts',1,0,0) ON CONFLICT DO NOTHING;\
         UPDATE finance_accounts SET currency='THB' WHERE company_id='company' AND id='sales';\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('sales-customer','company','customer','SAL-C','Native sales customer','THB','active',1,0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn input() -> SalesInvoiceCreateInput {
    SalesInvoiceCreateInput {
        party_id: "sales-customer".into(),
        invoice_date: "2026-01-02".into(),
        due_date: "2026-01-31".into(),
        currency: "THB".into(),
        completion_confirmed: true,
        completion_note: "Customer accepted the completed installation".into(),
        lines: vec![
            SalesInvoiceCreateLine {
                description: "Installation".into(),
                quantity: "2".into(),
                unit_price: "100.00".into(),
                tax_bps: 700,
            },
            SalesInvoiceCreateLine {
                description: "On-site training".into(),
                quantity: "1.5".into(),
                unit_price: "80.00".into(),
                tax_bps: 700,
            },
        ],
    }
}

#[tokio::test]
async fn sales_invoice_create_writes_lines_money_and_receipts() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let key = "sales-create-0001";
        let result = service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_selection(&p),
                CommandKey::new(key).unwrap(),
                input(),
            )
            .await
            .unwrap();
        assert_eq!(result.data.id, format!("invoice-{key}-invoice"));
        assert_eq!(result.data.number, "INV-2026-000001");
        assert_eq!(result.data.total, "342.40");
        assert_eq!(result.data.outstanding, "342.40");
        assert_eq!(result.data.status, "draft");
        assert_eq!(result.data.version, 1);
        assert_eq!(result.data.completion_evidence.invoice_revision, 1);
        assert_eq!(result.data.completion_evidence.confirmed_by, "fixture-a");

        let invoice = sqlx::query(
            "SELECT kind,invoice_number,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,status,version,source_type,line_mode,line_revision,debit_account_id,credit_account_id,tax_account_id FROM finance_invoices WHERE id=$1 AND company_id='company'",
        )
        .bind(&result.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(invoice.try_get::<String, _>("kind").unwrap(), "receivable");
        assert_eq!(invoice.try_get::<String, _>("invoice_number").unwrap(), "INV-2026-000001");
        assert_eq!(invoice.try_get::<i64, _>("subtotal_minor").unwrap(), 32_000);
        assert_eq!(invoice.try_get::<i64, _>("tax_minor").unwrap(), 2_240);
        assert_eq!(invoice.try_get::<i64, _>("total_minor").unwrap(), 34_240);
        assert_eq!(invoice.try_get::<String, _>("status").unwrap(), "draft");
        assert_eq!(invoice.try_get::<String, _>("source_type").unwrap(), "standalone");
        assert_eq!(invoice.try_get::<String, _>("line_mode").unwrap(), "native_lines");
        assert_eq!(invoice.try_get::<i64, _>("line_revision").unwrap(), 1);
        assert_eq!(invoice.try_get::<String, _>("debit_account_id").unwrap(), "ar");
        assert_eq!(invoice.try_get::<String, _>("credit_account_id").unwrap(), "sales");
        assert_eq!(invoice.try_get::<String, _>("tax_account_id").unwrap(), "vat");

        let lines = sqlx::query(
            "SELECT ordinal,description,quantity_millis,unit_price_minor,tax_bps,subtotal_minor,tax_minor,total_minor,account_id,tax_account_id,origin_kind FROM finance_invoice_line_versions WHERE invoice_id=$1 AND source_version=1 ORDER BY ordinal",
        )
        .bind(&result.data.id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].try_get::<i32, _>("ordinal").unwrap(), 0);
        assert_eq!(lines[0].try_get::<i64, _>("quantity_millis").unwrap(), 2_000);
        assert_eq!(lines[0].try_get::<i64, _>("subtotal_minor").unwrap(), 20_000);
        assert_eq!(lines[0].try_get::<i64, _>("tax_minor").unwrap(), 1_400);
        assert_eq!(lines[1].try_get::<i64, _>("quantity_millis").unwrap(), 1_500);
        assert_eq!(lines[1].try_get::<i64, _>("subtotal_minor").unwrap(), 12_000);
        assert_eq!(lines[1].try_get::<i64, _>("tax_minor").unwrap(), 840);
        assert_eq!(lines[1].try_get::<String, _>("origin_kind").unwrap(), "native_line");

        let money = sqlx::query(
            "SELECT amount_units::text,usd_units::text,currency_scale,used_latest_fallback FROM money_transaction_snapshots WHERE company_id='company' AND source_type='finance_invoice' AND source_id=$1 AND amount_kind='draft:v1'",
        )
        .bind(&result.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(money.try_get::<String, _>("amount_units").unwrap(), "34240");
        assert_eq!(money.try_get::<i16, _>("currency_scale").unwrap(), 2);
        assert!(!money.try_get::<bool, _>("used_latest_fallback").unwrap());

        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ($1,$2) AND actor_id='fixture-a'",
        )
        .bind(key)
        .bind(format!("{key}-invoice"))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 2);
        let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id IN ($1,$2) AND action IN ('finance.invoice.create','finance.sales-invoice.prepare')",
        )
        .bind(key)
        .bind(format!("{key}-invoice"))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audits, 2);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id IN ($1,$2) AND event_type IN ('finance.invoice.create','finance.sales-invoice.prepare')",
        )
        .bind(key)
        .bind(format!("{key}-invoice"))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 2);

        // Exact replay returns the retained result without a second invoice.
        let replay = service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_selection(&p),
                CommandKey::new(key).unwrap(),
                input(),
            )
            .await
            .unwrap();
        assert_eq!(replay.data.id, result.data.id);
        let invoices: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_invoices WHERE company_id='company' AND source_type='standalone'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(invoices, 1);

        // A different request under the same key is an idempotency conflict.
        let mut changed = input();
        changed.completion_note = "A different completed service request".into();
        match service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_selection(&p),
                CommandKey::new(key).unwrap(),
                changed,
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
            }
            other => panic!("different request must conflict: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn sales_invoice_create_requires_defaults_company_scope_and_tax_account() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        // Branch scope is rejected before the writer.
        match service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                {
                    let mut request = selection(&p);
                    request.branch_slug = Some("branch".into());
                    move || Ok(request)
                },
                CommandKey::new("sales-scope-0001").unwrap(),
                input(),
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
        // Tax without an approved VAT account is rejected.
        sqlx::query("UPDATE finance_sales_defaults SET tax_account_id=NULL WHERE id='sales-defaults-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        match service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_selection(&p),
                CommandKey::new("sales-tax-0001").unwrap(),
                input(),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "TAX_ACCOUNT_REQUIRED");
            }
            other => panic!("tax without account must fail: {other:?}"),
        }
        // Missing approved defaults is a conflict.
        sqlx::query("UPDATE finance_sales_defaults SET status='superseded' WHERE id='sales-defaults-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        match service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_selection(&p),
                CommandKey::new("sales-defaults-0001").unwrap(),
                input(),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SALES_DEFAULTS_REQUIRED");
            }
            other => panic!("missing defaults must fail: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn sales_invoice_create_http_route_accepts_real_session_and_command_key() {
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
        let key = "sales-http-0001";
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/sales/invoices")
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
            .body(Body::from(serde_json::to_vec(&input()).unwrap()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        assert_eq!(value["data"]["id"], format!("invoice-{key}-invoice"));
        assert_eq!(value["data"]["number"], "INV-2026-000001");
        assert_eq!(value["data"]["total"], "342.40");

        let missing = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/sales/invoices")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(serde_json::to_vec(&input()).unwrap()))
            .unwrap();
        assert_eq!(
            app.oneshot(missing).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    })
    .await;
}
