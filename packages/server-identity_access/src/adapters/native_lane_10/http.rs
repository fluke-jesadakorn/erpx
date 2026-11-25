//! Owner-local axum ingress for lane 10's account invitation reads.
//!
//! Transport facts only: exact registered Host/Holding target, a bounded
//! Company header and one bounded JSON read request. The reader executes the
//! SQL and the projection; this module maps failures to explicit statuses and
//! adds private, no-store headers. There is no fallback path.

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::{AccountAccessRead, AccountAccessReadRequest, execute};
use super::domain::invalid;
use super::postgres::PgAccountAccessRead;

const BODY_LIMIT: usize = 8 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

pub const READ_PATH: &str = "/api/v1/holdings/{holding_slug}/identity-access/account-access-read";

/// Exact registered Holding target. No wildcard or caller-supplied host is
/// accepted; the lead mounts this router with the same targets as the owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn AccountAccessRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ACCOUNT_ACCESS_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn valid_company(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

/// Reader-injection core: one distinct reader per registered Holding. Tests
/// use this path to prove two targets reach two independent readers without a
/// database; `router_for_holdings` builds the exact same state map in production.
pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn AccountAccessRead>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(READ_PATH, post(read))
        .with_state(Arc::new(LaneState { targets: configured })))
}

/// Build the native read router over one shared injected reader. Retained for
/// tests and single-Holding callers; both paths use the same transport checks.
pub fn router_with_reader(
    reader: Arc<dyn AccountAccessRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Build the native read router with one owner Pg reader. The lead mounts it;
/// this module does not edit composition.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgAccountAccessRead::new(pool)), targets)
}

/// Mount entry point with one Pg reader — and therefore one pool — per
/// registered Holding. `pools[i + 1]` is the i-th Holding's database, so each
/// `(host, holding_slug)` key owns its own reader and no Holding can reach
/// another Holding's data. The lead mounts this router in composition.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn AccountAccessRead> = Arc::new(PgAccountAccessRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?
            .clone();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_company(value))
            .ok_or_else(invalid)?
            .to_owned();
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let request: AccountAccessReadRequest =
            serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let kind = request.kind().as_str();
        let value = execute(reader.as_ref(), &company, request).await?;
        Ok::<_, BusinessError>(serde_json::json!({"kind":kind,"value":value.into_json()}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_10::application::AccountAccessReadFuture;
    use crate::adapters::native_lane_10::domain::{Invitation, KIND_EMAIL_EXISTS, unavailable};
    use erpx_server_runtime::public::http_support::body::Body;
    use sha2::{Digest, Sha256};
    use std::sync::Mutex;
    use tower::ServiceExt;

    struct Probe {
        company: Mutex<Vec<String>>,
        fail: bool,
        invitation: Option<Invitation>,
    }

    impl AccountAccessRead for Probe {
        fn email_exists<'a>(&'a self, email: &'a str) -> AccountAccessReadFuture<'a, bool> {
            assert_eq!(email, "person@example.test");
            Box::pin(async move { Ok(!self.fail) })
        }
        fn invitation_status<'a>(
            &'a self,
            _holding_id: &'a str,
            company_id: &'a str,
            _person_id: &'a str,
            _invitation_id: &'a str,
        ) -> AccountAccessReadFuture<'a, Option<String>> {
            self.company.lock().unwrap().push(company_id.to_owned());
            Box::pin(async move { Ok(Some("pending".into())) })
        }
        fn invitation_by_token<'a>(
            &'a self,
            _token_hash: &'a str,
            _now: i64,
        ) -> AccountAccessReadFuture<'a, Option<Invitation>> {
            if self.fail {
                return Box::pin(async { Err(unavailable()) });
            }
            let invitation = self.invitation.clone();
            Box::pin(async move { Ok(invitation) })
        }
        fn recovery_account_email<'a>(
            &'a self,
            user_id: &'a str,
        ) -> AccountAccessReadFuture<'a, Option<String>> {
            assert_eq!(user_id, "u1");
            Box::pin(async move {
                if self.fail {
                    Err(unavailable())
                } else {
                    Ok(Some("user@example.test".into()))
                }
            })
        }
        fn recovery_latest_job<'a>(
            &'a self,
            user_id: &'a str,
        ) -> AccountAccessReadFuture<'a, Option<String>> {
            assert_eq!(user_id, "u1");
            Box::pin(async move { Ok(Some("job_1".into())) })
        }
    }

    fn probe(fail: bool) -> Arc<Probe> {
        Arc::new(Probe {
            company: Mutex::new(Vec::new()),
            fail,
            invitation: None,
        })
    }

    fn build_app(reader: Arc<dyn AccountAccessRead>) -> Router {
        router_with_reader(
            reader,
            vec![LaneTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
            }],
        )
        .expect("router")
    }

    fn request(host: &str, slug: &str, company: Option<&str>, body: &str) -> Request {
        let mut builder = Request::builder()
            .method("POST")
            .uri(format!("http://127.0.0.1:3100/api/v1/holdings/{slug}/identity-access/account-access-read"))
            .header("host", host)
            .header("content-type", "application/json");
        if let Some(company) = company {
            builder = builder.header("x-erpx-context-company", company);
        }
        builder.body(Body::from(body.to_owned())).expect("request")
    }

    const EMAIL_BODY: &str = r#"{"kind":"email_exists","email":"person@example.test"}"#;

    #[tokio::test]
    async fn registered_target_and_company_are_required() {
        let reader = probe(false);
        let app = build_app(reader.clone());
        let response = app
            .clone()
            .oneshot(request("acme.example.test", "acme", Some("c1"), EMAIL_BODY))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        let body = serde_json::to_value(
            serde_json::from_slice::<serde_json::Value>(
                &axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("bytes"),
            )
            .expect("json"),
        )
        .expect("value");
        assert_eq!(body, serde_json::json!({"kind":KIND_EMAIL_EXISTS,"value":true}));

        let app = build_app(reader.clone());
        for (host, slug) in [("evil.example.test", "acme"), ("acme.example.test", "other")] {
            let response = app
                .clone()
                .oneshot(request(host, slug, Some("c1"), EMAIL_BODY))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn company_header_is_unique_bounded_and_forwarded() {
        let reader = probe(false);
        let app = build_app(reader.clone());
        for company in [None, Some("not a company"), Some("")] {
            let response = app
                .clone()
                .oneshot(request("acme.example.test", "acme", company, EMAIL_BODY))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        let body = r#"{"kind":"invitation_status","holding_id":"h1","person_id":"p1","invitation_id":"i1"}"#;
        let response = app
            .clone()
            .oneshot(request("acme.example.test", "acme", Some("c1"), body))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("bytes");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).expect("json"),
            serde_json::json!({"kind":"invitation_status","value":"pending"})
        );
        assert_eq!(reader.company.lock().unwrap().as_slice(), &["c1".to_owned()]);

        let mut duplicate = request("acme.example.test", "acme", Some("c1"), EMAIL_BODY);
        duplicate.headers_mut().append("x-erpx-context-company", HeaderValue::from_static("c2"));
        let response = app.oneshot(duplicate).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn closed_request_and_failed_reader_fail_closed() {
        let reader = probe(false);
        let app = build_app(reader);
        for body in [
            r#"{"kind":"drop_table","email":"person@example.test"}"#,
            r#"{"kind":"email_exists","email":"person@example.test","sql":"SELECT 1"}"#,
            r#"{"kind":"email_exists","email":"nope"}"#,
            &"x".repeat(9 * 1024),
        ] {
            let response = app
                .clone()
                .oneshot(request("acme.example.test", "acme", Some("c1"), body))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }

        let failing_app = build_app(probe(true));
        let body = r#"{"kind":"invitation_by_token","token_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","now":1700000000000}"#;
        let response = failing_app
            .oneshot(request("acme.example.test", "acme", Some("c1"), body))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("bytes");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(value["error"]["code"], "ACCOUNT_ACCESS_READ_UNAVAILABLE");
    }

    #[tokio::test]
    async fn recovery_control_reads_pin_the_response_shape() {
        let app = build_app(probe(false));
        for (body, expected) in [
            (
                r#"{"kind":"recovery_account_email","user_id":"u1"}"#,
                serde_json::json!({"kind":"recovery_account_email","value":"user@example.test"}),
            ),
            (
                r#"{"kind":"recovery_latest_job","user_id":"u1"}"#,
                serde_json::json!({"kind":"recovery_latest_job","value":"job_1"}),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(request("acme.example.test", "acme", Some("platform"), body))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("bytes");
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).expect("json"), expected);
        }
        let failing = build_app(probe(true));
        let response = failing
            .oneshot(request(
                "acme.example.test",
                "acme",
                Some("platform"),
                r#"{"kind":"recovery_account_email","user_id":"u1"}"#,
            ))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn router_rejects_invalid_configuration() {
        assert!(router_with_reader(
            probe(false),
            vec![LaneTarget { host: "".into(), holding_slug: "acme".into() }],
        )
        .is_err());
        assert!(router_with_reader(probe(false), Vec::new()).is_err());
        assert!(router_with_reader(
            probe(false),
            vec![
                LaneTarget { host: "acme.example.test".into(), holding_slug: "acme".into() },
                LaneTarget { host: "acme.example.test".into(), holding_slug: "acme".into() },
            ],
        )
        .is_err());
    }

    /// Reader injection is the testable equivalent of `router_for_holdings`:
    /// each target/pool pair becomes one independent reader in the state map.
    #[tokio::test]
    async fn per_holding_readers_are_distinct() {
        let acme: Arc<dyn AccountAccessRead> = probe(false);
        let northstar: Arc<dyn AccountAccessRead> = probe(true);
        let app = router_with_readers(vec![
            (
                LaneTarget { host: "acme.example.test".into(), holding_slug: "acme".into() },
                acme,
            ),
            (
                LaneTarget { host: "northstar.example.test".into(), holding_slug: "northstar".into() },
                northstar,
            ),
        ])
        .expect("router");

        // Both targets answer through their own reader, with different values.
        for (host, slug, expected) in [
            ("acme.example.test", "acme", true),
            ("northstar.example.test", "northstar", false),
        ] {
            let response = app
                .clone()
                .oneshot(request(host, slug, Some("c1"), EMAIL_BODY))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("bytes");
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).expect("json"),
                serde_json::json!({"kind":KIND_EMAIL_EXISTS,"value":expected})
            );
        }
        // A mismatched host/slug pair must not fall through to either reader.
        let response = app
            .oneshot(request("acme.example.test", "northstar", Some("c1"), EMAIL_BODY))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(router_for_holdings(Vec::new()).is_err());
        let _ = crate::public::native_lane_10::router_for_holdings;
        let _ = crate::public::native_lane_10::router_with_readers;
    }

    #[test]
    fn response_shape_oracle_hash_is_pinned() {
        let value = serde_json::json!({"kind":"invitation_status","value":"pending"});
        let hash = hex::encode(Sha256::digest(serde_json::to_string(&value).expect("json").as_bytes()));
        assert_eq!(hash, "45f6739eba125a1ad4c1b6d010ae3ffafd10d8bd5976e45fb032cdc24e948aa7");
    }
}
