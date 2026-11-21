//! Actual native standalone sales-invoice edit/void over leased PostgreSQL,
//! current Account/IAM authority and the caller's command receipts.
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
use erpx_server_finance::public::sales_invoice_edit::{
    SalesInvoiceEditInput, SalesInvoiceEditLine, SalesInvoiceVoidInput,
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
         ('ar','company','chart','1100','Trade receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('vat','company','chart','2200','Output VAT','liability','THB',0,'none','active',1,0,0) ON CONFLICT DO NOTHING;\
         UPDATE finance_accounts SET currency='THB' WHERE company_id='company' AND id='sales';\
         INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id,status,reason,created_by,reviewed_by,review_reason,version,created_at,updated_at)VALUES\
         ('sales-defaults-1','company','chart','THB','ar','sales','vat','approved','Approved sales accounts for native invoice editing','fixture-a','fixture-b','Independent review of the sales accounts',1,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('sales-customer','company','customer','SAL-C','Native sales customer','THB','active',1,0,0),\
         ('sales-customer-2','company','customer','SAL-D','Second sales customer','THB','active',1,0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn create_input() -> SalesInvoiceCreateInput {
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

fn edit_input() -> SalesInvoiceEditInput {
    SalesInvoiceEditInput {
        kind: "receivable".into(),
        party_id: "sales-customer-2".into(),
        invoice_number: "INV-2026-000001".into(),
        invoice_date: "2026-01-03".into(),
        due_date: "2026-02-02".into(),
        currency: "THB".into(),
        subtotal: "250.00".into(),
        tax: "17.50".into(),
        debit_account_id: "ar".into(),
        credit_account_id: "sales".into(),
        tax_account_id: Some("vat".into()),
        expected_version: 1,
        lines: Some(vec![
            SalesInvoiceEditLine {
                description: "Revised installation".into(),
                quantity: "1".into(),
                unit_price: "200.00".into(),
                tax_bps: 700,
                account_id: "sales".into(),
                tax_account_id: None,
            },
            SalesInvoiceEditLine {
                description: "Follow-up visit".into(),
                quantity: "0.5".into(),
                unit_price: "100.00".into(),
                tax_bps: 700,
                account_id: "sales".into(),
                tax_account_id: None,
            },
        ]),
    }
}

#[tokio::test]
async fn sales_invoice_edit_writes_revision_money_and_receipt() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let created = service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-create-0001").unwrap(),
                create_input(),
            )
            .await
            .unwrap();
        assert_eq!(created.data.id, "invoice-sales-create-0001-invoice");
        assert_eq!(created.data.version, 1);
        {
            use sqlx::Row;
            let row = sqlx::query("SELECT version,status,kind,source_type,line_mode FROM finance_invoices WHERE id=$1")
                .bind(&created.data.id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            eprintln!("DEBUG_INVOICE version={:?} status={:?} kind={:?} source={:?} line_mode={:?}",
                row.try_get::<i64,_>("version").ok(), row.try_get::<String,_>("status").ok(),
                row.try_get::<String,_>("kind").ok(), row.try_get::<Option<String>,_>("source_type").ok(),
                row.try_get::<String,_>("line_mode").ok());
        }

        let key = "sales-edit-0001";
        let edited = service(&p)
            .edit_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                created.data.id.clone(),
                edit_input(),
            )
            .await
            .unwrap()
            .expect("native sales invoice is editable");
        assert_eq!(edited.data.id, created.data.id);
        assert_eq!(edited.data.total, "267.50");
        assert_eq!(edited.data.outstanding, "267.50");
        assert_eq!(edited.data.status, "draft");
        assert_eq!(edited.data.version, 2);

        let invoice = sqlx::query(
            "SELECT party_id,invoice_number,invoice_date,due_date,subtotal_minor,tax_minor,\
             total_minor,outstanding_minor,status,version,line_mode,line_revision \
             FROM finance_invoices WHERE id=$1 AND company_id='company'",
        )
        .bind(&created.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(invoice.try_get::<String, _>("party_id").unwrap(), "sales-customer-2");
        assert_eq!(invoice.try_get::<String, _>("invoice_number").unwrap(), "INV-2026-000001");
        assert_eq!(invoice.try_get::<String, _>("invoice_date").unwrap(), "2026-01-03");
        assert_eq!(invoice.try_get::<i64, _>("subtotal_minor").unwrap(), 25_000);
        assert_eq!(invoice.try_get::<i64, _>("tax_minor").unwrap(), 1_750);
        assert_eq!(invoice.try_get::<i64, _>("total_minor").unwrap(), 26_750);
        assert_eq!(invoice.try_get::<i64, _>("version").unwrap(), 2);
        assert_eq!(invoice.try_get::<String, _>("line_mode").unwrap(), "native_lines");
        assert_eq!(invoice.try_get::<i64, _>("line_revision").unwrap(), 2);

        let lines = sqlx::query(
            "SELECT source_version,ordinal,description,quantity_millis,subtotal_minor,tax_minor,\
             origin_kind FROM finance_invoice_line_versions WHERE invoice_id=$1 \
             ORDER BY source_version,ordinal",
        )
        .bind(&created.data.id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[2].try_get::<i64, _>("source_version").unwrap(), 2);
        assert_eq!(lines[2].try_get::<String, _>("description").unwrap(), "Revised installation");
        assert_eq!(lines[2].try_get::<i64, _>("quantity_millis").unwrap(), 1_000);
        assert_eq!(lines[2].try_get::<i64, _>("subtotal_minor").unwrap(), 20_000);
        assert_eq!(lines[3].try_get::<String, _>("description").unwrap(), "Follow-up visit");
        assert_eq!(lines[3].try_get::<i64, _>("quantity_millis").unwrap(), 500);
        assert_eq!(lines[3].try_get::<i64, _>("tax_minor").unwrap(), 350);

        let money = sqlx::query(
            "SELECT amount_units::text,usd_units::text,currency_scale \
             FROM money_transaction_snapshots WHERE company_id='company' \
             AND source_type='finance_invoice' AND source_id=$1 AND amount_kind='draft:v2'",
        )
        .bind(&created.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(money.try_get::<String, _>("amount_units").unwrap(), "26750");
        assert_eq!(money.try_get::<i16, _>("currency_scale").unwrap(), 2);
        let old_money: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM money_transaction_snapshots WHERE source_id=$1 \
             AND amount_kind='draft:v1'",
        )
        .bind(&created.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(old_money, 1);

        let receipt = sqlx::query(
            "SELECT operation,record_id FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt.try_get::<String, _>("operation").unwrap(), "finance.invoice.update");
        assert_eq!(receipt.try_get::<String, _>("record_id").unwrap(), created.data.id);
        let audit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id=$1 AND action='finance.invoice.update'",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, 1);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id=$1 AND event_type='finance.invoice.update'",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 1);

        // Exact replay returns the retained result and does not add a revision.
        let replay = service(&p)
            .edit_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                created.data.id.clone(),
                edit_input(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(replay.data.version, 2);
        let revisions: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT source_version) FROM finance_invoice_line_versions WHERE invoice_id=$1",
        )
        .bind(&created.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(revisions, 2);

        // A different request under the same key is an idempotency conflict.
        let mut changed = edit_input();
        changed.due_date = "2026-02-03".into();
        match service(&p)
            .edit_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                created.data.id.clone(),
                changed,
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
            }
            other => panic!("different request must conflict: {other:?}"),
        }

        // A stale expected version is a conflict.
        match service(&p)
            .edit_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-edit-stale-0001").unwrap(),
                created.data.id.clone(),
                edit_input(),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "VERSION_CONFLICT");
            }
            other => panic!("stale version must conflict: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn sales_invoice_void_writes_terminal_state_and_rejects_stale_versions() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let created = service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-void-create-0001").unwrap(),
                create_input(),
            )
            .await
            .unwrap();
        let key = "sales-void-0001";
        let voided = service(&p)
            .void_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                created.data.id.clone(),
                SalesInvoiceVoidInput {
                    expected_version: 1,
                },
            )
            .await
            .unwrap()
            .expect("native sales invoice is voidable");
        assert_eq!(voided.data.id, created.data.id);
        assert_eq!(voided.data.status, "voided");
        assert_eq!(voided.data.version, 2);

        let invoice = sqlx::query(
            "SELECT status,outstanding_minor,version FROM finance_invoices WHERE id=$1",
        )
        .bind(&created.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(invoice.try_get::<String, _>("status").unwrap(), "voided");
        assert_eq!(invoice.try_get::<i64, _>("outstanding_minor").unwrap(), 0);
        assert_eq!(invoice.try_get::<i64, _>("version").unwrap(), 2);

        let receipt = sqlx::query(
            "SELECT operation FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt.try_get::<String, _>("operation").unwrap(), "finance.invoice.void");

        let replay = service(&p)
            .void_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                created.data.id.clone(),
                SalesInvoiceVoidInput {
                    expected_version: 1,
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(replay.data.version, 2);

        match service(&p)
            .void_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-void-stale-0001").unwrap(),
                created.data.id.clone(),
                SalesInvoiceVoidInput {
                    expected_version: 1,
                },
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "VERSION_CONFLICT");
            }
            other => panic!("stale void must conflict: {other:?}"),
        }

        let unknown = service(&p)
            .void_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-void-unknown-0001").unwrap(),
                "invoice-missing".into(),
                SalesInvoiceVoidInput {
                    expected_version: 1,
                },
            )
            .await
            .unwrap();
        assert!(unknown.is_none());
    })
    .await;
}

#[tokio::test]
async fn sales_invoice_edit_supports_whole_document_and_rejects_other_owners() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        sqlx::raw_sql(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,base_total_minor,base_outstanding_minor,source_type,line_mode,line_revision)VALUES\
             ('invoice-whole-document','company','sales-customer','receivable','INV-WD-1','2026-01-02','2026-01-31','THB',10000,0,10000,10000,'ar','sales','draft',1,'fixture-a',0,0,0,0,'standalone','whole_document',1),\
             ('invoice-payable','company','sales-customer','payable','BILL-WD-1','2026-01-02','2026-01-31','THB',10000,0,10000,10000,'sales','ar','draft',1,'fixture-a',0,0,0,0,'standalone','whole_document',1) ON CONFLICT DO NOTHING;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let whole = service(&p)
            .edit_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-edit-whole-0001").unwrap(),
                "invoice-whole-document".into(),
                SalesInvoiceEditInput {
                    kind: "receivable".into(),
                    party_id: "sales-customer".into(),
                    invoice_number: "INV-WD-1".into(),
                    invoice_date: "2026-01-02".into(),
                    due_date: "2026-01-31".into(),
                    currency: "THB".into(),
                    subtotal: "120.00".into(),
                    tax: "0.00".into(),
                    debit_account_id: "ar".into(),
                    credit_account_id: "sales".into(),
                    tax_account_id: None,
                    expected_version: 1,
                    lines: None,
                },
            )
            .await
            .unwrap()
            .expect("whole-document invoice is editable");
        assert_eq!(whole.data.total, "120.00");
        assert_eq!(whole.data.version, 2);
        let row = sqlx::query(
            "SELECT description,quantity_millis,origin_kind,subtotal_minor \
             FROM finance_invoice_line_versions WHERE invoice_id='invoice-whole-document' \
             AND source_version=2",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(row.try_get::<String, _>("description").unwrap(), "Whole document INV-WD-1");
        assert!(row.try_get::<Option<i64>, _>("quantity_millis").unwrap().is_none());
        assert_eq!(row.try_get::<String, _>("origin_kind").unwrap(), "historical_header");

        let payable = service(&p)
            .edit_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-edit-payable-0001").unwrap(),
                "invoice-payable".into(),
                SalesInvoiceEditInput {
                    kind: "receivable".into(),
                    party_id: "sales-customer".into(),
                    invoice_number: "BILL-WD-1".into(),
                    invoice_date: "2026-01-02".into(),
                    due_date: "2026-01-31".into(),
                    currency: "THB".into(),
                    subtotal: "120.00".into(),
                    tax: "0.00".into(),
                    debit_account_id: "ar".into(),
                    credit_account_id: "sales".into(),
                    tax_account_id: None,
                    expected_version: 1,
                    lines: None,
                },
            )
            .await
            .unwrap();
        assert!(payable.is_none());
    })
    .await;
}

#[tokio::test]
async fn sales_invoice_edit_and_void_http_routes_accept_real_session_and_key() {
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
        let created = service(&p)
            .create_sales_invoice(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("sales-http-create-0001").unwrap(),
                create_input(),
            )
            .await
            .unwrap();
        let edit = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/holdings/acme/finance/invoices/{}",
                created.data.id
            ))
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", "sales-http-edit-0001")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(serde_json::to_vec(&edit_input()).unwrap()))
            .unwrap();
        let response = app.clone().oneshot(edit).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["data"]["version"], 2);
        assert_eq!(value["data"]["total"], "267.50");

        let void = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/holdings/acme/finance/invoices/{}/void",
                created.data.id
            ))
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", "sales-http-void-0001")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(r#"{"expectedVersion":2}"#))
            .unwrap();
        let response = app.clone().oneshot(void).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let missing = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/holdings/acme/finance/invoices/{}",
                created.data.id
            ))
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
            .body(Body::from(serde_json::to_vec(&edit_input()).unwrap()))
            .unwrap();
        assert_eq!(
            app.oneshot(missing).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    })
    .await;
}
