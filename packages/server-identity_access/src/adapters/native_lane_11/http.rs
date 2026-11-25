//! Owner-local axum router for the lane 11 person-identity reads.
//!
//! The route is keyed by an exact registered Host/Holding pair, accepts only a
//! bounded Company scope header and a bounded id list, and never trusts a
//! caller-supplied row, SQL or predicate. Missing configuration, malformed
//! input and persistence faults fail closed. No fallback read exists in TS.

use crate::adapters::native_lane_11::{
    application::{load_identity_ids, load_principals},
    domain::{
        IdentityIdsRequest, PrincipalsRequest, configuration_invalid, invalid, missing,
        unavailable, valid_company, valid_slug,
    },
    postgres::PgPersonIdentityQuery,
    PersonIdentityQuery,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Exact registered Holding target. Composition supplies these; the router
/// never infers a target from an untrusted path or header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn PersonIdentityQuery>>,
}

/// Build the native read router with the owner's Pg reader. The lead mounts it;
/// this module does not edit composition.
pub fn router(
    pool: DatabasePool,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let reader: Arc<dyn PersonIdentityQuery> = Arc::new(PgPersonIdentityQuery::new(pool));
    build_router(reader, targets)
}

fn build_router(
    reader: Arc<dyn PersonIdentityQuery>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/person/identity-ids",
            post(identity_ids),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/person/principals",
            post(principals),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
        })))
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

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| !b.is_ascii_control() && b != b'\'' && b != b'"')
}

async fn parse_body<T: DeserializeOwned>(request: Request) -> Result<T, BusinessError> {
    let bytes = to_bytes(request.into_body(), MAX_BODY_BYTES)
        .await
        .map_err(|_| invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
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

async fn identity_ids(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, company) = {
            let headers = request.headers();
            let host = unique_header(headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?
                .to_owned();
            let company = unique_header(headers, CONTEXT_COMPANY)?
                .filter(|value| valid_company(value))
                .ok_or_else(invalid)?
                .to_owned();
            (host, company)
        };
        let reader = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        let body: IdentityIdsRequest = parse_body(request).await?;
        if body.ids.iter().any(|id| !valid_id(id)) {
            return Err(invalid());
        }
        let result = load_identity_ids(reader.as_ref(), &company, &body.ids).await?;
        serde_json::to_value(result).map_err(|_| unavailable())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn principals(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, company) = {
            let headers = request.headers();
            let host = unique_header(headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?
                .to_owned();
            let company = unique_header(headers, CONTEXT_COMPANY)?
                .filter(|value| valid_company(value))
                .ok_or_else(invalid)?
                .to_owned();
            (host, company)
        };
        let reader = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        let body: PrincipalsRequest = parse_body(request).await?;
        if body.ids.iter().any(|id| !valid_id(id)) {
            return Err(invalid());
        }
        let result = load_principals(reader.as_ref(), &company, &body.ids, body.snapshot).await?;
        serde_json::to_value(result).map_err(|_| unavailable())
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
    use crate::adapters::native_lane_11::{
        application::PersonIdentityFuture,
        domain::{SuccessorRow, unavailable},
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct Captured {
        company: Vec<String>,
        ids: Vec<Option<Vec<String>>>,
    }

    struct Probe {
        captured: Arc<Mutex<Captured>>,
    }

    impl PersonIdentityQuery for Probe {
        fn aliases<'a>(
            &'a self,
            company: &'a str,
            ids: &'a [String],
        ) -> PersonIdentityFuture<'a, Vec<String>> {
            let captured = self.captured.clone();
            let company = company.to_owned();
            let ids = ids.to_vec();
            Box::pin(async move {
                let mut guard = captured.lock().unwrap();
                guard.company.push(company);
                guard.ids.push(Some(ids));
                Ok(vec!["alias".into()])
            })
        }

        fn successors<'a>(
            &'a self,
            company: &'a str,
            ids: Option<&'a [String]>,
        ) -> PersonIdentityFuture<'a, Vec<SuccessorRow>> {
            let captured = self.captured.clone();
            let company = company.to_owned();
            let ids = ids.map(<[String]>::to_vec);
            Box::pin(async move {
                let mut guard = captured.lock().unwrap();
                guard.company.push(company);
                guard.ids.push(ids);
                Ok(vec![SuccessorRow {
                    original_id: "old".into(),
                    current_id: "new".into(),
                }])
            })
        }
    }

    struct Failing;

    impl PersonIdentityQuery for Failing {
        fn aliases<'a>(
            &'a self,
            _company: &'a str,
            _ids: &'a [String],
        ) -> PersonIdentityFuture<'a, Vec<String>> {
            Box::pin(async { Err(unavailable()) })
        }

        fn successors<'a>(
            &'a self,
            _company: &'a str,
            _ids: Option<&'a [String]>,
        ) -> PersonIdentityFuture<'a, Vec<SuccessorRow>> {
            Box::pin(async { Err(unavailable()) })
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (
            status,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn serve(reader: Arc<dyn PersonIdentityQuery>) -> (u16, tokio::task::JoinHandle<()>) {
        let router = build_router(reader, vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        (addr.port(), server)
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe {
            captured: Arc::new(Mutex::new(Captured::default())),
        });
        assert!(build_router(probe.clone(), vec![target()]).is_ok());
        assert!(build_router(probe.clone(), vec![target(), target()]).is_err());
        assert!(build_router(probe.clone(), vec![]).is_err());
        assert!(build_router(
            probe,
            vec![LaneTarget {
                holding_slug: "BAD".into(),
                ..target()
            }]
        )
        .is_err());
    }

    #[tokio::test]
    async fn identity_ids_returns_exact_projection_and_scope() {
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (port, server) = serve(Arc::new(Probe {
            captured: captured.clone(),
        }))
        .await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let body = r#"{"ids":["user"]}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/identity-access/person/identity-ids HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: acme:co\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1, serde_json::json!({"identityIds":["user","alias"]}));
        let guard = captured.lock().unwrap();
        assert_eq!(guard.company, vec!["acme:co".to_owned()]);
        assert_eq!(guard.ids, vec![Some(vec!["user".to_owned()])]);
        drop(guard);
        server.abort();
    }

    #[tokio::test]
    async fn principals_returns_exact_projection_and_snapshot_flag() {
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (port, server) = serve(Arc::new(Probe {
            captured: captured.clone(),
        }))
        .await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let body = r#"{"ids":["old"],"snapshot":false}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/identity-access/person/principals HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-company: acme:co\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(
            response.1,
            serde_json::json!({"principals":[{"original":"old","current":"new"}]})
        );
        let guard = captured.lock().unwrap();
        assert_eq!(guard.ids, vec![Some(vec!["old".to_owned()])]);
        drop(guard);
        server.abort();
    }

    #[tokio::test]
    async fn missing_scope_unknown_target_and_reader_fault_fail_closed() {
        let (port, server) = serve(Arc::new(Failing)).await;
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let body = r#"{"ids":["user"]}"#;
        let request = |host: &str, company: Option<&str>, path: &str, payload: &str| {
            format!(
                "POST /api/v1/holdings/acme/identity-access/person/{path} HTTP/1.1\r\nHost: {host}\r\n{}content-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{payload}",
                company
                    .map(|value| format!("x-erpx-context-company: {value}\r\n"))
                    .unwrap_or_default(),
                payload.len()
            )
        };
        let missing_company = http(addr, request("acme.example.test", None, "identity-ids", body)).await;
        assert_eq!(missing_company.0, 422);
        let unknown = http(addr, request("other.example.test", Some("acme:co"), "identity-ids", body)).await;
        assert_eq!(unknown.0, 404);
        let malformed = http(addr, request("acme.example.test", Some("acme:co"), "principals", r#"{"ids":"#)).await;
        assert_eq!(malformed.0, 422);
        let failed = http(addr, request("acme.example.test", Some("acme:co"), "identity-ids", body)).await;
        assert_eq!(failed.0, 503);
        server.abort();
    }
}
