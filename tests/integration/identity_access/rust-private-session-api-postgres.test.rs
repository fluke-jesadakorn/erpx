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
