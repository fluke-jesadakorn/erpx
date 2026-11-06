//! Native Maintenance HTTP command over the actual Account session, current
//! IAM and PostgreSQL. The retained create path remains explicit.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_maintenance::public::http::{
    MaintenanceHttpTarget, maintenance_router,
};
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
use erpx_service::maintenance::{
    MaintenanceBackend, MaintenanceConfiguration, MaintenanceService,
};
use std::sync::Arc;
use tower::ServiceExt;

fn cookie(p: &review::Prepared) -> String {
    format!("__Host-erpx-product={}", p.maker_token.expose())
}

fn request(p: &review::Prepared, body: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/holdings/acme/maintenance")
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

async fn grant_maintenance(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"records.write\",\"records.review\",\"module.maintenance.read\"]','[]','[\"holding\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';\
         UPDATE assignments SET modules_json='[\"finance\",\"maintenance\"]',version=4 WHERE id='finance-fixture-a';\
         UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a'",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

async fn seed_work(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('branch','branch','branch','Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) \
         VALUES('worker','fixture-a','company','Synthetic Worker','TH','fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) \
         VALUES('engagement','worker','company','employee','salaried','active','2026-01-01','Technician','THB','fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) \
         VALUES('company','fixture-a','local:fixture-a','worker','active','local') ON CONFLICT DO NOTHING;\
         INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) \
         VALUES('asset','company','branch','SYNTHETIC','Synthetic asset','synthetic','THB','fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at,version) \
         VALUES('work','company','branch','asset','Blocked work evidence','normal','in_progress','fixture-a','fixture-a',0,0,1) ON CONFLICT DO NOTHING;\
         INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) \
         VALUES('company','assets',1,'active',0,3600000000000,1,'Synthetic approved source','public-synthetic-evidence',0),\
               ('company','assets-parts-costs',1,'active',0,3600000000000,1,'Synthetic approved source','public-synthetic-evidence',0) ON CONFLICT DO NOTHING;\
         INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
         SELECT 'maintenance-grant-'||package_id,company_id,package_id,'fixture-a','synthetic-request',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),0 \
         FROM company_package_grants WHERE company_id='company' AND package_id IN ('assets','assets-parts-costs') ON CONFLICT DO NOTHING",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn router(p: &review::Prepared) -> axum::Router {
    let service = MaintenanceService::new(
        p.authorizer.clone(),
        MaintenanceConfiguration::new(
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
    let backend = MaintenanceBackend::new(
        Arc::new(review::account::fixture::account::policy()),
        Arc::new(FieldServiceReadScopeHeaders),
        Arc::new(service),
    );
    maintenance_router(vec![MaintenanceHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(backend),
    }])
    .unwrap()
}

#[tokio::test]
async fn maintenance_http_blocks_work_through_the_native_backend_and_keeps_create_retained() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_maintenance(&p).await;
        seed_work(&p).await;
        let app = router(&p);

        let blocked = app
            .clone()
            .oneshot(request(
                &p,
                r#"{"id":"work","action":"block","expectedVersion":1,"reason":"Synthetic independent block evidence"}"#,
                "maintenance-http-block",
            ))
            .await
            .unwrap();
        let blocked_status = blocked.status();
        let blocked = json(blocked).await;
        assert_eq!(blocked_status, StatusCode::OK, "{blocked}");
        assert_eq!(blocked["data"]["id"], "work");
        assert_eq!(blocked["data"]["status"], "blocked");
        assert_eq!(blocked["data"]["version"], 2);

        let persisted: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM maintenance_work_orders WHERE id='work' AND company_id='company'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(persisted, ("blocked".to_owned(), 2));

        let replayed = app
            .clone()
            .oneshot(request(
                &p,
                r#"{"id":"work","action":"block","expectedVersion":1,"reason":"Synthetic independent block evidence"}"#,
                "maintenance-http-block",
            ))
            .await
            .unwrap();
        assert_eq!(replayed.status(), StatusCode::OK);
        assert_eq!(json(replayed).await["data"]["version"], 2);

        let create = app
            .clone()
            .oneshot(request(
                &p,
                r#"{"assetId":"asset","assetVersion":1,"title":"Retained create boundary","priority":"normal","ownerId":"fixture-a","reason":"Synthetic create boundary"}"#,
                "maintenance-http-create",
            ))
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            json(create).await["error"]["code"],
            "MAINTENANCE_COMMAND_UNAVAILABLE"
        );
    })
    .await;
}
