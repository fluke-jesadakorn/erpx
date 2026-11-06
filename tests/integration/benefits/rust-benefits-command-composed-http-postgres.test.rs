//! Composed native Benefits command HTTP over the real Account session, current
//! IAM, same-unit Commercial History and PostgreSQL.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_benefits::public::command_http::{
    BenefitsCommandHttpTarget, benefits_command_http_router,
};
use erpx_server_shared::public::Clock;
use erpx_service::benefits::{
    BenefitsBackend, BenefitsConfiguration, BenefitsService,
};
use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
use std::sync::Arc;
use tower::ServiceExt;

fn cookie(p: &review::Prepared) -> String {
    format!("__Host-erpx-product={}", p.maker_token.expose())
}

fn request(p: &review::Prepared, body: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/holdings/acme/benefits")
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

async fn grant_iam_only(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"records.read\",\"records.write\",\"benefits.review\",\"module.benefits.read\"]','[]','[\"holding\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';\
         UPDATE assignments SET modules_json='[\"finance\",\"benefits\"]',version=4 WHERE id='finance-fixture-a';\
         UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a'",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

async fn grant_benefits_packages(p: &review::Prepared) {
    grant_iam_only(p).await;
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) \
         VALUES('company','people-benefits',1,'active',0,{expires},1,'Synthetic reviewed benefits purchase','public synthetic evidence',{now}),\
               ('company','people',1,'active',0,{expires},1,'Synthetic reviewed people dependency','public synthetic evidence',{now});\
         INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
         SELECT 'benefits-grant-'||package_id,company_id,package_id,'fixture-a','synthetic-request',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),{now} \
         FROM company_package_grants WHERE company_id='company' AND package_id IN ('people-benefits','people')"
    )))
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn router(p: &review::Prepared) -> axum::Router {
    let service = BenefitsService::new(
        p.authorizer.clone(),
        BenefitsConfiguration::new(
            erpx_server_shared::public::HoldingId::new("h001").unwrap(),
            erpx_server_shared::public::CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
    let backend = BenefitsBackend::new(
        Arc::new(review::account::fixture::account::policy()),
        Arc::new(FieldServiceReadScopeHeaders),
        Arc::new(service),
    );
    benefits_command_http_router(vec![BenefitsCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(backend),
    }])
    .unwrap()
}

const PLAN: &str = r#"{"entity":"plan","name":"Health Plus","provider":"Synthetic Provider","coverageType":"family","currency":"thb","employeeDeduction":"125.50","employerCost":"300.00","effectiveFrom":"2026-09-01","effectiveTo":null}"#;

#[tokio::test]
async fn benefits_command_http_persists_a_plan_and_replays_once() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_benefits_packages(&p).await;
        let app = router(&p);

        let created = app
            .clone()
            .oneshot(request(&p, PLAN, "benefits-http-plan-1"))
            .await
            .unwrap();
        let status = created.status();
        let created = json(created).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        assert_eq!(created["data"]["status"], "draft");
        assert_eq!(created["data"]["version"], 1);

        let id = created["data"]["id"].as_str().unwrap().to_owned();
        let persisted: (String, i64, String) = sqlx::query_as(
            "SELECT status,version,name FROM benefit_plans WHERE id=$1 AND company_id='company'",
        )
        .bind(&id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(persisted.0, "draft");
        assert_eq!(persisted.1, 1);
        assert_eq!(persisted.2, "Health Plus");

        let replay = app
            .clone()
            .oneshot(request(&p, PLAN, "benefits-http-plan-1"))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["id"], id);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM benefit_plans WHERE company_id='company'")
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}

#[tokio::test]
async fn benefits_command_http_denies_without_a_current_purchase() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_iam_only(&p).await;
        let app = router(&p);
        let denied = app
            .oneshot(request(&p, PLAN, "benefits-http-plan-denied"))
            .await
            .unwrap();
        let status = denied.status();
        let denied = json(denied).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
        assert!(
            ["PURCHASE_REQUIRED", "DEPENDENCY_REQUIRED", "PACKAGE_REQUIRED", "PACKAGE_DRAINING"]
                .contains(&denied["error"]["code"].as_str().unwrap_or("")),
            "{denied}"
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM benefit_plans WHERE company_id='company'")
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}
