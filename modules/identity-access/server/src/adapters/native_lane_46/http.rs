//! Owner-local axum ingress for the native directory "connection by id" read.
//!
//! Transport facts only: one exact registered `(Host, holding_slug)` target and
//! a closed body carrying the Company scope and connection id. The injected
//! reader executes the SQL and builds the projection; this module never builds
//! SQL and has no fallback.

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::{ConnectionRead, ConnectionReadRequest, execute};
use super::domain::{ConnectionRow, invalid, valid_id, valid_token};
use super::postgres::PgConnectionRead;

const BODY_LIMIT: usize = 4 * 1024;

pub const READ_PATH: &str = "/api/v1/holdings/{holding_slug}/identity-access/connection-read";

/// One exact registered Holding target. No wildcard or caller-supplied Host is
/// accepted; the lead mounts this router with the same targets as the owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn ConnectionRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "IDENTITY_CONNECTION_READ_CONFIG_INVALID",
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

fn bounded(row: ConnectionRow) -> Response {
    let value = serde_json::to_value(row).unwrap_or(serde_json::Value::Null);
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
/// database; `router_for_holdings` builds the same state map in production.
pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn ConnectionRead>)>,
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
    reader: Arc<dyn ConnectionRead>,
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
    router_with_reader(Arc::new(PgConnectionRead::new(pool)), targets)
}

/// Per-Holding mount entry point: one Pg reader (and therefore one Control or
/// Holding pool) per registered Holding, so no Holding can reach another's
/// connections.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn ConnectionRead> = Arc::new(PgConnectionRead::new(pool));
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
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        if bytes.is_empty() || bytes.len() > BODY_LIMIT {
            return Err(invalid());
        }
        let body: ConnectionReadRequest =
            serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if !valid_token(&body.company_id) || !valid_id(&body.id) {
            return Err(invalid());
        }
        Ok::<_, BusinessError>(execute(reader.as_ref(), &body.company_id, &body.id).await?)
    }
    .await;
    match result {
        Ok(row) => bounded(row),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_46::application::ConnectionReadFuture;
    use crate::adapters::native_lane_46::domain::ConnectionFields;
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use std::sync::Mutex;
    use tower::ServiceExt;

    struct Probe {
        seen: Mutex<Vec<(String, String)>>,
        fail: bool,
    }

    impl ConnectionRead for Probe {
        fn connection<'a>(
            &'a self,
            company_id: &'a str,
            id: &'a str,
        ) -> ConnectionReadFuture<'a, Option<ConnectionRow>> {
            self.seen
                .lock()
                .unwrap()
                .push((company_id.to_owned(), id.to_owned()));
            if self.fail {
                return Box::pin(async { Err(invalid()) });
            }
            Box::pin(async move {
                Ok(Some(
                    ConnectionFields {
                        id: "connection-1".into(),
                        company_id: "acme-company".into(),
                        name: "Acme AD".into(),
                        provider: "active_directory".into(),
                        config: serde_json::json!({"mode":"ldaps","authority":"ad.acme.test"}),
                        status: "active".into(),
                        version: 3,
                        tested_version: Some(2),
                        last_sync_at: Some(1_700_000_000_000),
                        health: "healthy".into(),
                        last_error: None,
                        created_by: "actor-1".into(),
                        created_at: 1_699_000_000_000,
                    }
                    .into_row()
                    .expect("row"),
                ))
            })
        }
    }

    fn probe(fail: bool) -> Arc<Probe> {
        Arc::new(Probe {
            seen: Mutex::new(Vec::new()),
            fail,
        })
    }

    fn build_app(reader: Arc<dyn ConnectionRead>) -> Router {
        router_with_reader(
            reader,
            vec![LaneTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
            }],
        )
        .expect("router")
    }

    fn request(host: &str, slug: &str, body: &str) -> Request {
        Request::builder()
            .method("POST")
            .uri(format!("http://127.0.0.1:3100/api/v1/holdings/{slug}/identity-access/connection-read"))
            .header("host", host)
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .expect("request")
    }

    #[tokio::test]
    async fn registered_target_and_complete_payload_are_required() {
        let reader = probe(false);
        let app = build_app(reader.clone());
        let response = app
            .clone()
            .oneshot(request(
                "acme.example.test",
                "acme",
                r#"{"company_id":"acme-company","id":"connection-1"}"#,
            ))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("bytes");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            value["data"]["id"],
            serde_json::json!("connection-1")
        );
        assert_eq!(value["data"]["company_id"], serde_json::json!("acme-company"));
        assert_eq!(value["data"]["provider"], serde_json::json!("active_directory"));
        assert_eq!(value["data"]["version"], serde_json::json!(3));
        assert_eq!(value["data"]["tested_version"], serde_json::json!(2));
        assert_eq!(
            value["data"]["config"]["mode"],
            serde_json::json!("ldaps")
        );
        assert_eq!(
            value["data"].as_object().expect("object").len(),
            13,
            "the retired SELECT * row carried all thirteen columns"
        );
        assert_eq!(
            reader.seen.lock().unwrap().as_slice(),
            &[("acme-company".to_owned(), "connection-1".to_owned())]
        );

        for (host, slug) in [("evil.example.test", "acme"), ("acme.example.test", "other")] {
            let response = app
                .clone()
                .oneshot(request(host, slug, r#"{"company_id":"acme","id":"connection-1"}"#))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn caller_bodies_are_closed_and_scopes_are_validated() {
        let reader = probe(false);
        let app = build_app(reader.clone());
        for body in [
            "{}",
            r#"{"company_id":"acme company","id":"connection-1"}"#,
            r#"{"company_id":"acme","id":"bad id"}"#,
            r#"{"company_id":"acme","id":"connection-1","sql":"DROP TABLE x"}"#,
            "not json",
        ] {
            let response = app
                .clone()
                .oneshot(request("acme.example.test", "acme", body))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        }
        assert!(reader.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_rows_and_persistence_faults_fail_closed() {
        let reader = probe(true);
        let app = build_app(reader.clone());
        let response = app
            .clone()
            .oneshot(request(
                "acme.example.test",
                "acme",
                r#"{"company_id":"acme","id":"connection-1"}"#,
            ))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let missing: Arc<dyn ConnectionRead> = Arc::new(Missing);
        let app = build_app(missing);
        let response = app
            .oneshot(request(
                "acme.example.test",
                "acme",
                r#"{"company_id":"acme","id":"connection-1"}"#,
            ))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("bytes");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(value["error"]["code"], "CONNECTION_NOT_FOUND");
    }

    struct Missing;

    impl ConnectionRead for Missing {
        fn connection<'a>(
            &'a self,
            _company_id: &'a str,
            _id: &'a str,
        ) -> ConnectionReadFuture<'a, Option<ConnectionRow>> {
            Box::pin(async { Ok(None) })
        }
    }

    #[tokio::test]
    async fn per_holding_readers_are_distinct() {
        let acme = probe(false);
        let other = probe(false);
        let app = router_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                },
                acme.clone() as Arc<dyn ConnectionRead>,
            ),
            (
                LaneTarget {
                    host: "other.example.test".into(),
                    holding_slug: "other".into(),
                },
                other.clone() as Arc<dyn ConnectionRead>,
            ),
        ])
        .expect("router");
        let response = app
            .clone()
            .oneshot(request(
                "other.example.test",
                "other",
                r#"{"company_id":"other","id":"connection-2"}"#,
            ))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            other.seen.lock().unwrap().as_slice(),
            &[("other".to_owned(), "connection-2".to_owned())]
        );
        assert!(acme.seen.lock().unwrap().is_empty());
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
