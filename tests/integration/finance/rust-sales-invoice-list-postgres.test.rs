//! Actual native Finance sales-invoice list read over leased PostgreSQL, current
//! Account/IAM authority, the rendered-context CAS and the original Commerce
//! branch/department fulfillment lineage.
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
use erpx_server_finance::public::sales_invoice_list::SalesInvoiceListQuery;
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

fn query(tab: &str, q: &str, status: &str, currency: Option<&str>, queue: Option<&str>) -> SalesInvoiceListQuery {
    SalesInvoiceListQuery {
        tab: tab.into(),
        q: q.into(),
        status: status.into(),
        start_date: None,
        end_date: None,
        cursor: None,
        currency: currency.map(Into::into),
        queue: queue.map(Into::into),
    }
}

fn selection(p: &Prepared) -> HoldingWorkspaceRequest {
    p.reviewer_selection.clone()
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES\
         ('customer-a','company','customer','CUS-A','Acme Customer','THB',0,0),\
         ('customer-b','company','customer','CUS-B','Second Customer','THB',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,base_total_minor,base_outstanding_minor)VALUES\
         ('inv-overdue','company','customer-a','receivable','INV-OVERDUE','2026-01-01','2026-01-01','THB',5000,0,5000,5000,'cash','sales','posted',1,'fixture-a',100,100,5000,5000),\
         ('inv-current','company','customer-b','receivable','INV-CURRENT','2026-01-02','2030-01-01','THB',3000,0,3000,3000,'cash','sales','posted',1,'fixture-a',200,200,3000,3000),\
         ('inv-paid','company','customer-a','receivable','INV-PAID','2026-01-03','2030-01-01','THB',1000,0,1000,0,'cash','sales','paid',1,'fixture-a',300,300,1000,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at)VALUES\
         ('journal-receipt','company','chart','period','J-RECEIPT','2026-01-04','THB','manual','Receipt journal','posted',1,'fixture-a','fixture-b',1,400,400) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,journal_id,version,created_by,created_at,updated_at)VALUES\
         ('payment-1','company','customer-a','receipt','PAY-1','2026-01-04','THB',2000,0,'settled','journal-receipt',1,'fixture-a',400,400) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_pages(pool: &DatabasePool) {
    for index in 0..31 {
        let id = format!("inv-page-{index}");
        let number = format!("INV-PAGE-{index:02}");
        let created = 1_000 + index;
        sqlx::query("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,base_total_minor,base_outstanding_minor)VALUES($1,'company','customer-a','receivable',$2,'2026-01-05','2030-01-01','THB',100,0,100,100,'cash','sales','posted',1,'fixture-a',$3,$3,100,100) ON CONFLICT DO NOTHING")
            .bind(&id)
            .bind(&number)
            .bind(created)
            .execute(pool.raw())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn sales_invoice_list_reads_owner_facts_and_filters() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        let result = service(&p)
            .sales_invoice_list(
                "acme.example.test",
                p.reviewer.clone(),
                {
                    let sel = selection(&p);
                    move || Ok(sel.clone())
                },
                query("invoices", "", "", None, None),
            )
            .await
            .unwrap();
        assert_eq!(result.context.company_id, "company");
        assert_eq!(result.context.user_id, "fixture-b");
        assert_eq!(result.data.query.tab, "invoices");
        assert_eq!(result.data.summary.len(), 1);
        assert_eq!(result.data.summary[0]["currency"], "THB");
        assert_eq!(result.data.summary[0]["awaiting_minor"], 8_000);
        assert_eq!(result.data.summary[0]["overdue_minor"], 5_000);
        assert_eq!(result.data.received.len(), 1);
        assert_eq!(result.data.received[0]["received_minor"], 2_000);
        assert_eq!(result.data.page.items.len(), 3);
        assert_eq!(result.data.page.items[0]["id"], "inv-paid");
        assert_eq!(result.data.page.items[0]["party_name"], "Acme Customer");
        assert_eq!(result.data.page.items[1]["number"], "INV-CURRENT");
        assert!(result.data.page.next_cursor.is_none());

        let posted = service(&p)
            .sales_invoice_list(
                "acme.example.test",
                p.reviewer.clone(),
                {
                    let sel = selection(&p);
                    move || Ok(sel.clone())
                },
                query("invoices", "", "posted", None, None),
            )
            .await
            .unwrap();
        assert_eq!(posted.data.page.items.len(), 2);
        assert!(posted
            .data
            .page
            .items
            .iter()
            .all(|row| row["status"] == "posted"));

        let overdue = service(&p)
            .sales_invoice_list(
                "acme.example.test",
                p.reviewer.clone(),
                {
                    let sel = selection(&p);
                    move || Ok(sel.clone())
                },
                query("invoices", "", "", None, Some("overdue")),
            )
            .await
            .unwrap();
        assert_eq!(overdue.data.page.items.len(), 1);
        assert_eq!(overdue.data.page.items[0]["id"], "inv-overdue");

        let searched = service(&p)
            .sales_invoice_list(
                "acme.example.test",
                p.reviewer.clone(),
                {
                    let sel = selection(&p);
                    move || Ok(sel.clone())
                },
                query("invoices", "current", "", None, None),
            )
            .await
            .unwrap();
        assert_eq!(searched.data.page.items.len(), 1);
        assert_eq!(searched.data.page.items[0]["id"], "inv-current");

        // Company isolation and branch scope are resolved by the owner.
        let mut tx = p.system.holdings[0].raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let foreign = erpx_server_finance::public::sales_invoice_list::read_sales_invoice_list(
            &mut tx,
            "another-company",
            None,
            None,
            &query("invoices", "", "", None, None),
        )
        .await
        .unwrap();
        assert!(foreign.page.items.is_empty());
        let scoped = erpx_server_finance::public::sales_invoice_list::read_sales_invoice_list(
            &mut tx,
            "company",
            Some("branch"),
            None,
            &query("invoices", "", "", None, None),
        )
        .await
        .unwrap();
        assert!(scoped.page.items.is_empty());
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn sales_invoice_list_paginates_and_rejects_a_stale_context() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        seed_pages(&p.system.holdings[0]).await;
        let first = service(&p)
            .sales_invoice_list(
                "acme.example.test",
                p.reviewer.clone(),
                {
                    let sel = selection(&p);
                    move || Ok(sel.clone())
                },
                query("invoices", "", "", None, None),
            )
            .await
            .unwrap();
        assert_eq!(first.data.page.items.len(), 30);
        let cursor = first.data.page.next_cursor.clone().unwrap();
        assert!(first.data.page.has_more);
        let mut next = query("invoices", "", "", None, None);
        next.cursor = Some(cursor);
        let second = service(&p)
            .sales_invoice_list(
                "acme.example.test",
                p.reviewer.clone(),
                {
                    let sel = selection(&p);
                    move || Ok(sel.clone())
                },
                next,
            )
            .await
            .unwrap();
        assert_eq!(second.data.page.items.len(), 4);
        assert!(second.data.page.next_cursor.is_none());

        match service(&p)
            .sales_invoice_list(
                "acme.example.test",
                p.reviewer.clone(),
                {
                    let mut request = selection(&p);
                    request.expected = Some(
                        erpx_server_identity_access::public::work_context::ExpectedWorkSelection {
                            revision: 99,
                            assignment_id: erpx_server_shared::public::AssignmentId::new(
                                "finance-fixture-b",
                            )
                            .unwrap(),
                            assignment_version: 3,
                        },
                    );
                    move || Ok(request.clone())
                },
                query("invoices", "", "", None, None),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert!(matches!(error.kind, erpx_server_shared::public::FailureKind::Conflict));
            }
            other => panic!("stale context must fail: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn sales_invoice_list_http_route_accepts_real_session_and_rendered_context() {
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
            .uri("/api/v1/holdings/acme/finance/sales/invoices?tab=invoices&status=posted")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.reviewer_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-b")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["context"]["companyId"], "company");
        assert_eq!(value["data"]["query"]["tab"], "invoices");
        assert_eq!(value["data"]["page"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(value["data"]["summary"][0]["awaiting_minor"], 8_000);

        let invalid = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/sales/invoices?tab=receipts")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.reviewer_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-b")
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
