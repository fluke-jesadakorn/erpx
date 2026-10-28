#[allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod finance_facts;
#[allow(dead_code)]
#[path = "../../support/runtime/private-session-config.rs"]
mod fixture;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::SystemAuthenticationSecrets;
use erpx_service::private_development_api::*;
use finance_facts::account;
use fixture::cookie;
async fn document(system: &account::System) -> serde_json::Value {
    fixture::document(&system.control, &system.holdings).await
}
use std::sync::Arc;
use tower::ServiceExt;
async fn call(
    router: &Router,
    host: &str,
    cookie: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder()
        .uri("/api/auth/get-session")
        .header("host", host);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie)
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert!(response.headers().get("set-cookie").is_none());
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 32768).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8(bytes.to_vec()).unwrap())
        }),
    )
}
#[tokio::test]
async fn assembled_router_uses_real_account_and_both_sso_children_and_current_holding_bindings_without_listener()
 {
    account::with_system(|system| async move {
        let central = account::login(&system, "a@example.test", None).await;
        let first = account::child(&system, &central, account::ACME).await;
        let second = account::child(&system, &central, account::NORTHSTAR).await;
        let config =
            DevelopmentApiConfiguration::parse(&document(&system).await.to_string()).unwrap();
        let runtime = assemble_development_api(
            config,
            system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
        )
        .await
        .unwrap();
        let router = runtime.router();
        let (status, body) = call(&router, "account.example.test", Some(&cookie(&central))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["id"], "fixture-a");
        assert!(body.get("actor").is_none());
        assert!(body["session"].get("permissions").is_none());
        for (host, issued) in [
            ("acme.example.test", &first),
            ("northstar.example.test", &second),
        ] {
            let (status, body) = call(
                &router,
                host,
                Some(&format!("__Host-erpx-product={}", issued.token.expose())),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["session"]["id"], issued.result.session_id);
        }
        sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'")
            .execute(system.control.raw())
            .await
            .unwrap();
        assert_eq!(
            call(
                &router,
                "acme.example.test",
                Some(&format!("__Host-erpx-product={}", first.token.expose()))
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(&router, "account.example.test", Some(&cookie(&central)))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(
                &router,
                "northstar.example.test",
                Some(&format!("__Host-erpx-product={}", second.token.expose()))
            )
            .await
            .0,
            StatusCode::OK
        );
        runtime.close().await;
    })
    .await;
}
#[tokio::test]
async fn session_only_router_keeps_anonymous_null_and_closed_host_proxy_method_cookie_and_readiness_boundaries()
 {
    account::with_system(|system| async move {
        let config =
            DevelopmentApiConfiguration::parse(&document(&system).await.to_string()).unwrap();
        let runtime = assemble_development_api(
            config,
            system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
        )
        .await
        .unwrap();
        let router = runtime.router();
        assert_eq!(
            call(&router, "account.example.test", None).await,
            (StatusCode::OK, serde_json::Value::Null)
        );
        assert_eq!(
            call(&router, "acme.example.test", None).await,
            (StatusCode::OK, serde_json::Value::Null)
        );
        assert_eq!(
            call(&router, "retired.example.test", None).await.0,
            StatusCode::NOT_FOUND
        );
        for (method, path, status) in [
            ("POST", "/api/auth/sign-in/email", StatusCode::NOT_FOUND),
            ("POST", "/api/auth/sign-out", StatusCode::NOT_FOUND),
            ("POST", "/api/auth/get-session", StatusCode::NOT_FOUND),
            (
                "GET",
                "/api/v1/account/device-accounts",
                StatusCode::NOT_FOUND,
            ),
            ("GET", "/api/health", StatusCode::OK),
            ("GET", "/api/ready", StatusCode::SERVICE_UNAVAILABLE),
        ] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header("host", "account.example.test")
                .body(Body::empty())
                .unwrap();
            let response = router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), status, "{method}/{path}");
            if path == "/api/ready" {
                let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
                let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(body["acceptance"], false);
                assert_eq!(body["error"]["code"], "RUST_MIGRATION_INCOMPLETE");
            }
        }
        for key in [
            "forwarded",
            "x-forwarded-host",
            "x-forwarded-proto",
            "x-forwarded-port",
            "x-original-host",
        ] {
            let request = Request::builder()
                .uri("/api/auth/get-session")
                .header("host", "account.example.test")
                .header(key, "untrusted")
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                router.clone().oneshot(request).await.unwrap().status(),
                StatusCode::FORBIDDEN
            );
        }
        let request = Request::builder()
            .uri("/api/auth/get-session")
            .header("host", "account.example.test")
            .header("host", "account.example.test")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router.clone().oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(
                &router,
                "account.example.test",
                Some("unrelated=a; unrelated=b")
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        runtime.close().await;
    })
    .await;
}
#[tokio::test]
async fn assembly_refuses_wrong_tracks_missing_changed_extra_receipts_and_stale_registry_without_auto_repair()
 {
    account::with_system(|system|async move{
  let original=document(&system).await;let mut swapped=original.clone();swapped["control"]=original["holdings"][0]["database"].clone();swapped["holdings"][0]["database"]=original["control"].clone();
  let config=DevelopmentApiConfiguration::parse(&swapped.to_string()).unwrap();assert!(matches!(assemble_development_api(config,system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await,Err("PRIVATE_API_SCHEMA_MISMATCH")));
  let (name,checksum):(String,String)=sqlx::query_as("SELECT name,checksum FROM erpx_migration_receipts() ORDER BY name LIMIT 1").fetch_one(system.control.raw()).await.unwrap();
  // These metadata mutations touch only the exact leased disposable fixture.
  // They are restored explicitly by the test, never by bootstrap.
  for mode in ["missing","checksum","extra"]{
   match mode{"missing"=>{sqlx::query("DELETE FROM erpx_migrations WHERE name=$1").bind(&name).execute(system.control.raw()).await.unwrap();},"checksum"=>{sqlx::query("UPDATE erpx_migrations SET checksum=repeat('a',64)WHERE name=$1").bind(&name).execute(system.control.raw()).await.unwrap();},_=>{sqlx::query("INSERT INTO erpx_migrations(name,checksum)VALUES('unknown-test.sql',repeat('b',64))").execute(system.control.raw()).await.unwrap();}}
   let config=DevelopmentApiConfiguration::parse(&original.to_string()).unwrap();assert!(matches!(assemble_development_api(config,system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await,Err("PRIVATE_API_SCHEMA_MISMATCH")),"{mode}");
   match mode{"missing"=>{assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM erpx_migrations WHERE name=$1").bind(&name).fetch_one(system.control.raw()).await.unwrap(),0);sqlx::query("INSERT INTO erpx_migrations(name,checksum)VALUES($1,$2)").bind(&name).bind(&checksum).execute(system.control.raw()).await.unwrap();},"checksum"=>{assert_eq!(sqlx::query_scalar::<_,String>("SELECT checksum FROM erpx_migrations WHERE name=$1").bind(&name).fetch_one(system.control.raw()).await.unwrap(),"a".repeat(64));sqlx::query("UPDATE erpx_migrations SET checksum=$1 WHERE name=$2").bind(&checksum).bind(&name).execute(system.control.raw()).await.unwrap();},_=>{assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM erpx_migrations WHERE name='unknown-test.sql'").fetch_one(system.control.raw()).await.unwrap(),1);sqlx::query("DELETE FROM erpx_migrations WHERE name='unknown-test.sql'").execute(system.control.raw()).await.unwrap();}}
  }
  sqlx::query("UPDATE holding_bindings SET write_binding='MOVED_H001_DB'WHERE holding_id='h001'").execute(system.control.raw()).await.unwrap();let config=DevelopmentApiConfiguration::parse(&original.to_string()).unwrap();assert!(matches!(assemble_development_api(config,system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await,Err("PRIVATE_API_BINDINGS_MISMATCH")));
 }).await;
}

#[tokio::test]
async fn optional_finance_workspace_uses_real_account_and_holding_authority() {
    finance_facts::account::with_system(|system| async move {
        let prepared = finance_facts::setup(system).await;
        prepared.create("private-workspace", false).await.unwrap();
        let system = &prepared.system;
        let mut doc = serde_json::json!({
            "version":1,"mode":"development","transport":"direct-loopback","listen":"127.0.0.1:43117",
            "accountHost":"account.example.test","requireEmailVerification":true,"passwordWorkers":1,
            "maximumRequests":8,"requestTimeoutMs":5000,
            "control":fixture::database(&system.control).await,
            "holdings":[
                {"holdingId":"h001","slug":"acme","host":"acme.example.test","writeBinding":"HOLDING_H001_DB","readBinding":"HOLDING_H001_READ_DB","filesBinding":"HOLDING_H001_FILES","database":fixture::database(&system.holdings[0]).await},
                {"holdingId":"h002","slug":"northstar","host":"northstar.example.test","writeBinding":"HOLDING_H002_DB","readBinding":"HOLDING_H002_READ_DB","filesBinding":"HOLDING_H002_FILES","database":fixture::database(&system.holdings[1]).await}
            ]
        });
        let config = DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap();
        let runtime = assemble_development_api(config, system.scenario.clock.clone(), Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let path = "/api/v1/holdings/acme/finance/journals/journal-private-workspace/workspace";
        let request = || Request::builder().uri(path).header("host", "acme.example.test").header("cookie", format!("__Host-erpx-product={}", prepared.reviewer_token.expose())).body(Body::empty()).unwrap();
        assert_eq!(runtime.router().oneshot(request()).await.unwrap().status(), StatusCode::NOT_FOUND);
        runtime.close().await;
        doc["financeJournalReads"] = serde_json::json!([{ "holdingId":"h001", "companyId":"company" }]);
        let config = DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap();
        let runtime = assemble_development_api(config, system.scenario.clock.clone(), Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let response = runtime.router().oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["context"]["holdingId"], "h001");
        assert_eq!(body["context"]["companyId"], "company");
        let anonymous = Request::builder().uri(path).header("host", "acme.example.test").body(Body::empty()).unwrap();
        assert_ne!(runtime.router().oneshot(anonymous).await.unwrap().status(), StatusCode::OK);
        let other_host = Request::builder().uri(path).header("host", "northstar.example.test").body(Body::empty()).unwrap();
        assert_eq!(runtime.router().oneshot(other_host).await.unwrap().status(), StatusCode::NOT_FOUND);
        let detail = Request::builder().uri("/api/v1/holdings/acme/finance/journals/journal-private-workspace").header("host", "acme.example.test").body(Body::empty()).unwrap();
        assert_eq!(runtime.router().oneshot(detail).await.unwrap().status(), StatusCode::NOT_FOUND);
        runtime.close().await;
    }).await;
}

#[tokio::test]
async fn opt_in_account_bootstrap_performs_real_http_sign_in_and_sso_with_closed_boundaries() {
    account::with_system(|system| async move {
        let mut doc = document(&system).await;
        doc["financeJournalReads"] = serde_json::json!([{ "holdingId":"h001", "companyId":"company" }]);
        doc["accountBootstrap"] = true.into();
        let runtime = assemble_development_api(
            DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),
            system.scenario.clock.clone(), Arc::new(SystemAuthenticationSecrets),
        ).await.unwrap();
        let router = runtime.router();
        let sign_in = || Request::builder().method("POST").uri("/api/auth/sign-in/email")
            .header("host", "account.example.test").header("origin", "https://account.example.test")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({"email":"a@example.test", "password":account::fixture::PASSWORD}).to_string())).unwrap();
        let response = router.clone().oneshot(sign_in()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let account_cookies = response.headers().get_all("set-cookie").iter()
            .map(|v| v.to_str().unwrap().split(';').next().unwrap()).collect::<Vec<_>>().join("; ");
        let start = router.clone().oneshot(Request::builder().uri("/sso/start?next=%2Ffinance")
            .header("host", "acme.example.test").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(start.status(), StatusCode::SEE_OTHER);
        let browser = start.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_owned();
        let location = start.headers()["location"].to_str().unwrap().strip_prefix("https://account.example.test").unwrap();
        let authorized = router.clone().oneshot(Request::builder().uri(location).header("host", "account.example.test")
            .header("cookie", &account_cookies).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(authorized.status(), StatusCode::SEE_OTHER);
        let callback = authorized.headers()["location"].to_str().unwrap().strip_prefix("https://acme.example.test").unwrap();
        let issued = router.clone().oneshot(Request::builder().uri(callback).header("host", "acme.example.test")
            .header("cookie", browser).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(issued.status(), StatusCode::SEE_OTHER);
        let child_cookie = issued.headers().get_all("set-cookie").iter().map(|v| v.to_str().unwrap().split(';').next().unwrap())
            .find(|v| v.starts_with("__Host-erpx-product=")).unwrap();
        assert_eq!(call(&router, "acme.example.test", Some(child_cookie)).await.0, StatusCode::OK);
        for (method, host, path) in [
            ("POST", "acme.example.test", "/api/auth/sign-in/email"),
            ("GET", "northstar.example.test", "/sso/start"),
            ("GET", "acme.example.test", "/sso/authorize"),
            ("POST", "account.example.test", "/api/auth/sign-out"),
            ("HEAD", "acme.example.test", "/sso/start"),
        ] {
            let request = Request::builder().method(method).uri(path).header("host",host).body(Body::empty()).unwrap();
            assert_eq!(router.clone().oneshot(request).await.unwrap().status(), StatusCode::NOT_FOUND, "{method} {host} {path}");
        }
        let missing_origin = Request::builder().method("POST").uri("/api/auth/sign-in/email").header("host","account.example.test")
            .header("content-type","application/json").body(Body::from("{}" )).unwrap();
        assert_eq!(router.oneshot(missing_origin).await.unwrap().status(),StatusCode::FORBIDDEN);
        runtime.close().await;
    }).await;
}

#[tokio::test]
async fn opt_in_journal_post_enforces_current_workflow_and_replays_once() {
    finance_facts::account::with_system(|system| async move {
        let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(wall, std::sync::atomic::Ordering::SeqCst);
        let prepared = finance_facts::setup(system).await;
        prepared.create("private-post", false).await.unwrap();
        let system = &prepared.system;
        let db = &system.holdings[0];
        let now = erpx_server_shared::public::Clock::now(system.scenario.clock.as_ref()).get();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','journal_flow_reviewer','Journal workflow reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','journal_flow_reviewer',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"workflow.finance_approver\"]','[]','[\"holding\"]','fixture',0);UPDATE assignments SET role='journal_flow_reviewer' WHERE id='finance-fixture-b';INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('person-reviewer','fixture-b','company','Reviewer','TH','active','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('reviewer-engagement','person-reviewer','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0)").execute(db.raw()).await.unwrap();
        let mut doc=fixture::document(&system.control,&system.holdings).await;
        doc["financeJournalReads"]=serde_json::json!([{ "holdingId":"h001", "companyId":"company" }]);
        doc["financeJournalPosts"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(), system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let path="/api/v1/holdings/acme/finance/journals/journal-private-post/post";
        for (host, path) in [("northstar.example.test",path),("acme.example.test","/api/v1/holdings/acme/finance/journals/journal-private-post/reverse")] {
            let closed=Request::builder().method("POST").uri(path).header("host",host).body(Body::empty()).unwrap();
            assert_eq!(runtime.router().oneshot(closed).await.unwrap().status(),StatusCode::NOT_FOUND);
        }

        let request=|key: &str, version: i64, revision: i64| Request::builder().method("POST").uri(path).header("host","acme.example.test").header("origin","https://acme.example.test")
            .header("cookie",format!("__Host-erpx-product={}",prepared.reviewer_token.expose()))
            .header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-revision",revision.to_string()).header("x-erpx-context-assignment-version","3").header("idempotency-key",key)
            .body(Body::from(serde_json::json!({"expectedVersion":version}).to_string())).unwrap();
        sqlx::raw_sql("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status)VALUES('company','fixture-a','principal-maker','same-person','active'),('company','fixture-b','principal-reviewer','same-person','active')").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("same-person",1,7)).await.unwrap().status(),StatusCode::FORBIDDEN);
        let self_review:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='same-person'").fetch_one(db.raw()).await.unwrap();assert_eq!(self_review,0);
        sqlx::query("DELETE FROM iam_members WHERE company_id='company' AND user_id IN ('fixture-a','fixture-b')").execute(db.raw()).await.unwrap();
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('reviewer-c-assignment','fixture-c','journal_flow_reviewer','[\"finance\"]','company',$1,$2,1,'holding')").bind(now-1000).bind(now+3_600_000).execute(db.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('person-reviewer-c','fixture-c','company','Other Reviewer','TH','active','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('reviewer-c-engagement','person-reviewer-c','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status)VALUES('company','fixture-a','principal-maker','creator-person','active'),('company','fixture-c','principal-alias','creator-person','active')").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("alias-pool",1,7)).await.unwrap().status(),StatusCode::CONFLICT);
        let alias_receipts:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='alias-pool'").fetch_one(db.raw()).await.unwrap();assert_eq!(alias_receipts,0);
        sqlx::raw_sql("DELETE FROM iam_members WHERE company_id='company' AND user_id IN ('fixture-a','fixture-c');UPDATE hr_engagements SET status='ended' WHERE id='reviewer-c-engagement';INSERT INTO flow_routes(company_id,kind,record_id,defaults_revision,steps_json,created_at)VALUES('company','journal','journal-private-post',0,'{\"draft\":{\"primaryUserId\":42}}',0)").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("malformed-primary",1,7)).await.unwrap().status(),StatusCode::CONFLICT);
        let malformed:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='malformed-primary'").fetch_one(db.raw()).await.unwrap();assert_eq!(malformed,0);
        sqlx::query("UPDATE flow_routes SET steps_json='null' WHERE company_id='company' AND kind='journal' AND record_id='journal-private-post'").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("null-route",1,7)).await.unwrap().status(),StatusCode::INTERNAL_SERVER_ERROR);
        sqlx::raw_sql("DELETE FROM flow_routes WHERE company_id='company' AND kind='journal' AND record_id='journal-private-post';UPDATE hr_engagements SET status='active' WHERE id='reviewer-c-engagement';INSERT INTO flow_routes(company_id,kind,record_id,defaults_revision,steps_json,created_at)VALUES('company','journal','journal-private-post',0,'{\"draft\":{\"primaryUserId\":\"fixture-c\",\"teamName\":\"Company\",\"defaultRevision\":0}}',0)").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("not-primary",1,7)).await.unwrap().status(),StatusCode::FORBIDDEN);
        let denied:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='not-primary'").fetch_one(db.raw()).await.unwrap();assert_eq!(denied,0);
        sqlx::raw_sql("DELETE FROM flow_routes WHERE company_id='company' AND kind='journal' AND record_id='journal-private-post';UPDATE hr_engagements SET status='ended' WHERE id='reviewer-c-engagement'").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("stale-role",1,8)).await.unwrap().status(),StatusCode::CONFLICT);
        assert_eq!(runtime.router().oneshot(request("stale-version",2,7)).await.unwrap().status(),StatusCode::CONFLICT);
        sqlx::query("ALTER TABLE flow_events ADD CONSTRAINT fixture_reject_flow CHECK (command_id <> 'rollback-flow')").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("rollback-flow",1,7)).await.unwrap().status(),StatusCode::INTERNAL_SERVER_ERROR);
        let rollback:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_events WHERE command_id='rollback-flow'),(SELECT count(*) FROM flow_routes WHERE record_id='journal-private-post'),(SELECT count(*) FROM command_receipts WHERE command_id='rollback-flow'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_id='journal-private-post')").fetch_one(db.raw()).await.unwrap();
        assert_eq!(rollback,(0,0,0,0));
        sqlx::query("ALTER TABLE flow_events DROP CONSTRAINT fixture_reject_flow").execute(db.raw()).await.unwrap();
        sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT fixture_reject_audit CHECK (command_id <> 'rollback-audit')").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("rollback-audit",1,7)).await.unwrap().status(),StatusCode::INTERNAL_SERVER_ERROR);
        let late:(String,i64,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT status FROM finance_journals WHERE id='journal-private-post'),(SELECT version FROM finance_journals WHERE id='journal-private-post'),(SELECT count(*) FROM flow_events WHERE command_id='rollback-audit'),(SELECT count(*) FROM flow_routes WHERE record_id='journal-private-post'),(SELECT count(*) FROM command_receipts WHERE command_id='rollback-audit'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_id='journal-private-post'),(SELECT count(*) FROM audit_events WHERE command_id='rollback-audit'),(SELECT count(*) FROM outbox_events WHERE command_id='rollback-audit')").fetch_one(db.raw()).await.unwrap();
        assert_eq!(late,("draft".into(),1,0,0,0,0,0,0));
        sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT fixture_reject_audit").execute(db.raw()).await.unwrap();
        let response=runtime.router().oneshot(request("private-post-review",1,7)).await.unwrap();
        let status=response.status(); let bytes=to_bytes(response.into_body(),65536).await.unwrap();
        assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        let response:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(response["data"]["status"],"posted");assert_eq!(response["data"]["version"],2);
        sqlx::query("UPDATE flow_routes SET steps_json='{\"draft\":{\"primaryUserId\":\"fixture-c\"}}',version=version+1 WHERE company_id='company' AND kind='journal' AND record_id='journal-private-post'").execute(db.raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("private-post-review",1,7)).await.unwrap().status(),StatusCode::OK);
        let read=runtime.router().oneshot(Request::builder().uri("/api/v1/holdings/acme/finance/journals/journal-private-post/workspace").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",prepared.reviewer_token.expose())).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(read.status(),StatusCode::OK);
        let read:serde_json::Value=serde_json::from_slice(&to_bytes(read.into_body(),2*1024*1024).await.unwrap()).unwrap();
        assert_eq!(read["data"]["journal"]["status"],"posted");assert_eq!(read["data"]["journal"]["version"],2);
        let totals:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_events WHERE command_id='private-post-review'),(SELECT count(*) FROM command_receipts WHERE command_id='private-post-review'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_id='journal-private-post'),(SELECT count(*) FROM audit_events WHERE command_id='private-post-review'),(SELECT count(*) FROM outbox_events WHERE command_id='private-post-review')").fetch_one(db.raw()).await.unwrap();
        assert_eq!(totals,(1,1,1,1,1));
        let event:(String,String,i64,Option<String>,Option<String>)=sqlx::query_as("SELECT step_key,action,record_version,next_step_key,next_user_id FROM flow_events WHERE command_id='private-post-review'").fetch_one(db.raw()).await.unwrap();
        assert_eq!(event,("draft".into(),"post".into(),2,Some("posted".into()),None));
        assert!(now>0);
        runtime.close().await;
    }).await;
}
