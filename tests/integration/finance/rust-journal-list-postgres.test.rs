//! Actual native Finance journal list read over leased PostgreSQL, current
//! Account/IAM authority, the rendered-context CAS, the Finance purchase and the
//! embedded initial-stock purchase filter.
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
    p.reviewer_selection.clone()
}

async fn mark_initial_stock(pool: &DatabasePool, journal: &str) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','finance','Finance branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('finance-wh','company','branch','FIN-WH','Finance warehouse','active',1) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,approved_by,journal_id,created_at,updated_at)VALUES('initial-count','company','branch','finance-wh','IC-1','2026-01-01','Initial physical stock','posted',1,0,'fixture-a','fixture-a','fixture-b',$1,0,0) ON CONFLICT DO NOTHING")
        .bind(journal)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)VALUES('initial-count-receipt','inventory.count.create','fixture-a','synthetic','initial-count',$1,0,$2) ON CONFLICT DO NOTHING")
        .bind(r#"{"_initialPhysicalStockCount":true}"#)
        .bind(r#"{"companyId":"company"}"#)
        .execute(pool.raw())
        .await
        .unwrap();
}

#[tokio::test]
async fn finance_journal_list_hides_unpurchased_initial_stock_and_filters_status() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        p.create("list-one", false).await.unwrap();
        p.create("list-opening", true).await.unwrap();

        let sel = selection(&p);
        let result = service(&p)
            .list(
                "acme.example.test",
                p.reviewer.clone(),
                move || Ok(sel.clone()),
                Some("chart".into()),
                "all".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.context.company_id, "company");
        assert_eq!(result.context.user_id, "fixture-b");
        assert!(!result.data.charts.is_empty());
        assert_eq!(result.data.chart["id"], "chart");
        assert!(!result.data.accounts.is_empty());
        assert!(!result.data.periods.is_empty());
        assert_eq!(result.data.journals.len(), 2);
        let ids: Vec<&str> = result
            .data
            .journals
            .iter()
            .filter_map(|row| row.get("id").and_then(serde_json::Value::as_str))
            .collect();
        assert!(ids.contains(&"journal-list-one"));
        assert!(ids.contains(&"journal-list-opening"));
        assert!(result.data.next_cursor.is_none());

        let sel = selection(&p);
        let drafts = service(&p)
            .list(
                "acme.example.test",
                p.reviewer.clone(),
                move || Ok(sel.clone()),
                Some("chart".into()),
                "draft".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(drafts.data.journals.len(), 2);
        let sel = selection(&p);
        let posted = service(&p)
            .list(
                "acme.example.test",
                p.reviewer.clone(),
                move || Ok(sel.clone()),
                Some("chart".into()),
                "posted".into(),
                None,
            )
            .await
            .unwrap();
        assert!(posted.data.journals.is_empty());
    })
    .await;
}

#[tokio::test]
async fn finance_journal_list_http_route_accepts_real_session_and_rendered_context() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        p.create("http-one", false).await.unwrap();
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
            .uri("/api/v1/holdings/acme/finance/journals?chart=chart&status=draft")
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
        assert_eq!(value["data"]["chart"]["id"], "chart");
        assert_eq!(value["data"]["journals"].as_array().unwrap().len(), 1);

        let stale = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/finance/journals?chart=chart")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.reviewer_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-b")
            .header("x-erpx-context-revision", "99")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.oneshot(stale).await.unwrap().status(), StatusCode::CONFLICT);
    })
    .await;
}
