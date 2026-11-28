//! Owner-local axum ingress for the native Agent workspace/evidence read.
//!
//! Transport facts only: an exact registered Host/Holding target, one bounded
//! POST with an allow-listed body and the exact target Origin. The caller
//! (composition) owns authority resolution and the read-only Holding unit;
//! this router never opens SQL and never mints an identity.
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");

pub type AgentWorkspaceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The two read shapes accepted by the router. Unknown fields are rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentWorkspaceHttpKind<'a> {
    Workspace { include_evidence: bool },
    Record { record_kind: &'a str, id: &'a str },
}

fn bounded(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.chars().any(char::is_control)
}

pub(crate) fn parse_kind(value: &Value) -> Result<AgentWorkspaceHttpKind<'_>, BusinessError> {
    let object = value.as_object().ok_or_else(invalid)?;
    let keys: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    match object.get("kind").and_then(Value::as_str) {
        Some("workspace") => {
            if keys != BTreeSet::from(["kind", "includeEvidence"]) {
                return Err(invalid());
            }
            let include_evidence = object
                .get("includeEvidence")
                .and_then(Value::as_bool)
                .ok_or_else(invalid)?;
            Ok(AgentWorkspaceHttpKind::Workspace { include_evidence })
        }
        Some("record") => {
            if keys != BTreeSet::from(["kind", "recordKind", "id"]) {
                return Err(invalid());
            }
            let record_kind = object
                .get("recordKind")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if !matches!(record_kind, "agent_invocation" | "agent_session") {
                return Err(invalid());
            }
            let id = object.get("id").and_then(Value::as_str).ok_or_else(invalid)?;
            if !bounded(id) {
                return Err(invalid());
            }
            Ok(AgentWorkspaceHttpKind::Record { record_kind, id })
        }
        _ => Err(invalid()),
    }
}

pub struct AgentWorkspaceHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub holding_id: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: AgentWorkspaceHttpKind<'a>,
}

pub trait AgentWorkspaceHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: AgentWorkspaceHttpReadRequest<'a>,
    ) -> AgentWorkspaceHttpFuture<'a, Value>;
}

pub struct AgentWorkspaceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub holding_id: String,
    pub origin: String,
    pub backend: Arc<dyn AgentWorkspaceHttpBackend>,
}

struct StateData {
    targets: BTreeMap<(String, String), AgentWorkspaceHttpTarget>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "AGENT_WORKSPACE_REQUEST_INVALID",
        "Use one bounded Agent workspace read.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered Holding origin.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "AGENT_WORKSPACE_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}
fn valid_origin(value: &str) -> bool {
    origin_host(value).is_some()
}
fn origin_host(value: &str) -> Option<&str> {
    let host = value.strip_prefix("https://")?;
    let (host, port) = match host.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (host, None),
    };
    if host.is_empty()
        || host
            .bytes()
            .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    {
        return None;
    }
    if port.is_some_and(|port| {
        !(1..=5).contains(&port.len())
            || !port.bytes().all(|byte| byte.is_ascii_digit())
            || port.parse::<u16>().is_err()
    }) {
        return None;
    }
    Some(host)
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
fn respond(result: Result<Value, BusinessError>) -> Response {
    match result {
        Ok(data) => {
            let mut response = (StatusCode::OK, Json(serde_json::json!({ "data": data })))
                .into_response();
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
        Err(error) => (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
    }
}

async fn read(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return respond(Err(invalid())),
    };
    let result: Result<Value, BusinessError> = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let origin = unique_header(&headers, ORIGIN_HEADER.clone())?;
        if origin != Some(target.origin.as_str()) {
            return Err(origin_forbidden());
        }
        if unique_header(&headers, header::CONTENT_TYPE)?
            .is_none_or(|value| !value.starts_with("application/json"))
        {
            return Err(invalid());
        }
        let value: Value = serde_json::from_slice(&body).map_err(|_| invalid())?;
        let kind = parse_kind(&value)?;
        target
            .backend
            .read(AgentWorkspaceHttpReadRequest {
                host,
                holding_slug: &slug,
                holding_id: &target.holding_id,
                headers: &headers,
                kind,
            })
            .await
    }
    .await;
    respond(result)
}

/// Build the owner read router from exact, distinct Holding targets. Any
/// duplicate, malformed or ambiguous target fails closed at construction.
pub fn router_for_holdings(
    targets: Vec<AgentWorkspaceHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    let mut holding_ids = BTreeSet::new();
    for target in targets {
        if target.host.is_empty()
            || target.host.len() > 255
            || target.host != origin_host(&target.origin).unwrap_or("")
            || !valid_slug(&target.holding_slug)
            || !bounded(&target.holding_id)
            || !valid_origin(&target.origin)
            || !holding_ids.insert(target.holding_id.clone())
            || map
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/agent/workspace-read",
            post(read),
        )
        .with_state(Arc::new(StateData { targets: map })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Noop;
    impl AgentWorkspaceHttpBackend for Noop {
        fn read<'a>(
            &'a self,
            _: AgentWorkspaceHttpReadRequest<'a>,
        ) -> AgentWorkspaceHttpFuture<'a, Value> {
            Box::pin(async { Ok(serde_json::json!({ "agents": [] })) })
        }
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }
    impl AgentWorkspaceHttpBackend for Probe {
        fn read<'a>(
            &'a self,
            request: AgentWorkspaceHttpReadRequest<'a>,
        ) -> AgentWorkspaceHttpFuture<'a, Value> {
            let label = match &request.kind {
                AgentWorkspaceHttpKind::Workspace { include_evidence } => {
                    format!("workspace:{}:{}", request.holding_id, include_evidence)
                }
                AgentWorkspaceHttpKind::Record { record_kind, id } => {
                    format!("record:{}:{}:{}", request.holding_id, record_kind, id)
                }
            };
            self.seen.lock().unwrap().push(label);
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "AGENT_WORKSPACE_READ_UNAVAILABLE",
                        "The Agent workspace read is unavailable.",
                    ))
                } else {
                    Ok(serde_json::json!({ "agents": [] }))
                }
            })
        }
    }

    fn target(backend: Arc<dyn AgentWorkspaceHttpBackend>) -> AgentWorkspaceHttpTarget {
        AgentWorkspaceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            holding_id: "holding-1".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        let backend: Arc<dyn AgentWorkspaceHttpBackend> = Arc::new(Noop);
        assert!(router_for_holdings(vec![target(backend.clone())]).is_ok());
        assert!(router_for_holdings(vec![target(backend.clone()), target(backend.clone())]).is_err());
        assert!(router_for_holdings(vec![AgentWorkspaceHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        }])
        .is_err());
        assert!(router_for_holdings(vec![AgentWorkspaceHttpTarget {
            host: "other.example.test".into(),
            ..target(backend.clone())
        }])
        .is_err());
        assert!(router_for_holdings(vec![AgentWorkspaceHttpTarget {
            holding_slug: "ACME".into(),
            ..target(backend)
        }])
        .is_err());
    }

    #[test]
    fn parses_only_the_exact_client_bodies() {
        let workspace = serde_json::json!({"kind":"workspace","includeEvidence":false});
        assert_eq!(
            parse_kind(&workspace).unwrap(),
            AgentWorkspaceHttpKind::Workspace {
                include_evidence: false
            }
        );
        let record = serde_json::json!({"kind":"record","recordKind":"agent_session","id":"s-1"});
        assert_eq!(
            parse_kind(&record).unwrap(),
            AgentWorkspaceHttpKind::Record {
                record_kind: "agent_session",
                id: "s-1"
            }
        );
        assert!(parse_kind(&serde_json::json!({"kind":"record","recordKind":"other","id":"s-1"})).is_err());
        assert!(parse_kind(&serde_json::json!({"kind":"workspace","includeEvidence":"false"})).is_err());
        assert!(parse_kind(&serde_json::json!({"kind":"workspace","includeEvidence":true,"extra":1})).is_err());
        assert!(parse_kind(&serde_json::json!({"kind":"record","recordKind":"agent_session","id":"bad\nid"})).is_err());
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, Value) {
        let bytes = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream.write_all(raw.as_bytes()).await.unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).await.unwrap();
            bytes
        })
        .await
        .expect("bounded HTTP test exchange");
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    async fn serve(router: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (addr, server)
    }

    #[tokio::test]
    async fn actual_router_matches_host_holding_and_fails_closed() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = router_for_holdings(vec![target(Arc::new(Probe {
            seen: seen.clone(),
            fail: false,
        }))])
        .unwrap();
        let (addr, server) = serve(router).await;

        let body = r#"{"kind":"record","recordKind":"agent_invocation","id":"ai-evidence-1"}"#;
        let ok = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/agent/workspace-read HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.1["data"]["agents"], serde_json::json!([]));
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["record:holding-1:agent_invocation:ai-evidence-1".to_owned()]
        );

        let wrong_host = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/agent/workspace-read HTTP/1.1\r\nHost: other.example.test\r\nOrigin: https://acme.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(wrong_host.0, 404);

        let wrong_origin = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/agent/workspace-read HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(wrong_origin.0, 403);

        let bad_body = http(
            addr,
            "POST /api/v1/holdings/acme/agent/workspace-read HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nContent-Type: application/json\r\nContent-Length: 20\r\nConnection: close\r\n\r\n{\"kind\":\"workspace\"}".to_owned(),
        )
        .await;
        assert_eq!(bad_body.0, 422);
        server.abort();
    }

    #[tokio::test]
    async fn backend_failure_maps_to_503_error_envelope() {
        let router = router_for_holdings(vec![target(Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        }))])
        .unwrap();
        let (addr, server) = serve(router).await;
        let body = r#"{"kind":"workspace","includeEvidence":true}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/agent/workspace-read HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(response.0, 503);
        assert_eq!(response.1["error"]["code"], "AGENT_WORKSPACE_READ_UNAVAILABLE");
        server.abort();
    }
}
