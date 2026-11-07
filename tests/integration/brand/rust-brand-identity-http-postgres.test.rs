//! Native Brand HTTP ingress over the actual Account session and current IAM
//! authority. Exercises GET workspace, POST draft save and closed failures.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
#[path = "../../support/runtime/private-session-config.rs"]
mod private_fixture;
use erpx_server_identity_access::public::SystemAuthenticationSecrets;
use erpx_service::private_development_api::{DevelopmentApiConfiguration, assemble_development_api};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use erpx_service::brand_identity::{
    BrandIdentityConfiguration, BrandIdentityService,
};
use erpx_service::brand_identity_http::{BrandIdentityTarget, brand_identity_router};
use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
use erpx_server_identity_access::public::account_holding_bindings::{
    ConfiguredAccountHolding, ConfiguredHoldingRegistry,
};
use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
use std::sync::Arc;
use tower::ServiceExt;

fn owned_maker(
    p: &review::Prepared,
) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, erpx_server_shared::public::BusinessError>
       + Send
       + 'static {
    let request = p.maker_selection.clone();
    move || Ok(request)
}

async fn seed(p: &review::Prepared) {
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_brand_manager','Synthetic Brand manager',false,1,'active') ON CONFLICT DO NOTHING;\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_brand_manager',1,'[\"records.read\",\"organization.manage\",\"company.owner\",\"module.brand.read\"]','[]','[\"holding\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at)VALUES('brand-person-a','fixture-a','company','Synthetic Brand manager','TH','active',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at)VALUES('brand-engagement-a','brand-person-a','company','employee','salaried','active','2026-01-01',NULL,'Brand manager','THB',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','brand-person-a','active','local',1) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)\
         VALUES('brand-fixture-a','fixture-a','custom_brand_manager','[\"brand\"]','company',$1,$2,3,'holding')",
    )
    .bind(0_i64)
    .bind(9_007_199_254_740_991_i64)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE active_work_contexts SET assignment_id='brand-fixture-a',assignment_version=3,revision=7,expires_at=$1,updated_at=$2 WHERE user_id='fixture-a'",
    )
    .bind(expires)
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();
    let checks: (bool, bool, bool) = sqlx::query_as(
        "SELECT iam_assignment_active('brand-fixture-a',$1), \
         iam_principal_active('fixture-a','company',$1), \
         iam_role_allows('custom_brand_manager','company','organization.manage')",
    )
    .bind(now)
    .fetch_one(pool.raw())
    .await
    .unwrap();
    eprintln!("BRAND_AUTHORITY_CHECKS {checks:?}");
}

fn router(p: &review::Prepared) -> axum::Router {
    let service = BrandIdentityService::new(
        p.authorizer.clone(),
        BrandIdentityConfiguration::new(
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
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
    brand_identity_router(
        Arc::new(policy),
        Arc::new(registry),
        Arc::new(FieldServiceReadScopeHeaders),
        vec![BrandIdentityTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            service: Arc::new(service),
        }],
    )
    .unwrap()
}

fn cookie(p: &review::Prepared) -> String {
    format!("__Host-erpx-product={}", p.maker_token.expose())
}

#[tokio::test]
async fn brand_http_reads_and_saves_a_draft_under_normal_session_authority() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        seed(&p).await;
        let app = router(&p);
        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/brand")
            .header("host", "acme.example.test")
            .header("cookie", cookie(&p))
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "brand-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        if status != StatusCode::OK { panic!("brand read failed: {value}"); }
        assert_eq!(value["data"]["drafts"].as_array().unwrap().len(), 0);
        assert_eq!(value["data"]["initial"]["identity"]["shortName"], "Synthetic Acme");
        assert_eq!(value["context"]["companyId"], "company");

        let save = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/brand")
            .header("host", "acme.example.test")
            .header("cookie", cookie(&p))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "brand-http-save-1")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "brand-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r##"{"action":"save","expectedPublishedVersion":0,"identity":{"identity":{"shortName":"Acme HTTP","tagline":"Native brand"},"visual":{"accentColor":"#224488","logoPublicationId":null}},"reason":"Prepare the current storefront identity."}"##,
            ))
            .unwrap();
        let response = app.clone().oneshot(save).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        if status != StatusCode::OK { panic!("brand save failed: {value}"); }
        assert_eq!(value["data"]["status"], "draft");
        assert_eq!(value["data"]["version"], 1);
        let (status, short): (String, String) = sqlx::query_as(
            "SELECT d.status, d.identity_json::jsonb#>>'{identity,shortName}' \
             FROM brand_identity_drafts d WHERE d.id='brand-draft-brand-http-save-1' AND d.company_id='company'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!((status.as_str(), short.as_str()), ("draft", "Acme HTTP"));
        let (audit, outbox): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*)::bigint FROM audit_events WHERE command_id='brand-http-save-1'),\
             (SELECT count(*)::bigint FROM outbox_events WHERE command_id='brand-http-save-1')",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!((audit, outbox), (1, 1));
        let _ = owned_maker(&p);
    })
    .await;
}

#[tokio::test]
async fn brand_http_requires_origin_and_idempotency_and_rejects_foreign_scope() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        seed(&p).await;
        let app = router(&p);
        let without_key = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/brand")
            .header("host", "acme.example.test")
            .header("cookie", cookie(&p))
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "brand-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"action":"save"}"#))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(without_key).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let foreign_origin = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/brand")
            .header("host", "acme.example.test")
            .header("cookie", cookie(&p))
            .header("origin", "https://foreign.example.test")
            .header("idempotency-key", "brand-http-origin")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "brand-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"action":"save"}"#))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(foreign_origin).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        let unknown_host = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/brand")
            .header("host", "unknown.example.test")
            .header("cookie", cookie(&p))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(unknown_host).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
    })
    .await;
}

#[tokio::test]
async fn brand_route_is_reachable_through_the_private_api_boundary() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        seed(&p).await;
        let mut document = private_fixture::document(&p.system.control, &p.system.holdings).await;
        document["financeJournalReads"] =
            serde_json::json!([{"holdingId": "h001", "companyId": "company"}]);
        let configuration = DevelopmentApiConfiguration::parse(&document.to_string()).unwrap();
        let runtime = assemble_development_api(
            configuration,
            p.system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
        )
        .await
        .unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/brand")
            .header("host", "acme.example.test")
            .header("cookie", cookie(&p))
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "brand-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        let response = runtime.router().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        runtime.close().await;
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(value["data"]["drafts"].is_array());
    })
    .await;
}
