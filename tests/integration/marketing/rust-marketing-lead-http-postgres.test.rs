//! Composed native Marketing lead/asset HTTP over the real Account session,
//! current IAM, same-unit Commercial History and PostgreSQL.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_marketing::public::lead_candidate_command_http::{
    MarketingLeadCommandHttpTarget, marketing_lead_command_http_router,
};
use erpx_server_marketing::public::lead_candidate_http::{
    MarketingLeadHttpTarget, marketing_lead_http_router,
};
use erpx_server_shared::public::Clock;
use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
use erpx_service::marketing::{
    MarketingBackend, MarketingConfiguration, MarketingService,
};
use std::sync::Arc;
use tower::ServiceExt;

fn cookie(p: &review::Prepared) -> String {
    format!("__Host-erpx-product={}", p.maker_token.expose())
}

fn request(path: &str, p: &review::Prepared) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("host", "acme.example.test")
        .header("cookie", cookie(p))
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", "finance-fixture-a")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "4")
        .body(Body::empty())
        .unwrap()
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

fn command_request(path: &str, p: &review::Prepared, key: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie(p))
        .header("content-type", "application/json")
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", "finance-fixture-a")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "4")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn engagement(expected_version: i64) -> serde_json::Value {
    serde_json::json!({
        "action": "record-engagement",
        "expectedVersion": expected_version,
        "reason": "Reviewed the lead and recorded the current engagement.",
        "eventType": "email_open",
        "channel": "email"
    })
}

/// Candidate capture requires the selected branch workspace, so the native
/// command carries the exact branch slug header.
fn branch_command_request(
    path: &str,
    p: &review::Prepared,
    key: &str,
    body: serde_json::Value,
) -> Request<Body> {
    let mut request = command_request(path, p, key, body);
    request
        .headers_mut()
        .insert("x-erpx-workspace-branch", "north".parse().unwrap());
    request
}

async fn effects(p: &review::Prepared, key: &str) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
         (SELECT count(*) FROM audit_events WHERE command_id=$1),\
         (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
    )
    .bind(key)
    .fetch_one(p.system.holdings[0].raw())
    .await
    .unwrap()
}

async fn grant_marketing(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"records.read\",\"records.write\",\"module.marketing.read\"]','[]','[\"holding\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';\
         UPDATE assignments SET modules_json='[\"finance\",\"marketing\"]',version=4 WHERE id='finance-fixture-a';\
         UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a'",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) \
         VALUES('company','sales-marketing',1,'active',0,{expires},1,'Synthetic reviewed marketing purchase','public synthetic evidence',{now}),\
               ('company','sales',1,'active',0,{expires},1,'Synthetic reviewed sales dependency','public synthetic evidence',{now}),\
               ('company','inventory',1,'active',0,{expires},1,'Synthetic reviewed inventory dependency','public synthetic evidence',{now}),\
               ('company','finance',1,'active',0,{expires},1,'Synthetic reviewed finance dependency','public synthetic evidence',{now});\
         INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
         SELECT 'marketing-grant-'||package_id,company_id,package_id,'fixture-a','synthetic-request',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),{now} \
         FROM company_package_grants WHERE company_id='company' AND package_id IN ('sales-marketing','sales','inventory','finance')"
    )))
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

async fn grant_marketing_iam_only(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"records.read\",\"records.write\",\"module.marketing.read\"]','[]','[\"holding\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';\
         UPDATE assignments SET modules_json='[\"finance\",\"marketing\"]',version=4 WHERE id='finance-fixture-a';\
         UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a'",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

async fn seed_marketing(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('branch','branch','north','North','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at) \
         VALUES('form-company','company','form','Lead form','{\"consentRequired\":true}','{}','active','fixture-a','fixture-b',1,10,10) ON CONFLICT DO NOTHING;\
         INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) \
         VALUES('campaign-autumn','company','AUTUMN','Autumn demand','fixture-a',10) ON CONFLICT DO NOTHING;\
         INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) \
         VALUES('candidate-1','company','branch',NULL,'form-company',1,'Synthetic one','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','pending',NULL,'fixture-a',NULL,1,100,100) ON CONFLICT DO NOTHING;\
         INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) \
         VALUES('submission-1','candidate-1',1,'form_submit','web','evidence','fixture-a',100) ON CONFLICT DO NOTHING",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

/// Draft assets used by the review transition. `content-review` belongs to a
/// different author so the current session can activate it independently;
/// `content-own` proves the creator cannot independently activate their own
/// draft.
async fn seed_marketing_review_assets(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at) \
         VALUES('content-review','company','content','Reviewable content','{\"reference\":\"Launch brief\"}','{\"summary\":\"Review summary\"}','draft','fixture-b',NULL,1,10,10),\
               ('content-own','company','content','Own content','{\"reference\":\"Own brief\"}','{\"summary\":\"Own summary\"}','draft','fixture-a',NULL,1,10,10) ON CONFLICT DO NOTHING",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn router(p: &review::Prepared) -> axum::Router {
    let service = MarketingService::new(
        p.authorizer.clone(),
        MarketingConfiguration::new(
            erpx_server_shared::public::HoldingId::new("h001").unwrap(),
            erpx_server_shared::public::CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
    let backend = MarketingBackend::new(
        Arc::new(review::account::fixture::account::policy()),
        Arc::new(FieldServiceReadScopeHeaders),
        Arc::new(service),
    );
    marketing_lead_http_router(vec![MarketingLeadHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(backend),
    }])
    .unwrap()
}

fn command_router(p: &review::Prepared) -> axum::Router {
    let service = MarketingService::new(
        p.authorizer.clone(),
        MarketingConfiguration::new(
            erpx_server_shared::public::HoldingId::new("h001").unwrap(),
            erpx_server_shared::public::CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
    let backend = MarketingBackend::new(
        Arc::new(review::account::fixture::account::policy()),
        Arc::new(FieldServiceReadScopeHeaders),
        Arc::new(service),
    );
    marketing_lead_command_http_router(vec![MarketingLeadCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(backend),
    }])
    .unwrap()
}

#[tokio::test]
async fn marketing_lead_http_reads_assets_and_detail_with_same_unit_purchase_history() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_marketing(&p).await;
        seed_marketing(&p).await;
        let app = router(&p);

        let assets = app
            .clone()
            .oneshot(request("/api/v1/holdings/acme/marketing/assets", &p))
            .await
            .unwrap();
        assert_eq!(assets.status(), StatusCode::OK);
        let assets = json(assets).await;
        assert_eq!(assets["data"]["assets"].as_array().unwrap().len(), 1);
        assert_eq!(assets["data"]["assets"][0]["id"], "form-company");

        let detail = app
            .clone()
            .oneshot(request(
                "/api/v1/holdings/acme/marketing/leads/candidate-1",
                &p,
            ))
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        let detail = json(detail).await;
        assert_eq!(detail["data"]["candidate"]["id"], "candidate-1");
        assert_eq!(detail["data"]["history"].as_array().unwrap().len(), 1);
        assert_eq!(detail["data"]["purchase"]["history"], true);
        assert!(detail["data"]["purchase"]["revision"].as_str().unwrap().len() > 0);

    })
    .await;
}

#[tokio::test]
async fn marketing_lead_http_denies_without_a_current_purchase_history() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_marketing_iam_only(&p).await;
        seed_marketing(&p).await;
        let app = router(&p);
        let denied = app
            .oneshot(request("/api/v1/holdings/acme/marketing/assets", &p))
            .await
            .unwrap();
        let status = denied.status();
        let denied = json(denied).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
        assert!(
            ["PURCHASE_REQUIRED", "DEPENDENCY_REQUIRED", "PACKAGE_REQUIRED"]
                .contains(&denied["error"]["code"].as_str().unwrap_or(""))
        );
    })
    .await;
}

#[tokio::test]
async fn marketing_lead_command_records_versioned_engagement_history_and_replays() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_marketing(&p).await;
        seed_marketing(&p).await;
        let app = command_router(&p);
        let path = "/api/v1/holdings/acme/marketing/leads/candidate-1/changes";

        let response = app
            .clone()
            .oneshot(command_request(path, &p, "engagement-browser", engagement(1)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = json(response).await;
        assert_eq!(response["data"]["id"], "candidate-1");
        assert_eq!(response["data"]["status"], "pending");
        assert_eq!(response["data"]["version"], 2);

        let row: (i64, String) = sqlx::query_as(
            "SELECT version,status FROM marketing_lead_candidates WHERE id='candidate-1'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(row, (2, "pending".into()));
        let history: Vec<(i64, String, String, String)> = sqlx::query_as(
            "SELECT version,event_type,channel,evidence FROM marketing_engagement_events \
             WHERE candidate_id='candidate-1' ORDER BY version,id",
        )
        .fetch_all(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(
            history[1],
            (
                2,
                "email_open".into(),
                "email".into(),
                "Reviewed the lead and recorded the current engagement.".into()
            )
        );
        let evidence: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='engagement-browser'),\
             (SELECT count(*) FROM audit_events WHERE command_id='engagement-browser'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='engagement-browser')",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(evidence, (1, 1, 1));

        let replay = app
            .clone()
            .oneshot(command_request(path, &p, "engagement-browser", engagement(1)))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["version"], 2);
        let after_replay: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM marketing_engagement_events WHERE candidate_id='candidate-1'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(after_replay, 2);

        let stale = app
            .clone()
            .oneshot(command_request(path, &p, "engagement-stale", engagement(1)))
            .await
            .unwrap();
        let status = stale.status();
        let stale = json(stale).await;
        assert_eq!(status, StatusCode::CONFLICT, "{stale}");
        assert_eq!(stale["error"]["code"], "MARKETING_CANDIDATE_CHANGED");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM marketing_engagement_events WHERE candidate_id='candidate-1'",
            )
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap(),
            2
        );
    })
    .await;
}

#[tokio::test]
async fn marketing_lead_command_denies_without_purchase_and_non_native_actions() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_marketing_iam_only(&p).await;
        seed_marketing(&p).await;
        let app = command_router(&p);
        let path = "/api/v1/holdings/acme/marketing/leads/candidate-1/changes";

        let denied = app
            .clone()
            .oneshot(command_request(path, &p, "engagement-no-purchase", engagement(1)))
            .await
            .unwrap();
        let status = denied.status();
        let denied = json(denied).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
        assert!(
            ["PURCHASE_REQUIRED", "DEPENDENCY_REQUIRED", "PACKAGE_REQUIRED"]
                .contains(&denied["error"]["code"].as_str().unwrap_or(""))
        );

        // New-work creates are denied for the same missing purchase before any
        // Marketing row is written.
        let create = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/marketing/campaigns",
                &p,
                "campaign-no-purchase",
                serde_json::json!({"code": "DENIED", "name": "Denied demand"}),
            ))
            .await
            .unwrap();
        let status = create.status();
        let create = json(create).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{create}");
        assert!(
            ["PURCHASE_REQUIRED", "DEPENDENCY_REQUIRED", "PACKAGE_REQUIRED"]
                .contains(&create["error"]["code"].as_str().unwrap_or(""))
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM marketing_campaigns WHERE company_id='company' AND code='DENIED'",
            )
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap(),
            0
        );

        let handoff = app
            .oneshot(command_request(
                path,
                &p,
                "handoff-not-native",
                serde_json::json!({
                    "action": "handoff",
                    "expectedVersion": 1,
                    "reason": "Not part of the native engagement slice.",
                    "dueDate": "2026-12-01"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(handoff.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json(handoff).await["error"]["code"],
            "MARKETING_COMMAND_NOT_NATIVE"
        );
    })
    .await;
}

#[tokio::test]
async fn marketing_lead_command_creates_campaign_candidate_and_asset_with_replay() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_marketing(&p).await;
        seed_marketing(&p).await;
        let app = command_router(&p);
        let db = p.system.holdings[0].raw();

        // Campaign create through the normalized native body.
        let path = "/api/v1/holdings/acme/marketing/campaigns";
        let campaign = serde_json::json!({"code": "SPRING", "name": "Spring demand"});
        let created = app
            .clone()
            .oneshot(command_request(path, &p, "campaign-create", campaign.clone()))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);
        let created = json(created).await;
        assert_eq!(created["data"]["id"], "campaign-campaign-create");
        assert_eq!(created["data"]["code"], "SPRING");
        assert_eq!(created["data"]["name"], "Spring demand");
        assert_eq!(created["data"]["version"], 1);
        let stored: (String, String) = sqlx::query_as(
            "SELECT code,name FROM marketing_campaigns \
             WHERE id='campaign-campaign-create' AND company_id='company'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stored, ("SPRING".into(), "Spring demand".into()));
        assert_eq!(effects(&p, "campaign-create").await, (1, 1, 1));

        let replay = app
            .clone()
            .oneshot(command_request(path, &p, "campaign-create", campaign))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["id"], "campaign-campaign-create");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM marketing_campaigns WHERE company_id='company' AND code='SPRING'",
            )
            .fetch_one(db)
            .await
            .unwrap(),
            1
        );

        let duplicate = app
            .clone()
            .oneshot(command_request(
                path,
                &p,
                "campaign-duplicate",
                serde_json::json!({"code": "SPRING", "name": "Other demand"}),
            ))
            .await
            .unwrap();
        let status = duplicate.status();
        let duplicate = json(duplicate).await;
        assert_eq!(status, StatusCode::CONFLICT, "{duplicate}");
        assert_eq!(duplicate["error"]["code"], "MARKETING_CAMPAIGN_CODE_TAKEN");

        // Candidate capture consumes the approved active form in the selected
        // branch workspace.
        let path = "/api/v1/holdings/acme/marketing/leads";
        let candidate = serde_json::json!({
            "formId": "form-company",
            "formVersion": 1,
            "branchId": "branch",
            "contactName": "Synthetic new lead",
            "contactDetail": "new-lead@example.test",
            "consentEvidence": "Visitor consent accepted",
            "attribution": {"campaign": "SPRING", "source": "web"},
            "brief": "Requested a callback"
        });
        let created = app
            .clone()
            .oneshot(branch_command_request(
                path,
                &p,
                "candidate-create",
                candidate.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);
        let created = json(created).await;
        assert_eq!(created["data"]["id"], "marketing-candidate-candidate-create");
        assert_eq!(created["data"]["status"], "pending");
        assert_eq!(created["data"]["version"], 1);
        let stored: (String, String, i64) = sqlx::query_as(
            "SELECT status,contact_name,version FROM marketing_lead_candidates \
             WHERE id='marketing-candidate-candidate-create'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stored, ("pending".into(), "Synthetic new lead".into(), 1));
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT event_type FROM marketing_engagement_events \
                 WHERE candidate_id='marketing-candidate-candidate-create'",
            )
            .fetch_all(db)
            .await
            .unwrap(),
            vec!["form_submit".to_string()]
        );
        assert_eq!(effects(&p, "candidate-create").await, (1, 1, 1));

        let replay = app
            .clone()
            .oneshot(branch_command_request(
                path,
                &p,
                "candidate-create",
                candidate,
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(
            json(replay).await["data"]["id"],
            "marketing-candidate-candidate-create"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM marketing_engagement_events \
                 WHERE candidate_id='marketing-candidate-candidate-create'",
            )
            .fetch_one(db)
            .await
            .unwrap(),
            1
        );

        // Asset draft creation keeps the browser envelope on the router, which
        // strips `entity` before the exact native parse.
        let path = "/api/v1/holdings/acme/marketing/assets";
        let asset = serde_json::json!({
            "entity": "asset",
            "kind": "content",
            "name": "Launch content",
            "configuration": {"reference": "Launch brief"},
            "preview": {"summary": "Launch summary"}
        });
        let created = app
            .clone()
            .oneshot(command_request(path, &p, "asset-create", asset.clone()))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);
        let created = json(created).await;
        assert_eq!(created["data"]["id"], "marketing-content-asset-create");
        assert_eq!(created["data"]["status"], "draft");
        assert_eq!(created["data"]["version"], 1);
        let stored: (String, String, String) = sqlx::query_as(
            "SELECT status,kind,configuration_json FROM marketing_assets \
             WHERE id='marketing-content-asset-create'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stored.0, "draft");
        assert_eq!(stored.1, "content");
        assert!(stored.2.contains("Launch brief"));
        let event: (i64, String, String) = sqlx::query_as(
            "SELECT version,action,status FROM marketing_asset_events \
             WHERE asset_id='marketing-content-asset-create'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(event, (1, "create".into(), "draft".into()));
        assert_eq!(effects(&p, "asset-create").await, (1, 1, 1));

        let replay = app
            .clone()
            .oneshot(command_request(path, &p, "asset-create", asset))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(
            json(replay).await["data"]["id"],
            "marketing-content-asset-create"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM marketing_asset_events \
                 WHERE asset_id='marketing-content-asset-create'",
            )
            .fetch_one(db)
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn marketing_lead_command_reviews_assets_with_independent_review_and_settlement() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_marketing(&p).await;
        seed_marketing(&p).await;
        seed_marketing_review_assets(&p).await;
        let app = command_router(&p);
        let db = p.system.holdings[0].raw();
        let path = "/api/v1/holdings/acme/marketing/assets/content-review/changes";
        let activate = serde_json::json!({
            "action": "activate",
            "expectedVersion": 1,
            "reason": "Independent activation review"
        });

        let reviewed = app
            .clone()
            .oneshot(command_request(path, &p, "asset-activate", activate.clone()))
            .await
            .unwrap();
        assert_eq!(reviewed.status(), StatusCode::OK);
        let reviewed = json(reviewed).await;
        assert_eq!(reviewed["data"]["id"], "content-review");
        assert_eq!(reviewed["data"]["status"], "active");
        assert_eq!(reviewed["data"]["version"], 2);
        let stored: (String, String, i64) = sqlx::query_as(
            "SELECT status,reviewed_by,version FROM marketing_assets WHERE id='content-review'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stored, ("active".into(), "fixture-a".into(), 2));
        let event: (i64, String, String) = sqlx::query_as(
            "SELECT version,action,status FROM marketing_asset_events WHERE asset_id='content-review'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(event, (2, "activate".into(), "active".into()));
        assert_eq!(effects(&p, "asset-activate").await, (1, 1, 1));

        let replay = app
            .clone()
            .oneshot(command_request(path, &p, "asset-activate", activate))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["version"], 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM marketing_asset_events WHERE asset_id='content-review'",
            )
            .fetch_one(db)
            .await
            .unwrap(),
            1
        );

        let stale = app
            .clone()
            .oneshot(command_request(
                path,
                &p,
                "asset-activate-stale",
                serde_json::json!({
                    "action": "activate",
                    "expectedVersion": 1,
                    "reason": "Second activation attempt"
                }),
            ))
            .await
            .unwrap();
        let status = stale.status();
        let stale = json(stale).await;
        assert_eq!(status, StatusCode::CONFLICT, "{stale}");
        assert_eq!(stale["error"]["code"], "MARKETING_ASSET_CHANGED");

        // The creator cannot independently activate their own draft.
        let own = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/marketing/assets/content-own/changes",
                &p,
                "asset-activate-own",
                serde_json::json!({
                    "action": "activate",
                    "expectedVersion": 1,
                    "reason": "Self activation attempt"
                }),
            ))
            .await
            .unwrap();
        let status = own.status();
        let own = json(own).await;
        assert_eq!(status, StatusCode::CONFLICT, "{own}");
        assert_eq!(
            own["error"]["code"],
            "MARKETING_INDEPENDENT_REVIEW_REQUIRED"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM marketing_assets WHERE id='content-own'",
            )
            .fetch_one(db)
            .await
            .unwrap(),
            "draft"
        );

        // Retirement settles the existing active asset and may be performed by
        // its creator.
        let retired = app
            .oneshot(command_request(
                "/api/v1/holdings/acme/marketing/assets/form-company/changes",
                &p,
                "asset-retire",
                serde_json::json!({
                    "action": "retire",
                    "expectedVersion": 1,
                    "reason": "Superseded by the reviewed launch form"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(retired.status(), StatusCode::OK);
        let retired = json(retired).await;
        assert_eq!(retired["data"]["status"], "retired");
        assert_eq!(retired["data"]["version"], 2);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM marketing_assets WHERE id='form-company'",
            )
            .fetch_one(db)
            .await
            .unwrap(),
            "retired"
        );
        assert_eq!(effects(&p, "asset-retire").await, (1, 1, 1));
    })
    .await;
}
