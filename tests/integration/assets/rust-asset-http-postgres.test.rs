//! Native Assets HTTP ingress over the actual Account session, current IAM
//! authority and a serializable Holding transaction.
#![allow(dead_code)]
#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[path = "../../support/assets/http-backend.rs"]
mod backend;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_assets::public::asset_http::{AssetHttpTarget, asset_router};
use erpx_server_assets::public::asset_register::{AcquisitionCostProvider, AssetFuture, NewAsset};
use erpx_server_identity_access::public::{
    AuthenticationSecrets, OpaqueSecret, SecretDigest, SecretPurpose, SystemAuthenticationSecrets,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::sync::Arc;
use tower::ServiceExt;

const MAKER_PRODUCT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MAKER_BROWSER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const REVIEWER_PRODUCT: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const REVIEWER_BROWSER: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

/// Test-only Finance handoff. It writes the same `money_transaction_snapshots`
/// identity the retained snapshot producer writes, on the request transaction.
struct PostedCost;
impl AcquisitionCostProvider for PostedCost {
    fn prepare<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        id: &'a str,
        asset: &'a NewAsset,
    ) -> AssetFuture<'a, (), BusinessError> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) \
                 VALUES('company','operational_asset',$1,'acquisition_cost',$2::date,$3,2,$4::numeric,$4::numeric,NULL,true,0)",
            )
            .bind(id)
            .bind(&asset.acquired_date)
            .bind(&asset.currency)
            .bind(asset.cost_minor)
            .execute(&mut *connection)
            .await
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "ASSET_COST_TEST_FAILED",
                    "The test Finance handoff failed.",
                )
            })?;
            Ok(())
        })
    }
}

struct Fixture {
    control: DatabasePool,
    holding: DatabasePool,
    authorizer:
        Arc<erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer>,
    clock: Arc<auth::MutableClock>,
    maker: String,
    reviewer: String,
}

impl Fixture {
    fn service(&self) -> backend::AssetHttpService<auth::MutableClock> {
        backend::AssetHttpService::new(
            Arc::new(auth::policy()),
            self.authorizer.clone(),
            backend::AssetHttpConfiguration::new(
                HoldingId::new("h001").unwrap(),
                CompanyId::new("company").unwrap(),
            ),
            self.clock.clone(),
            Arc::new(PostedCost),
        )
    }

    fn router(&self) -> Router {
        asset_router(vec![AssetHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(self.service()),
        }])
        .unwrap()
    }
}

fn digest(raw: &str) -> SecretDigest {
    SystemAuthenticationSecrets.digest(
        SecretPurpose::Session,
        &OpaqueSecret::new(raw.into()).unwrap(),
    )
}

async fn seed(control: &DatabasePool, holding: &DatabasePool) -> Fixture {
    let (authorizer, clock) = auth::setup(control, holding).await;
    let now = clock.now().get();
    let expires = now + 3_600_000;
    let maker = digest(MAKER_PRODUCT);
    let maker_browser = digest(MAKER_BROWSER);
    let reviewer = digest(REVIEWER_PRODUCT);
    let reviewer_browser = digest(REVIEWER_BROWSER);
    sqlx::query("UPDATE session SET token=$1, \"expiresAt\"=to_timestamp(($2::bigint+3600000)::double precision/1000)::text WHERE id='parent'")
        .bind(maker_browser.as_str())
        .bind(now)
        .execute(control.raw())
        .await
        .unwrap();
    sqlx::query("UPDATE session SET token=$1, \"expiresAt\"=to_timestamp(($2::bigint+3600000)::double precision/1000)::text WHERE id='session'")
        .bind(maker.as_str())
        .bind(now)
        .execute(control.raw())
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES('reviewer','Reviewer','reviewer@example.test',1,0,0);\
         INSERT INTO session(id,\"userId\",token,\"expiresAt\",\"createdAt\",\"updatedAt\") VALUES('reviewer-parent','reviewer','placeholder',to_timestamp(3600)::text,to_timestamp(0)::text,to_timestamp(0)::text);",
    )
    .execute(control.raw())
    .await
    .unwrap();
    sqlx::query("UPDATE session SET token=$1, \"expiresAt\"=to_timestamp(($2::bigint+3600000)::double precision/1000)::text WHERE id='reviewer-parent'")
        .bind(reviewer_browser.as_str())
        .bind(now)
        .execute(control.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO session(id,\"userId\",token,\"expiresAt\",\"createdAt\",\"updatedAt\") VALUES('reviewer-session','reviewer',$1,to_timestamp(($2::bigint+3600000)::double precision/1000)::text,to_timestamp(0)::text,to_timestamp(0)::text)")
        .bind(reviewer.as_str())
        .bind(now)
        .execute(control.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES('reviewer-parent','account','development',NULL,$1,$2)")
        .bind(now)
        .bind(expires)
        .execute(control.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES('reviewer-session','holding:development:h001:acme.example.test','development','reviewer-parent',$1,$2)")
        .bind(now)
        .bind(expires)
        .execute(control.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES('reviewer-browser','reviewer-parent',1,$1)")
        .bind(now)
        .execute(control.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES('reviewer-parent','reviewer-browser','saved-reviewer')")
        .execute(control.raw())
        .await
        .unwrap();

    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','custom_assets_manager','Synthetic Assets manager',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES\
         ('*','custom_assets_manager',1,'[\"records.read\",\"records.write\",\"records.review\",\"finance.read\",\"finance.write\",\"module.assets.read\"]','[]','[\"holding\"]','fixture',0);",
    )
    .execute(holding.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
         VALUES('assets-grant','user','custom_assets_manager','[\"assets\"]','company',$1,$2,3,'holding'),\
         ('reviewer-grant','reviewer','custom_assets_manager','[\"assets\"]','company',$1,$2,3,'holding')",
    )
    .bind(now - 1_000)
    .bind(expires)
    .execute(holding.raw())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE active_work_contexts SET assignment_id='assets-grant',assignment_version=3,revision=7,expires_at=$1,updated_at=$2 WHERE session_id='session'",
    )
    .bind(expires)
    .bind(now)
    .execute(holding.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) \
         VALUES('reviewer-session','reviewer','company','reviewer-grant',3,7,$1,$2)",
    )
    .bind(expires)
    .bind(now)
    .execute(holding.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES\
         ('department','department','department','Synthetic Department','branch','company','branch','active'),\
         ('branch-b','branch','south','Synthetic Branch B','company','company',NULL,'active');\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) VALUES\
         ('person-maker','user','company','Assets Maker','TH','active',1,'fixture',0,0),\
         ('person-reviewer','reviewer','company','Assets Reviewer','TH','active',1,'fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,version,created_by,created_at,updated_at) VALUES\
         ('engagement-maker','person-maker','company','employee','salaried','active','2020-01-01','Asset custodian','THB',1,'fixture',0,0),\
         ('engagement-reviewer','person-reviewer','company','employee','salaried','active','2020-01-01','Asset reviewer','THB',1,'fixture',0,0);\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES\
         ('company','user','local:user','person-maker','active','local',1),\
         ('company','reviewer','local:reviewer','person-reviewer','active','local',1);\
         INSERT INTO finance_charts(id,company_id,name,base_currency,status,created_by,created_at,updated_at) VALUES\
         ('chart','company','Synthetic Chart','THB','active','fixture',0,0);\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) VALUES\
         ('period','company','2026','2026-01-01','2026-12-31','open',0);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES\
         ('cash','chart','company','1000','Cash','asset',0,0);\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,status,created_by,created_at,updated_at) VALUES\
         ('journal-1','company','chart','period','J-1','2026-01-01','THB','posted','fixture',0,0);\
         INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,cost_minor,useful_life_months,in_service_date,status,currency,version,activation_journal_id,created_by,created_at,updated_at) VALUES\
         ('finance-1','company','FA-1','Finance Asset','cash',1000,12,'2026-01-01','in_service','THB',1,'journal-1','fixture',0,0);\
         INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,created_by,created_at,updated_at) VALUES\
         ('asset-a','company','branch','department','A-1','Asset A','equipment','THB',0,'2026-01-01','user',0,0),\
         ('asset-b','company','branch-b',NULL,'B-1','Asset B','equipment','THB',0,'2026-01-01','user',0,0),\
         ('asset-cost','company','branch','department','C-1','Cost Asset','equipment','THB',1000,'2026-01-01','user',0,0),\
         ('asset-foreign','company-b','branch-b',NULL,'F-1','Foreign Asset','equipment','THB',0,'2026-01-01','user',0,0);\
         INSERT INTO operational_asset_events(id,asset_id,version,action,actor_id,reason,condition,lifecycle_status,placement,custodian_id,created_at) VALUES\
         ('event-a','asset-a',1,'register','user','Asset registered','good','registered',NULL,NULL,0),\
         ('event-b','asset-b',1,'register','user','Asset registered','good','registered',NULL,NULL,0),\
         ('event-cost','asset-cost',1,'register','user','Asset registered','good','registered',NULL,NULL,0),\
         ('event-foreign','asset-foreign',1,'register','user','Asset registered','good','registered',NULL,NULL,0);\
         UPDATE operational_assets SET finance_asset_id='finance-1',finance_asset_version=1 WHERE id='asset-cost';\
         INSERT INTO operational_asset_finance_links(id,asset_id,finance_asset_id,asset_version,finance_asset_version,cost_minor,currency,reason,created_by,created_at) VALUES\
         ('link-cost','asset-cost','finance-1',1,1,1000,'THB','Fixture link','user',0);",
    )
    .execute(holding.raw())
    .await
    .unwrap();
    Fixture {
        control: control.clone(),
        holding: holding.clone(),
        authorizer,
        clock,
        maker: MAKER_PRODUCT.into(),
        reviewer: REVIEWER_PRODUCT.into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn request(
    method: &str,
    uri: &str,
    token: &str,
    assignment: &str,
    branch: Option<&str>,
    department: Option<&str>,
    origin: Option<&str>,
    key: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "acme.example.test")
        .header("cookie", format!("__Host-erpx-product={token}"))
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", assignment)
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "3");
    if let Some(branch) = branch {
        builder = builder.header("x-erpx-workspace-branch", branch);
    }
    if let Some(department) = department {
        builder = builder.header("x-erpx-workspace-department", department);
    }
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    if let Some(key) = key {
        builder = builder.header("idempotency-key", key);
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .unwrap()
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&body)}))
    };
    (status, value)
}

async fn get(
    app: &Router,
    f: &Fixture,
    query: &str,
    reviewer: bool,
    assignment: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> (StatusCode, Value) {
    call(
        app,
        request(
            "GET",
            &format!("/api/v1/holdings/acme/assets{query}"),
            if reviewer { &f.reviewer } else { &f.maker },
            assignment,
            branch,
            department,
            None,
            None,
            None,
        ),
    )
    .await
}

async fn post(
    app: &Router,
    f: &Fixture,
    body: Value,
    reviewer: bool,
    assignment: &str,
    key: &str,
    origin: Option<&str>,
) -> (StatusCode, Value) {
    call(
        app,
        request(
            "POST",
            "/api/v1/holdings/acme/assets",
            if reviewer { &f.reviewer } else { &f.maker },
            assignment,
            None,
            None,
            origin,
            Some(key),
            Some(body),
        ),
    )
    .await
}

#[tokio::test]
async fn workspace_and_detail_are_company_branch_and_department_scoped() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let f = seed(&control, &holding).await;
            let app = f.router();

            let (status, value) = get(&app, &f, "", false, "assets-grant", None, None).await;
            assert_eq!(status, StatusCode::OK, "{value}");
            assert_eq!(value["context"]["companyId"], "company");
            assert_eq!(value["context"]["assignmentId"], "assets-grant");
            assert_eq!(value["data"]["actorId"], "user");
            assert_eq!(value["data"]["items"].as_array().unwrap().len(), 3);
            assert!(
                value["data"]["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row["company_id"] == "company")
            );
            assert_eq!(value["data"].as_object().unwrap().len(), 7);
            assert_eq!(value["data"]["items"][0].as_object().unwrap().len(), 24);
            assert_eq!(value["context"].as_object().unwrap().len(), 15);

            let (status, value) =
                get(&app, &f, "?id=asset-a", false, "assets-grant", None, None).await;
            assert_eq!(status, StatusCode::OK, "{value}");
            assert_eq!(value["data"]["asset"]["id"], "asset-a");
            assert_eq!(value["data"]["history"].as_array().unwrap().len(), 1);
            assert!(value["data"]["finance"].is_null());

            let (status, value) = get(
                &app,
                &f,
                "?id=asset-foreign",
                false,
                "assets-grant",
                None,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{value}");
            assert_eq!(value["error"]["code"], "ASSET_NOT_FOUND");

            let (status, value) =
                get(&app, &f, "", false, "assets-grant", Some("north"), None).await;
            assert_eq!(status, StatusCode::OK, "{value}");
            assert_eq!(value["data"]["items"].as_array().unwrap().len(), 2);
            assert!(
                value["data"]["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row["branch_id"] == "branch")
            );

            let (status, value) = get(
                &app,
                &f,
                "",
                false,
                "assets-grant",
                Some("north"),
                Some("department"),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{value}");
            assert_eq!(value["data"]["items"].as_array().unwrap().len(), 2);
            assert!(
                value["data"]["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row["department_id"] == "department")
            );

            let (status, value) = get(
                &app,
                &f,
                "?id=asset-b",
                false,
                "assets-grant",
                Some("north"),
                Some("department"),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{value}");
        })
        .await
    })
    .await;
}

#[tokio::test]
async fn http_origin_idempotency_and_replay_are_closed_and_persisted() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let f = seed(&control, &holding).await;
            let app = f.router();
            let input = json!({
                "branchId":"branch",
                "departmentId":"department",
                "code":"NEW-1",
                "name":"Native Asset",
                "category":"equipment",
                "currency":"THB",
                "costMinor":1000,
                "acquiredDate":"2026-02-28"
            });

            let without_key = call(
                &app,
                request(
                    "POST",
                    "/api/v1/holdings/acme/assets",
                    &f.maker,
                    "assets-grant",
                    None,
                    None,
                    Some("https://acme.example.test"),
                    None,
                    Some(input.clone()),
                ),
            )
            .await;
            assert_eq!(without_key.0, StatusCode::UNPROCESSABLE_ENTITY, "{}", without_key.1);

            let unauthenticated = call(
                &app,
                request(
                    "GET",
                    "/api/v1/holdings/acme/assets",
                    "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                    "assets-grant",
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            )
            .await;
            assert_eq!(unauthenticated.0, StatusCode::UNAUTHORIZED, "{}", unauthenticated.1);
            assert_eq!(unauthenticated.1["error"]["code"], "UNAUTHENTICATED");

            let unknown_host = call(
                &app,
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/holdings/acme/assets")
                    .header("host", "unknown.example.test")
                    .header("cookie", format!("__Host-erpx-product={}", f.maker))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(unknown_host.0, StatusCode::NOT_FOUND, "{}", unknown_host.1);

            let wrong_content_type = call(
                &app,
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/holdings/acme/assets")
                    .header("host", "acme.example.test")
                    .header("cookie", format!("__Host-erpx-product={}", f.maker))
                    .header("origin", "https://acme.example.test")
                    .header("idempotency-key", "asset-http-content-type")
                    .header("x-erpx-context-company", "company")
                    .header("x-erpx-context-assignment", "assets-grant")
                    .header("x-erpx-context-revision", "7")
                    .header("x-erpx-context-assignment-version", "3")
                    .header("content-type", "text/plain")
                    .body(Body::from(input.to_string()))
                    .unwrap(),
            )
            .await;
            assert_eq!(wrong_content_type.0, StatusCode::UNPROCESSABLE_ENTITY, "{}", wrong_content_type.1);
            assert_eq!(wrong_content_type.1["error"]["code"], "ASSET_COMMAND_REQUIRED");

            let oversized = call(
                &app,
                request(
                    "POST",
                    "/api/v1/holdings/acme/assets",
                    &f.maker,
                    "assets-grant",
                    None,
                    None,
                    Some("https://acme.example.test"),
                    Some("asset-http-oversized"),
                    Some(json!({"padding":"x".repeat(70 * 1024)})),
                ),
            )
            .await;
            assert_eq!(oversized.0, StatusCode::UNPROCESSABLE_ENTITY, "{}", oversized.1);
            assert_eq!(oversized.1["error"]["code"], "BODY_TOO_LARGE");

            let wrong_origin = post(&app, &f, input.clone(), false, "assets-grant", "asset-http-create-1", Some("https://foreign.example.test")).await;
            assert_eq!(wrong_origin.0, StatusCode::FORBIDDEN, "{}", wrong_origin.1);
            assert_eq!(wrong_origin.1["error"]["code"], "ORIGIN_FORBIDDEN");

            let created = post(&app, &f, input.clone(), false, "assets-grant", "asset-http-create-1", Some("https://acme.example.test")).await;
            assert_eq!(created.0, StatusCode::OK, "{}", created.1);
            assert_eq!(created.1["data"]["id"], "asset-asset-http-create-1");
            assert_eq!(created.1["data"]["version"], 1);
            assert_eq!(created.1["data"]["status"], "registered");

            let replay = post(&app, &f, input.clone(), false, "assets-grant", "asset-http-create-1", Some("https://acme.example.test")).await;
            assert_eq!(replay.0, StatusCode::OK, "{}", replay.1);
            assert_eq!(replay.1["data"], created.1["data"]);

            let conflict = post(
                &app,
                &f,
                json!({
                    "branchId":"branch",
                    "code":"NEW-2",
                    "name":"Different Asset",
                    "category":"equipment",
                    "currency":"THB",
                    "costMinor":0,
                    "acquiredDate":"2026-02-28"
                }),
                false,
                "assets-grant",
                "asset-http-create-1",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(conflict.0, StatusCode::CONFLICT, "{}", conflict.1);
            assert_eq!(conflict.1["error"]["code"], "IDEMPOTENCY_CONFLICT");

            let readback: (String, String, i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT a.code,a.name,a.version,\
                 (SELECT count(*) FROM operational_asset_events e WHERE e.asset_id=a.id),\
                 (SELECT count(*) FROM command_receipts r WHERE r.command_id='asset-http-create-1'),\
                 (SELECT count(*) FROM audit_events u WHERE u.command_id='asset-http-create-1'),\
                 (SELECT count(*) FROM outbox_events o WHERE o.command_id='asset-http-create-1') \
                 FROM operational_assets a WHERE a.id='asset-asset-http-create-1'",
            )
            .fetch_one(holding.raw())
            .await
            .unwrap();
            assert_eq!(readback, ("NEW-1".into(), "Native Asset".into(), 1, 1, 1, 1, 1));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM money_transaction_snapshots WHERE source_type='operational_asset' AND source_id='asset-asset-http-create-1' AND amount_kind='acquisition_cost' AND amount_units=1000"
                )
                .fetch_one(holding.raw())
                .await
                .unwrap(),
                1
            );

            let (status, value) = get(&app, &f, "?id=asset-asset-http-create-1", false, "assets-grant", None, None).await;
            assert_eq!(status, StatusCode::OK, "{value}");
            assert_eq!(value["data"]["asset"]["code"], "NEW-1");
            assert_eq!(value["data"]["asset"]["version"], 1);
        })
        .await
    })
    .await;
}

#[tokio::test]
async fn placement_condition_and_retirement_are_persisted_with_independent_review() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let f = seed(&control, &holding).await;
            let app = f.router();

            let placed = post(
                &app,
                &f,
                json!({
                    "id":"asset-a",
                    "action":"place",
                    "expectedVersion":1,
                    "placement":"HQ desk",
                    "custodianId":"user",
                    "reason":"Assign the current custodian"
                }),
                false,
                "assets-grant",
                "asset-http-place-1",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(placed.0, StatusCode::OK, "{}", placed.1);
            assert_eq!(placed.1["data"]["version"], 2);
            assert_eq!(placed.1["data"]["status"], "in_service");

            let conditioned = post(
                &app,
                &f,
                json!({
                    "id":"asset-a",
                    "action":"condition",
                    "expectedVersion":2,
                    "condition":"degraded",
                    "reason":"Inspection found wear"
                }),
                false,
                "assets-grant",
                "asset-http-condition-1",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(conditioned.0, StatusCode::OK, "{}", conditioned.1);
            assert_eq!(conditioned.1["data"]["version"], 3);

            let requested = post(
                &app,
                &f,
                json!({
                    "id":"asset-a",
                    "action":"retire-request",
                    "expectedVersion":3,
                    "reason":"End of service life"
                }),
                false,
                "assets-grant",
                "asset-http-retire-1",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(requested.0, StatusCode::OK, "{}", requested.1);
            assert_eq!(requested.1["data"]["status"], "retirement_pending");
            assert_eq!(requested.1["data"]["version"], 4);

            let self_review = post(
                &app,
                &f,
                json!({
                    "id":"asset-a",
                    "action":"retire-approve",
                    "expectedVersion":4,
                    "reason":"Approve my own request"
                }),
                false,
                "assets-grant",
                "asset-http-retire-self",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(self_review.0, StatusCode::FORBIDDEN, "{}", self_review.1);
            assert_eq!(self_review.1["error"]["code"], "ASSET_RETIREMENT_REVIEW_INDEPENDENT");

            let retired = post(
                &app,
                &f,
                json!({
                    "id":"asset-a",
                    "action":"retire-approve",
                    "expectedVersion":4,
                    "reason":"Independent disposal approval"
                }),
                true,
                "reviewer-grant",
                "asset-http-retire-approve-1",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(retired.0, StatusCode::OK, "{}", retired.1);
            assert_eq!(retired.1["data"]["status"], "retired");
            assert_eq!(retired.1["data"]["version"], 5);

            let after_retired = post(
                &app,
                &f,
                json!({
                    "id":"asset-a",
                    "action":"condition",
                    "expectedVersion":5,
                    "condition":"good",
                    "reason":"Should be refused"
                }),
                true,
                "reviewer-grant",
                "asset-http-retired-condition",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(after_retired.0, StatusCode::CONFLICT, "{}", after_retired.1);
            assert_eq!(after_retired.1["error"]["code"], "ASSET_RETIRED");

            let row: (String, String, Option<String>, Option<String>, i64) = sqlx::query_as(
                "SELECT lifecycle_status,condition,placement,custodian_id,version FROM operational_assets WHERE id='asset-a'",
            )
            .fetch_one(holding.raw())
            .await
            .unwrap();
            assert_eq!(row, ("retired".into(), "degraded".into(), None, None, 5));
            let events: Vec<(String, i64)> = sqlx::query_as(
                "SELECT action,version FROM operational_asset_events WHERE asset_id='asset-a' ORDER BY version",
            )
            .fetch_all(holding.raw())
            .await
            .unwrap();
            assert_eq!(
                events,
                vec![
                    ("register".into(), 1),
                    ("place".into(), 2),
                    ("condition".into(), 3),
                    ("retire-request".into(), 4),
                    ("retire-approve".into(), 5),
                ]
            );

            let (status, value) = get(&app, &f, "?id=asset-a", true, "reviewer-grant", None, None).await;
            assert_eq!(status, StatusCode::OK, "{value}");
            assert_eq!(value["data"]["asset"]["lifecycle_status"], "retired");
            assert_eq!(value["data"]["history"].as_array().unwrap().len(), 5);
        })
        .await
    })
    .await;
}

#[tokio::test]
async fn stale_versions_disposal_guards_and_foreign_scope_refuse_without_writes() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let f = seed(&control, &holding).await;
            let app = f.router();

            let stale = post(
                &app,
                &f,
                json!({
                    "id":"asset-a",
                    "action":"condition",
                    "expectedVersion":9,
                    "condition":"degraded",
                    "reason":"Stale current version"
                }),
                false,
                "assets-grant",
                "asset-http-stale-1",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);
            assert_eq!(stale.1["error"]["code"], "CONFLICT");

            let wrong_company = call(
                &app,
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/holdings/acme/assets")
                    .header("host", "acme.example.test")
                    .header("cookie", format!("__Host-erpx-product={}", f.maker))
                    .header("x-erpx-context-company", "company-b")
                    .header("x-erpx-context-assignment", "assets-grant")
                    .header("x-erpx-context-revision", "7")
                    .header("x-erpx-context-assignment-version", "3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(wrong_company.0, StatusCode::CONFLICT, "{}", wrong_company.1);
            assert_eq!(wrong_company.1["error"]["code"], "WORK_CONTEXT_CHANGED");

            let wrong_assignment = call(
                &app,
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/holdings/acme/assets")
                    .header("host", "acme.example.test")
                    .header("cookie", format!("__Host-erpx-product={}", f.maker))
                    .header("x-erpx-context-company", "company")
                    .header("x-erpx-context-assignment", "foreign-grant")
                    .header("x-erpx-context-revision", "7")
                    .header("x-erpx-context-assignment-version", "3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(wrong_assignment.0, StatusCode::CONFLICT, "{}", wrong_assignment.1);
            assert_eq!(wrong_assignment.1["error"]["code"], "WORK_CONTEXT_CHANGED");

            let foreign = post(
                &app,
                &f,
                json!({
                    "id":"asset-foreign",
                    "action":"condition",
                    "expectedVersion":1,
                    "condition":"degraded",
                    "reason":"Foreign company asset"
                }),
                false,
                "assets-grant",
                "asset-http-foreign-1",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(foreign.0, StatusCode::NOT_FOUND, "{}", foreign.1);
            assert_eq!(foreign.1["error"]["code"], "ASSET_NOT_FOUND");

            let branch_outsider = call(
                &app,
                request(
                    "POST",
                    "/api/v1/holdings/acme/assets",
                    &f.maker,
                    "assets-grant",
                    Some("north"),
                    None,
                    Some("https://acme.example.test"),
                    Some("asset-http-branch-outsider"),
                    Some(json!({
                        "id":"asset-b",
                        "action":"condition",
                        "expectedVersion":1,
                        "condition":"degraded",
                        "reason":"Outside branch scope"
                    })),
                ),
            )
            .await;
            assert_eq!(branch_outsider.0, StatusCode::NOT_FOUND, "{}", branch_outsider.1);

            let request_disposal = post(
                &app,
                &f,
                json!({
                    "id":"asset-cost",
                    "action":"retire-request",
                    "expectedVersion":1,
                    "reason":"Retire the costed asset"
                }),
                false,
                "assets-grant",
                "asset-http-cost-request",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(request_disposal.0, StatusCode::OK, "{}", request_disposal.1);
            let no_disposal = post(
                &app,
                &f,
                json!({
                    "id":"asset-cost",
                    "action":"retire-approve",
                    "expectedVersion":2,
                    "reason":"Approve without Finance disposal"
                }),
                true,
                "reviewer-grant",
                "asset-http-cost-approve",
                Some("https://acme.example.test"),
            )
            .await;
            assert_eq!(no_disposal.0, StatusCode::CONFLICT, "{}", no_disposal.1);
            assert_eq!(no_disposal.1["error"]["code"], "ASSET_FINANCE_DISPOSAL_REQUIRED");

            let rows: (String, i64, i64, i64) = sqlx::query_as(
                "SELECT lifecycle_status,version,\
                 (SELECT count(*) FROM command_receipts WHERE command_id='asset-http-stale-1'),\
                 (SELECT count(*) FROM command_receipts WHERE command_id='asset-http-branch-outsider') \
                 FROM operational_assets WHERE id='asset-a'",
            )
            .fetch_one(holding.raw())
            .await
            .unwrap();
            assert_eq!(rows, ("registered".into(), 1, 0, 0));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM operational_asset_events WHERE asset_id='asset-a'"
                )
                .fetch_one(holding.raw())
                .await
                .unwrap(),
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT lifecycle_status FROM operational_assets WHERE id='asset-cost'"
                )
                .fetch_one(holding.raw())
                .await
                .unwrap(),
                "retirement_pending"
            );
        })
        .await
    })
    .await;
}
