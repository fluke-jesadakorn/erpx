//! Owner-local axum ingress for the native IAM "sessions by user" read.
//!
//! Transport facts only: one exact registered `(Host, holding_slug)` target,
//! the private `x-erpx-context-user` header injected by the authenticated
//! gateway, and an authority-free body. The injected reader executes the SQL
//! and builds the projection; this module never builds SQL and has no fallback.

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::{SessionListRead, SessionListRequest, execute};
use super::domain::{SessionRow, invalid, valid_id};
use super::postgres::PgSessionListRead;

const BODY_LIMIT: usize = 4 * 1024;
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

pub const READ_PATH: &str = "/api/v1/holdings/{holding_slug}/identity-access/session-list-read";

/// One exact registered Holding target. No wildcard or caller-supplied Host is
/// accepted; the lead mounts this router with the same targets as the owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn SessionListRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "IAM_SESSION_READ_CONFIG_INVALID",
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

fn valid_user(value: &str) -> bool {
    RecordId::new(value).is_ok() && valid_id(value)
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

fn bounded(rows: Vec<SessionRow>) -> Response {
    let value = serde_json::Value::Array(
        rows.into_iter()
            .map(|row| serde_json::to_value(row).unwrap_or(serde_json::Value::Null))
            .collect(),
    );
    let mut response = (StatusCode::OK, Json(serde_json::json!({"data": value}))).into_response();
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

/// Reader-injection core: one distinct reader per registered Holding. Tests use
/// this path to prove two targets reach two independent readers without a
/// database; `router_for_holdings` builds the exact same state map in production.
pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn SessionListRead>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || target.host.len() > 255
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

/// Single shared injected reader (tests and single-Holding callers).
pub fn router_with_reader(
    reader: Arc<dyn SessionListRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Native read router over one owner Pg reader. The lead mounts it; this module
/// does not edit composition.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgSessionListRead::new(pool)), targets)
}

/// Per-Holding mount entry point: one Pg reader (and therefore one Control
/// pool) per registered Holding, so no Holding can reach another's sessions.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn SessionListRead> = Arc::new(PgSessionListRead::new(pool));
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
            .filter(|host| !host.is_empty() && host.len() <= 255)
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?
            .clone();
        // Identity is authority: it comes only from the private context header.
        let user = unique_header(request.headers(), CONTEXT_USER)?
            .filter(|value| valid_user(value))
            .ok_or_else(invalid)?
            .to_owned();
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        if bytes.len() > BODY_LIMIT {
            return Err(invalid());
        }
        let body = if bytes.is_empty() {
            SessionListRequest {}
        } else {
            serde_json::from_slice::<SessionListRequest>(&bytes).map_err(|_| invalid())?
        };
        let _ = body;
        Ok::<_, BusinessError>(execute(reader.as_ref(), &user).await?)
    }
    .await;
    match result {
        Ok(rows) => bounded(rows),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use std::sync::Mutex;
    use tower::ServiceExt;

    struct Probe {
        users: Mutex<Vec<String>>,
        fail: bool,
    }

    impl SessionListRead for Probe {
        fn list_sessions<'a>(
            &'a self,
            user_id: &'a str,
        ) -> crate::adapters::native_lane_45::application::SessionListFuture<'a, Vec<SessionRow>> {
            self.users.lock().unwrap().push(user_id.to_owned());
            if self.fail {
                return Box::pin(async { Err(invalid()) });
            }
            Box::pin(async move {
                Ok(vec![SessionRow {
                    id: "session-1".into(),
                    created_at: "2026-01-01T00:00:00.000Z".into(),
                    expires_at: "2026-01-08T00:00:00.000Z".into(),
                    user_agent: Some("ERPX/1.0".into()),
                }])
            })
        }
    }

    fn probe(fail: bool) -> Arc<Probe> {
        Arc::new(Probe {
            users: Mutex::new(Vec::new()),
            fail,
        })
    }

    fn build_app(reader: Arc<dyn SessionListRead>) -> Router {
        router_with_reader(
            reader,
            vec![LaneTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
            }],
        )
        .expect("router")
    }

    fn request(host: &str, slug: &str, user: Option<&str>, body: &str) -> Request {
        let mut builder = Request::builder()
            .method("POST")
            .uri(format!("http://127.0.0.1:3100/api/v1/holdings/{slug}/identity-access/session-list-read"))
            .header("host", host)
            .header("content-type", "application/json");
        if let Some(user) = user {
            builder = builder.header("x-erpx-context-user", user);
        }
        builder.body(Body::from(body.to_owned())).expect("request")
    }

    #[tokio::test]
    async fn registered_target_and_private_actor_are_required() {
        let reader = probe(false);
        let app = build_app(reader.clone());
        let response = app
            .clone()
            .oneshot(request("acme.example.test", "acme", Some("user-1"), "{}"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("bytes");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            value,
            serde_json::json!({"data": [{
                "id": "session-1",
                "createdAt": "2026-01-01T00:00:00.000Z",
                "expiresAt": "2026-01-08T00:00:00.000Z",
                "userAgent": "ERPX/1.0",
            }]})
        );
        assert_eq!(reader.users.lock().unwrap().as_slice(), &["user-1".to_owned()]);

        for (host, slug) in [("evil.example.test", "acme"), ("acme.example.test", "other")] {
            let response = app
                .clone()
                .oneshot(request(host, slug, Some("user-1"), "{}"))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn caller_cannot_supply_identity_and_bodies_are_closed() {
        let reader = probe(false);
        let app = build_app(reader.clone());
        for (user, body) in [
            (None, "{}"),
            (Some("not a user"), "{}"),
            (Some("user-1"), r#"{"user_id":"someone-else"}"#),
            (Some("user-1"), "not json"),
        ] {
            let response = app
                .clone()
                .oneshot(request("acme.example.test", "acme", user, body))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        }
        assert!(reader.users.lock().unwrap().is_empty());

        let mut duplicate = request("acme.example.test", "acme", Some("user-1"), "{}");
        duplicate.headers_mut().append(
            "x-erpx-context-user",
            HeaderValue::from_static("someone-else"),
        );
        let response = app.oneshot(duplicate).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn per_holding_readers_are_distinct_and_fail_closed() {
        let acme: Arc<dyn SessionListRead> = probe(false);
        let other: Arc<dyn SessionListRead> = probe(true);
        let app = router_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                },
                acme,
            ),
            (
                LaneTarget {
                    host: "other.example.test".into(),
                    holding_slug: "other".into(),
                },
                other,
            ),
        ])
        .expect("router");
        let response = app
            .clone()
            .oneshot(request("other.example.test", "other", Some("user-2"), "{}"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = app
            .oneshot(request("acme.example.test", "other", Some("user-1"), "{}"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn configuration_is_strict() {
        assert!(router_with_reader(probe(false), Vec::new()).is_err());
        assert!(router_with_reader(
            probe(false),
            vec![LaneTarget {
                host: "".into(),
                holding_slug: "acme".into(),
            }],
        )
        .is_err());
        assert!(router_for_holdings(Vec::new()).is_err());
    }
}
