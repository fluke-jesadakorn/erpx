//! Actual native Finance invoice list and historical aging over leased
//! PostgreSQL, current Account/IAM authority and the dated accounting-evidence
//! CTE.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_finance::public::finance_invoice_read::InvoicePageQuery;
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

fn query(kind: &str, status: &str, queue: &str, q: &str, chart: Option<&str>, cursor: Option<String>) -> InvoicePageQuery {
    InvoicePageQuery {
        q: q.into(),
        kind: kind.into(),
        status: status.into(),
        queue: queue.into(),
        chart_id: chart.map(Into::into),
        cursor,
    }
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('ar','company','chart','1100','Trade receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('vat','company','chart','2200','Output VAT','liability','THB',0,'none','active',1,0,0) ON CONFLICT DO NOTHING;\
         UPDATE finance_accounts SET currency='THB' WHERE company_id='company' AND id IN ('sales','book-sales','cash','book-cash');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('aging-customer','company','customer','AGE-C','Aging customer','THB','active',1,0,0),\
         ('aging-supplier','company','supplier','AGE-S','Aging supplier','THB','active',1,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at)VALUES\
         ('aging-j-current','company','chart','period','J-AGE-CURRENT','2026-10-01','THB','manual','Current','posted',1,'fixture-a','fixture-b',0,0,0),\
         ('aging-j-45','company','chart','period','J-AGE-45','2026-08-01','THB','manual','45 days','posted',1,'fixture-a','fixture-b',0,0,0),\
         ('aging-j-100','company','chart','period','J-AGE-100','2026-06-01','THB','manual','100 days','posted',1,'fixture-a','fixture-b',0,0,0),\
         ('aging-j-payment','company','chart','period','J-AGE-PAYMENT','2026-06-15','THB','manual','Payment','posted',1,'fixture-a','fixture-b',0,0,0),\
         ('aging-j-payable','company','chart','period','J-AGE-PAYABLE','2026-08-01','THB','manual','Payable','posted',1,'fixture-a','fixture-b',0,0,0),\
         ('aging-j-book','company','book-chart','period','J-AGE-BOOK','2026-08-01','THB','manual','Other chart','posted',1,'fixture-a','fixture-b',0,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,base_total_minor,base_outstanding_minor,journal_id,fx_rate_micros)VALUES\
         ('aging-current','company','aging-customer','receivable','INV-AGE-CURRENT','2026-10-01','2026-11-10','THB',10000,0,10000,10000,'ar','sales','posted',1,'fixture-a',0,0,10000,10000,'aging-j-current',1000000),\
         ('aging-45','company','aging-customer','receivable','INV-AGE-45','2026-08-01','2026-09-20','THB',20000,0,20000,20000,'ar','sales','posted',1,'fixture-a',0,0,20000,20000,'aging-j-45',1000000),\
         ('aging-100','company','aging-customer','receivable','INV-AGE-100','2026-06-01','2026-07-27','THB',30000,0,30000,25000,'ar','sales','posted',1,'fixture-a',0,0,30000,25000,'aging-j-100',1000000),\
         ('aging-draft','company','aging-customer','receivable','INV-AGE-DRAFT','2026-09-01','2026-09-01','THB',9000,0,9000,9000,'ar','sales','draft',1,'fixture-a',0,0,9000,9000,NULL,1000000),\
         ('aging-payable','company','aging-supplier','payable','BILL-AGE-45','2026-08-01','2026-09-20','THB',15000,0,15000,15000,'sales','ar','posted',1,'fixture-a',0,0,15000,15000,'aging-j-payable',1000000),\
         ('aging-book','company','aging-customer','receivable','INV-AGE-BOOK','2026-08-01','2026-09-20','THB',5000,0,5000,5000,'book-cash','book-sales','posted',1,'fixture-a',0,0,5000,5000,'aging-j-book',1000000) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,journal_id,version,created_by,created_at,updated_at)VALUES\
         ('aging-payment','company','aging-customer','receipt','RCPT-AGE-1','2026-06-15','THB',5000,0,'settled','aging-j-payment',1,'fixture-a',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor,created_by,created_at)VALUES\
         ('aging-allocation','company','aging-payment','aging-100',5000,5000,5000,0,'fixture-a',0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_page(pool: &DatabasePool) {
    for index in 0..31 {
        let id = format!("aging-page-{index}");
        let number = format!("INV-AGE-PAGE-{index:02}");
        let created = 1_000 + index;
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,base_total_minor,base_outstanding_minor,journal_id,fx_rate_micros)VALUES($1,'company','aging-customer','receivable',$2,'2026-10-02','2026-12-31','THB',100,0,100,100,'ar','sales','posted',1,'fixture-a',$3,$3,100,100,'aging-j-current',1000000) ON CONFLICT DO NOTHING",
        )
        .bind(&id)
        .bind(&number)
        .bind(created)
        .execute(pool.raw())
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn invoice_page_reads_filters_and_paginates() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let all = service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                query("", "", "", "", None, None),
            )
            .await
            .unwrap();
        assert_eq!(all.data.items.len(), 6);
        assert!(all
            .data
            .items
            .iter()
            .any(|row| row["invoice_number"] == "INV-AGE-CURRENT"));
        assert!(all
            .data
            .items
            .iter()
            .all(|row| row.get("party_name").and_then(|value| value.as_str()).is_some()));

        let receivable = service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                query("receivable", "", "", "", None, None),
            )
            .await
            .unwrap();
        assert_eq!(receivable.data.items.len(), 5);
        assert!(receivable
            .data
            .items
            .iter()
            .all(|row| row["kind"] == "receivable"));

        let open = service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                query("", "", "open", "", None, None),
            )
            .await
            .unwrap();
        assert_eq!(open.data.items.len(), 5);
        assert!(open
            .data
            .items
            .iter()
            .all(|row| row["status"] == "posted" || row["status"] == "partially_paid"));

        let searched = service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                query("", "", "", "AGE-45", None, None),
            )
            .await
            .unwrap();
        assert_eq!(searched.data.items.len(), 2);
        assert!(searched
            .data
            .items
            .iter()
            .all(|row| row["invoice_number"].as_str().unwrap().contains("AGE-45")));

        let chart = service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                query("", "", "", "", Some("book-chart"), None),
            )
            .await
            .unwrap();
        assert_eq!(chart.data.items.len(), 1);
        assert_eq!(chart.data.items[0]["id"], "aging-book");

        // 31 extra rows plus the seeded rows paginate 30 at a time.
        seed_page(pool).await;
        let first = service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                query("", "", "", "PAGE", None, None),
            )
            .await
            .unwrap();
        assert_eq!(first.data.items.len(), 30);
        let cursor = first.data.next_cursor.clone().unwrap();
        let second = service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                query("", "", "", "PAGE", None, Some(cursor)),
            )
            .await
            .unwrap();
        assert_eq!(second.data.items.len(), 1);
        assert!(second.data.next_cursor.is_none());

        // A cursor is bound to its query scope.
        match service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                query("", "", "", "DIFFERENT", None, first.data.next_cursor),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "INVALID_CURSOR");
            }
            other => panic!("foreign cursor must fail: {other:?}"),
        }

        // Company isolation is enforced by the owner query.
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let foreign = erpx_server_finance::public::finance_invoice_read::read_invoice_page(
            &mut tx,
            "another-company",
            &query("", "", "", "", None, None),
        )
        .await
        .unwrap();
        assert!(foreign.items.is_empty());
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn invoice_aging_reads_historical_buckets_and_scope() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let receivable = service(&p)
            .finance_invoice_aging(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                "receivable".into(),
                "2026-11-04".into(),
                Some("chart".into()),
            )
            .await
            .unwrap();
        let rows = &receivable.data;
        assert_eq!(rows.len(), 3);
        let by_id = |id: &str| rows.iter().find(|row| row["id"] == id).unwrap();
        assert_eq!(by_id("aging-current")["bucket"], "current");
        assert_eq!(by_id("aging-current")["outstanding_minor"], 10_000);
        assert_eq!(by_id("aging-45")["bucket"], "31-60");
        assert_eq!(by_id("aging-45")["outstanding_minor"], 20_000);
        assert_eq!(by_id("aging-100")["bucket"], "90+");
        assert_eq!(by_id("aging-100")["outstanding_minor"], 25_000);
        assert_eq!(by_id("aging-100")["legacy_date_evidence"], false);
        assert_eq!(by_id("aging-100")["as_of"], "2026-11-04");
        assert!(rows.iter().all(|row| row["id"] != "aging-draft"));
        assert!(rows.iter().all(|row| row["id"] != "aging-book"));

        let payable = service(&p)
            .finance_invoice_aging(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                "payable".into(),
                "2026-11-04".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(payable.data.len(), 1);
        assert_eq!(payable.data[0]["id"], "aging-payable");
        assert_eq!(payable.data[0]["bucket"], "31-60");
        assert_eq!(payable.data[0]["outstanding_minor"], 15_000);

        let all_charts = service(&p)
            .finance_invoice_aging(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                "receivable".into(),
                "2026-11-04".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(all_charts.data.len(), 4);
        assert!(all_charts.data.iter().any(|row| row["id"] == "aging-book"));

        match service(&p)
            .finance_invoice_aging(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                "receivable".into(),
                "2026-02-30".into(),
                None,
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "FINANCE_INVOICE_AGING_QUERY_INVALID");
            }
            other => panic!("invalid date must fail: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn invoice_page_and_aging_http_routes_accept_real_session() {
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
            .uri("/api/v1/holdings/acme/finance/invoices?kind=receivable&queue=open")
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
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["data"]["items"].as_array().unwrap().len(), 4);

        let aging = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/invoices/aging?kind=receivable&asOf=2026-11-04&chartId=chart")
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
        let response = app.clone().oneshot(aging).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"].as_array().unwrap().len(), 3);

        let invalid = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/invoices/aging?kind=other&asOf=2026-11-04")
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
        assert_eq!(
            app.oneshot(invalid).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    })
    .await;
}
