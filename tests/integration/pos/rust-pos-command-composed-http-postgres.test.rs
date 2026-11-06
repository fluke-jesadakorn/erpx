//! Composed native POS command HTTP over the real Account session/IAM and
//! PostgreSQL. This case proves a retail sale create with the real Organization
//! issuer and Finance period source; journal-posting commands remain gated.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_pos::public::commands_http::{
    PosCommandHttpTarget, pos_command_router,
};
use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
use erpx_service::pos::{PosBackend, PosConfiguration, PosService};
use std::sync::Arc;
use tower::ServiceExt;

fn cookie(p: &review::Prepared) -> String {
    format!("__Host-erpx-product={}", p.maker_token.expose())
}

fn request(p: &review::Prepared, body: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/holdings/acme/pos/commands/sale.create")
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie(p))
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", "finance-fixture-a")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "4")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

async fn grant_pos(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"records.read\",\"pos.sell\",\"pos.manage\",\"module.pos.read\"]','[]','[\"holding\",\"branch\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';\
         UPDATE assignments SET modules_json='[\"finance\",\"pos\"]',version=4 WHERE id='finance-fixture-a';\
         UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a'",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

async fn seed_pos(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('branch','branch','branch','Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) \
         VALUES('warehouse','company','branch','W1','Retail warehouse') ON CONFLICT DO NOTHING;\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) \
         VALUES('chart','company','Chart','THB','fixture-a',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) \
         VALUES('period','company','2027','2027-01-01','2027-12-31',0) ON CONFLICT DO NOTHING;\
         INSERT INTO pos_settings(company_id,enabled,currency,accounts_json,accounting_prepared_by,accounting_approved_by,version) \
         VALUES('company',true,'THB','{\"chartId\":\"chart\",\"cash\":\"cash\",\"cashTransfer\":\"transfer\",\"cashDifference\":\"difference\"}','fixture-a','fixture-b',1) ON CONFLICT DO NOTHING;\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) \
         VALUES('register','company','branch','warehouse','Retail','retail',true,1) ON CONFLICT DO NOTHING;\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version,business_date) \
         VALUES('shift','register','company','branch','fixture-a',1800000000000,100000,'open',1,'2027-01-15') ON CONFLICT DO NOTHING",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn router(p: &review::Prepared) -> axum::Router {
    let service = PosService::new(
        p.authorizer.clone(),
        PosConfiguration::new(
            erpx_server_shared::public::HoldingId::new("h001").unwrap(),
            erpx_server_shared::public::CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
    let backend = PosBackend::new(
        Arc::new(review::account::fixture::account::policy()),
        Arc::new(FieldServiceReadScopeHeaders),
        Arc::new(service),
    );
    pos_command_router(vec![PosCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(backend),
    }])
    .unwrap()
}

#[tokio::test]
async fn pos_http_creates_a_retail_sale_with_real_organization_and_finance_sources() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_pos(&p).await;
        seed_pos(&p).await;
        let app = router(&p);

        let created = app
            .clone()
            .oneshot(request(
                &p,
                r#"{"shiftId":"shift","tableId":null,"guests":1,"partyId":null,"serviceMode":"retail","currency":"THB"}"#,
                "pos-http-sale-1",
            ))
            .await
            .unwrap();
        let status = created.status();
        let created = json(created).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let id = created["data"]["id"].as_str().unwrap().to_owned();

        let sale: (String, String, i64, String, String) = sqlx::query_as(
            "SELECT state,currency,version,number,issuer_json FROM pos_sales WHERE id=$1 AND company_id='company'",
        )
        .bind(&id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(sale.0, "open");
        assert_eq!(sale.1, "THB");
        assert_eq!(sale.2, 1);
        assert!(sale.3.starts_with("POS-20270115-"), "{}", sale.3);
        assert!(sale.4.contains("Synthetic Acme"), "{}", sale.4);

        let replay = app
            .clone()
            .oneshot(request(
                &p,
                r#"{"shiftId":"shift","tableId":null,"guests":1,"partyId":null,"serviceMode":"retail","currency":"THB"}"#,
                "pos-http-sale-1",
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["id"], id);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_sales WHERE company_id='company'")
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}
