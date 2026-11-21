#[allow(
    dead_code,
    reason = "shared actual Account/SSO fixtures support multiple native suites"
)]
#[path = "rust-account-revocation.rs"]
pub mod revocation;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode, header},
};
use erpx_server_identity_access::public::{OpaqueSecret, PresentedSecrets};
use revocation as fixture;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tower::ServiceExt;
pub const ACCOUNT: &str = "https://account.example.test";
pub const ACME: &str = "https://acme.example.test";
pub const NORTHSTAR: &str = "https://northstar.example.test";
pub const DEVICE: &str = "/api/v1/account/device-accounts";
pub const ACTIVE: &str = "__Secure-erpx-account.session_token";
pub const GROUP: &str = "__Host-erpx-device";
pub const SAVED: &str = "__Secure-erpx-account.session_token_multi-";
#[derive(Clone, Default)]
pub struct Jar(pub BTreeMap<String, String>);
impl Jar {
    pub fn header(&self) -> String {
        self.0
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
    pub fn apply(&mut self, headers: &HeaderMap) {
        for value in headers.get_all(header::SET_COOKIE) {
            let value = value.to_str().unwrap();
            let pair = value.split(';').next().unwrap();
            let (name, secret) = pair.split_once('=').unwrap();
            if secret.is_empty() || value.contains("Max-Age=0") {
                self.0.remove(name);
            } else {
                self.0.insert(name.into(), secret.into());
            }
        }
    }
    pub fn proof(&self) -> PresentedSecrets {
        PresentedSecrets {
            browser: self
                .0
                .get(GROUP)
                .map(|v| OpaqueSecret::new(v.clone()).unwrap()),
            active: self
                .0
                .get(ACTIVE)
                .map(|v| OpaqueSecret::new(v.clone()).unwrap()),
            saved: self
                .0
                .iter()
                .filter_map(|(name, value)| {
                    name.strip_prefix(SAVED)
                        .map(|handle| (handle.into(), OpaqueSecret::new(value.clone()).unwrap()))
                })
                .collect(),
        }
    }
}
pub async fn send(
    router: &Router,
    jar: &mut Jar,
    method: &str,
    path: &str,
    origin: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value, HeaderMap) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "account.example.test")
        .header("cookie", jar.header());
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    let bytes = if let Some(body) = body {
        request = request.header("content-type", "application/json");
        serde_json::to_vec(&body).unwrap()
    } else {
        vec![]
    };
    let response = router
        .clone()
        .oneshot(request.body(Body::from(bytes)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    jar.apply(&headers);
    let bytes = to_bytes(response.into_body(), 32 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value, headers)
}
pub async fn sign_in(
    router: &Router,
    jar: &mut Jar,
    email: &str,
    revision: Option<i64>,
    origin: &str,
) -> Value {
    let mut input = json!({"email":email,"password":fixture::fixture::PASSWORD,"callbackURL":"https://unrelated.example.test/never-forward"});
    if let Some(revision) = revision {
        input["revision"] = json!(revision);
    }
    let (status, body, headers) = send(
        router,
        jar,
        "POST",
        &format!("{DEVICE}/sign-in"),
        Some(origin),
        Some(input),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], origin);
    assert!(
        !headers.contains_key(header::LOCATION),
        "the device facade ignores callbackURL and never redirects credentials"
    );
    body["data"].clone()
}
pub fn no_credentials(value: &Value, jar: &Jar) {
    let body = serde_json::to_string(value).unwrap();
    for secret in jar.0.values() {
        assert!(!body.contains(secret));
    }
    assert!(!body.contains("password"));
    assert!(!body.contains("session_token"));
}
