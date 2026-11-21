//! Native Organization HTTP ingress over the actual Account session and
//! current IAM authority. Exercises GET workspace, POST lifecycle save/review
//! and the closed origin/idempotency/scope failures.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::{
    account_holding_bindings::{ConfiguredAccountHolding, ConfiguredHoldingRegistry},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
use erpx_service::organization_nodes::{
    OrganizationNodeService, OrganizationNodesConfiguration,
};
use erpx_service::organization_nodes_http::{OrganizationNodesTarget, organization_nodes_router};
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

async fn grant_organization(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"organization.manage\",\"people.manage\",\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) \
         ON CONFLICT DO NOTHING;\
         UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review'",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

async fn seed_nodes(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('branch','branch','north','North','company','company','branch','active'), \
               ('department','department','service','Service','branch','company','branch','active') \
         ON CONFLICT DO NOTHING;\
         INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,version,created_at,updated_at) \
         VALUES('level-1','company','ORG_L1','CEO',-1,1,1,1,0,0), \
               ('level-2','company','ORG_L2','Manager',-2,2,1,1,0,0), \
               ('level-foreign','foreign','ORG_L1','Foreign',-1,1,1,1,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at) \
         VALUES('position-branch','company','branch',NULL,'North lead','level-1','fixture',0,0), \
               ('position-department','company','branch','department','Service lead','level-2','fixture',0,0), \
               ('position-foreign','foreign',NULL,NULL,'Foreign lead','level-foreign','fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO organization_templates(id,company_id,name,positions_json,version,created_by,created_at,updated_at) \
         VALUES('template-company','company','Standard','[{\"key\":\"ceo\",\"title\":\"CEO\",\"levelNumber\":1,\"reportsToKey\":null,\"isCeo\":true}]',1,'fixture',0,0), \
               ('template-foreign','foreign','Foreign','[]',1,'fixture',0,0) ON CONFLICT DO NOTHING",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn router(p: &review::Prepared) -> axum::Router {
    let service = OrganizationNodeService::new(
        p.authorizer.clone(),
        OrganizationNodesConfiguration::new(
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
    organization_nodes_router(
        Arc::new(policy),
        Arc::new(registry),
        Arc::new(FieldServiceReadScopeHeaders),
        vec![OrganizationNodesTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            service: Arc::new(service),
        }],
    )
    .unwrap()
}

fn cookie(p: &review::Prepared, reviewer: bool) -> String {
    let token = if reviewer {
        &p.reviewer_token
    } else {
        &p.maker_token
    };
    format!("__Host-erpx-product={}", token.expose())
}

fn read_request(p: &review::Prepared, reviewer: bool, company: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri("/api/v1/holdings/acme/people/organization?changeState=open")
        .header("host", "acme.example.test")
        .header("cookie", cookie(p, reviewer))
        .header("x-erpx-context-company", company)
        .header(
            "x-erpx-context-assignment",
            if reviewer {
                "finance-fixture-b"
            } else {
                "finance-fixture-a"
            },
        )
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "3")
        .body(Body::empty())
        .unwrap()
}

fn save_request(p: &review::Prepared, key: &str, origin: Option<&str>, company: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/holdings/acme/people/organization/changes")
        .header("host", "acme.example.test")
        .header("cookie", cookie(p, false))
        .header("x-erpx-context-company", company)
        .header("x-erpx-context-assignment", "finance-fixture-a")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "3")
        .header("content-type", "application/json");
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    if !key.is_empty() {
        builder = builder.header("idempotency-key", key);
    }
    builder
        .body(Body::from(
            r#"{"submit":true,"action":"create","kind":"branch","slug":"alpha","displayName":"Alpha branch","branchId":null,"targetNodeId":null,"expectedNodeVersion":null,"expectedVersion":0,"reason":"Open the approved alpha operation"}"#,
        ))
        .unwrap()
}

fn review_request(p: &review::Prepared, change_id: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/holdings/acme/people/organization/changes/{change_id}/review"
        ))
        .header("host", "acme.example.test")
        .header("cookie", cookie(p, true))
        .header("origin", "https://acme.example.test")
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", "finance-fixture-b")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "3")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"expectedVersion":1,"decision":"approve","reason":"Capacity and authority reviewed"}"#,
        ))
        .unwrap()
}

fn review_request_with_proof(p: &review::Prepared, change_id: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/holdings/acme/people/organization/changes/{change_id}/review"
        ))
        .header("host", "acme.example.test")
        .header("cookie", cookie(p, true))
        .header("origin", "https://acme.example.test")
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", "finance-fixture-b")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "3")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"expectedVersion":1,"decision":"approve","reason":"Capacity and authority reviewed","attachmentSetIds":["proof-set"]}"#,
        ))
        .unwrap()
}

async fn body(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn organization_http_reads_and_reviews_with_the_normal_session() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_organization(&p).await;
        seed_nodes(&p).await;
        let app = router(&p);
        let response = app.clone().oneshot(read_request(&p, false, "company")).await.unwrap();
        let status = response.status();
        let value = body(response).await;
        if status != StatusCode::OK {
            panic!("organization read failed: {value}");
        }
        assert_eq!(value["data"]["canManageUnits"], true);
        assert_eq!(value["data"]["canManageSpending"], true);
        assert_eq!(value["data"]["actorId"], "fixture-a");
        assert_eq!(value["data"]["changeQueue"]["state"], "open");
        assert!(
            value["data"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["id"] == "branch")
        );
        let positions = value["data"]["positions"].as_array().unwrap();
        assert!(positions.iter().any(|position| position["id"] == "position-branch"));
        assert!(positions.iter().any(|position| position["id"] == "position-department"));
        assert!(!positions.iter().any(|position| position["id"] == "position-foreign"));
        assert_eq!(
            value["data"]["levels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|level| level["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["level-1", "level-2"]
        );
        assert_eq!(
            value["data"]["templates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|template| template["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["template-company"]
        );
        assert_eq!(
            value["data"]["templates"][0]["positions"][0]["title"],
            "CEO"
        );

        let response = app
            .clone()
            .oneshot(save_request(&p, "organization-http-save-0001", Some("https://acme.example.test"), "company"))
            .await
            .unwrap();
        let status = response.status();
        let value = body(response).await;
        if status != StatusCode::OK {
            panic!("organization save failed: {value}");
        }
        assert_eq!(value["data"]["state"], "submitted");
        assert_eq!(value["data"]["version"], 1);
        let change_id = value["data"]["id"].as_str().unwrap().to_owned();
        let (state, display): (String, String) = sqlx::query_as(
            "SELECT state,display_name FROM organization_node_changes WHERE id=$1 AND company_id='company'",
        )
        .bind(&change_id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!((state.as_str(), display.as_str()), ("submitted", "Alpha branch"));
        let (audit, outbox): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*)::bigint FROM audit_events WHERE command_id='organization-http-save-0001'),\
             (SELECT count(*)::bigint FROM outbox_events WHERE command_id='organization-http-save-0001')",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!((audit, outbox), (1, 1));

        let response = app
            .clone()
            .oneshot(review_request_with_proof(
                &p,
                &change_id,
                "organization-http-review-proof-0001",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body(response).await["error"]["code"],
            "ORGANIZATION_ATTACHMENT_BINDING_UNAVAILABLE"
        );
        let (state, receipt): (String, bool) = sqlx::query_as(
            "SELECT (SELECT state FROM organization_node_changes WHERE id=$1), \
             EXISTS(SELECT 1 FROM command_receipts WHERE command_id='organization-http-review-proof-0001')",
        )
        .bind(&change_id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!((state.as_str(), receipt), ("submitted", false));

        let response = app
            .clone()
            .oneshot(review_request(&p, &change_id, "organization-http-review-0001"))
            .await
            .unwrap();
        let status = response.status();
        let value = body(response).await;
        if status != StatusCode::OK {
            panic!("organization review failed: {value}");
        }
        assert_eq!(value["data"]["state"], "applied");
        assert_eq!(
            value["data"]["nodeId"],
            format!("org-node-{change_id}").as_str()
        );
        let (room, name): (String, String) = sqlx::query_as(
            "SELECT r.id,r.name FROM messaging_rooms r WHERE r.organization_node_id=$1",
        )
        .bind(value["data"]["nodeId"].as_str().unwrap())
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(name, "Alpha branch");
        assert_eq!(room, format!("org-channel-{}", value["data"]["nodeId"].as_str().unwrap()));
        let _ = owned_maker(&p);
    })
    .await;
}

#[tokio::test]
async fn organization_http_refuses_foreign_origin_missing_key_and_foreign_company() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_organization(&p).await;
        seed_nodes(&p).await;
        let app = router(&p);

        let response = app
            .clone()
            .oneshot(save_request(&p, "organization-http-origin-0001", Some("https://evil.example.test"), "company"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(body(response).await["error"]["code"], "ORIGIN_FORBIDDEN");

        let response = app
            .clone()
            .oneshot(save_request(&p, "", Some("https://acme.example.test"), "company"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body(response).await["error"]["code"],
            "IDEMPOTENCY_KEY_REQUIRED"
        );

        let response = app
            .clone()
            .oneshot(read_request(&p, false, "foreign"))
            .await
            .unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::CONFLICT | StatusCode::NOT_FOUND
        ));

        let response = app
            .clone()
            .oneshot(save_request(&p, "organization-http-foreign-0001", Some("https://acme.example.test"), "foreign"))
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::OK);
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM organization_node_changes WHERE id='org-change-organization-http-foreign-0001')",
            )
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap()
            .eq(&false)
        );

        sqlx::raw_sql(
            "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
             VALUES('*','custom_finance_review',3,'[\"organization.manage\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) \
             ON CONFLICT DO NOTHING;\
             UPDATE iam_roles SET published_revision=3 WHERE company_id='*' AND id='custom_finance_review'",
        )
        .execute(p.system.holdings[0].raw())
        .await
        .unwrap();
        let response = app
            .clone()
            .oneshot(read_request(&p, false, "company"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(body(response).await["error"]["code"], "PERMISSION_REQUIRED");
    })
    .await;
}
