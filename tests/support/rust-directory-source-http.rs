//! In-process actual native login/SSO/MFA and admin router. Work/role rows are
//! explicit published component seed facts, not an HTTP Actor or default policy.
#[allow(
    dead_code,
    reason = "shared actual Account/SSO/private-profile fixture"
)]
#[path = "rust-directory-source-admin.rs"]
pub mod admin;
use admin::security;
pub use admin::{account, ldap};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode, Uri, header},
};
use erpx_server_identity_access::public::{
    account_security_http::account_security_router, account_sso::*, *,
};
use erpx_server_shared::public::{BusinessError, Clock};
use erpx_service::{business_http::BusinessScopeSlugs, directory_source_http::*};
pub use security::http::Jar;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
pub struct HttpFixture {
    pub p: admin::Prepared,
    pub router: Router,
    pub account_a: Jar,
    pub account_b: Jar,
    pub holding_a: Jar,
    pub holding_b: Jar,
    pub child_a: String,
    pub child_b: String,
}
struct CompanyRoute;
impl BusinessScopeSlugs for CompanyRoute {
    fn slugs(&self, _: &HeaderMap) -> Result<(Option<String>, Option<String>), BusinessError> {
        Ok((None, None))
    }
}
pub fn router(p: &admin::Prepared) -> Router {
    let policy = account::fixture::account::policy();
    let sso_policy = SsoPolicy::new(policy.clone()).unwrap();
    let sso = Arc::new(AccountSso::new(
        sso_policy.clone(),
        Arc::new(PostgresAccountSso::from_control_database(
            p.system.control.clone(),
            sso_policy,
            p.system.scenario.clock.clone(),
            account::fixture::account::holding_registry(),
        )),
        Arc::new(SystemSsoCryptography),
        Arc::new(SystemAuthenticationSecrets),
        p.system.scenario.clock.clone(),
    ));
    let security = security::services_with_keys(&p.system, p.keys.clone());
    account_auth_router(p.system.scenario.auth.clone())
        .merge(account_sso_router(sso))
        .merge(account_security_router(security.service))
        .merge(
            directory_source_router(
                Arc::new(policy),
                account::fixture::account::holding_registry(),
                Arc::new(CompanyRoute),
                vec![DirectorySourceHttpTarget {
                    host: admin::HOST.into(),
                    holding_slug: "acme".into(),
                    administration: Arc::new(p.service()),
                }],
            )
            .unwrap(),
        )
}
#[allow(
    clippy::too_many_arguments,
    reason = "in-process HTTP fixture explicitly supplies host, cookies, method, path, Origin, body and headers"
)]
pub async fn send(
    router: &Router,
    host: &str,
    jar: &mut Jar,
    method: &str,
    path: &str,
    origin: Option<&str>,
    body: Vec<u8>,
    extra: HeaderMap,
) -> (StatusCode, Value, HeaderMap) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", host)
        .header("cookie", jar.header());
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    let mut request = request
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    for (name, value) in &extra {
        request.headers_mut().append(name, value.clone());
    }
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    jar.apply(&headers);
    let bytes = to_bytes(response.into_body(), 32 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value, headers)
}
pub async fn login_and_sso(router: &Router, email: &str) -> (Jar, Jar, String) {
    let mut account = Jar::default();
    let (status, _, headers) = send(
        router,
        "account.example.test",
        &mut account,
        "POST",
        "/api/auth/sign-in/email",
        Some("https://account.example.test"),
        serde_json::to_vec(&json!({"email":email,"password":account::fixture::PASSWORD})).unwrap(),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.get_all(header::SET_COOKIE).iter().all(|v| {
        v.to_str()
            .unwrap()
            .contains("Secure; HttpOnly; SameSite=Lax")
    }));
    let mut holding = Jar::default();
    let (status, _, headers) = send(
        router,
        admin::HOST,
        &mut holding,
        "GET",
        "/sso/start?next=%2Fiam%2Fconnections",
        None,
        vec![],
        HeaderMap::new(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let authorize: Uri = headers[header::LOCATION].to_str().unwrap().parse().unwrap();
    assert_eq!(authorize.host(), Some("account.example.test"));
    let (status, _, headers) = send(
        router,
        "account.example.test",
        &mut account,
        "GET",
        authorize.path_and_query().unwrap().as_str(),
        None,
        vec![],
        HeaderMap::new(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let callback: Uri = headers[header::LOCATION].to_str().unwrap().parse().unwrap();
    assert_eq!(callback.host(), Some(admin::HOST));
    let (status, _, headers) = send(
        router,
        admin::HOST,
        &mut holding,
        "GET",
        callback.path_and_query().unwrap().as_str(),
        None,
        vec![],
        HeaderMap::new(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers[header::LOCATION],
        "https://acme.example.test/iam/connections"
    );
    assert!(
        headers
            .get_all(header::SET_COOKIE)
            .iter()
            .all(|v| !v.to_str().unwrap().contains("Domain="))
    );
    let (status, session, _) = send(
        router,
        admin::HOST,
        &mut holding,
        "GET",
        "/api/auth/get-session",
        None,
        vec![],
        HeaderMap::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = session["session"]["id"].as_str().unwrap().to_owned();
    (account, holding, id)
}
pub async fn mfa(p: &admin::Prepared, router: &Router, jar: &mut Jar, user: &str) {
    let (status, value, _) = send(
        router,
        admin::HOST,
        jar,
        "POST",
        "/api/v1/account/iam/mfa/enroll",
        Some("https://acme.example.test"),
        b"{}".to_vec(),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        value["data"]["secret"]
            .as_str()
            .is_some_and(|s| s.len() == 32)
    );
    let code = security::code(&p.system, &p.keys, user, 0).await;
    let (status, value, _) = send(
        router,
        admin::HOST,
        jar,
        "POST",
        "/api/v1/account/iam/mfa/verify",
        Some("https://acme.example.test"),
        serde_json::to_vec(&json!({"code":code})).unwrap(),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(value["data"]["verifiedUntil"].as_i64().unwrap() > p.system.scenario.clock.now().get());
}
pub async fn prepare(system: account::System, verify: bool) -> HttpFixture {
    let p = admin::prepare_for_http(system).await;
    let router = router(&p);
    let (account_a, mut holding_a, child_a) = login_and_sso(&router, "a@example.test").await;
    let (account_b, mut holding_b, child_b) = login_and_sso(&router, "b@example.test").await;
    for (session, user, assignment) in [
        (&child_a, "fixture-a", "source-admin-a"),
        (&child_b, "fixture-b", "source-admin-b"),
    ] {
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,$2,'company',$3,3,7,$4,$5)").bind(session).bind(user).bind(assignment).bind(p.system.scenario.clock.now().get()+3_600_000).bind(p.system.scenario.clock.now().get()).execute(p.system.holdings[0].raw()).await.unwrap();
    }
    if verify {
        mfa(&p, &router, &mut holding_a, "fixture-a").await;
        mfa(&p, &router, &mut holding_b, "fixture-b").await;
    }
    HttpFixture {
        p,
        router,
        account_a,
        account_b,
        holding_a,
        holding_b,
        child_a,
        child_b,
    }
}
pub fn headers(reviewer: bool, key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("x-erpx-context-company", "company"),
        ("x-erpx-context-revision", "7"),
        (
            "x-erpx-context-assignment",
            if reviewer {
                "source-admin-b"
            } else {
                "source-admin-a"
            },
        ),
        ("x-erpx-context-assignment-version", "3"),
        ("idempotency-key", key),
    ] {
        headers.insert(name, value.parse().unwrap());
    }
    headers
}
pub fn preview_path() -> String {
    format!(
        "/api/v1/holdings/acme/iam/connections/{}/source/preview",
        ldap::CONNECTION
    )
}
pub fn approve_path(review: &str) -> String {
    format!(
        "/api/v1/holdings/acme/iam/connections/{}/source/reviews/{review}/approve",
        ldap::CONNECTION
    )
}
pub fn preview_body() -> Value {
    json!({"keyId":"synthetic_provenance_v1","expectedConnectionVersion":1,"reason":"Reviewed exact source proposal"})
}
