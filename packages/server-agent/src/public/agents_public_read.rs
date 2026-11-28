//! Owner-local public Agent read ingress for the retired Next route.
//!
//! Transport facts only: an exact registered Host/Holding target, the browser
//! Origin check and the `{data}` envelope. Authority and the read-only Holding
//! unit stay in composition; the SQL is owned by `agent_workspace_read` and
//! `PgAgentWorkspaceRead`. This module does not mint a session or open SQL.

use crate::adapters::workspace_read_http::{
    AgentWorkspaceHttpBackend, AgentWorkspaceHttpKind, AgentWorkspaceHttpReadRequest,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

pub type AgentsPublicReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct AgentsPublicReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub holding_id: &'a str,
    pub headers: &'a HeaderMap,
}

pub trait AgentsPublicReadBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: AgentsPublicReadRequest<'a>,
    ) -> AgentsPublicReadFuture<'a, Value>;
}

/// Composition's existing workspace-read backend already serves the full
/// workspace (invocations and sessions included) through owner SQL.
impl<T: AgentWorkspaceHttpBackend + ?Sized> AgentsPublicReadBackend for T {
    fn read<'a>(
        &'a self,
        request: AgentsPublicReadRequest<'a>,
    ) -> AgentsPublicReadFuture<'a, Value> {
        Box::pin(async move {
            AgentWorkspaceHttpBackend::read(
                self,
                AgentWorkspaceHttpReadRequest {
                    host: request.host,
                    holding_slug: request.holding_slug,
                    holding_id: request.holding_id,
                    headers: request.headers,
                    kind: AgentWorkspaceHttpKind::Workspace {
                        include_evidence: true,
                    },
                },
            )
            .await
        })
    }
}

pub struct AgentsPublicReadTarget {
    pub host: String,
    pub holding_slug: String,
    pub holding_id: String,
    pub origin: String,
    pub backend: Arc<dyn AgentsPublicReadBackend>,
}

struct StateData {
    targets: BTreeMap<(String, String), AgentsPublicReadTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "AGENT_REQUEST_INVALID",
        "Use one bounded Agent workspace read.",
    )
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
        "AGENT_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}
fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let host = rest.rsplit_once(':').map_or(rest, |(host, _)| host);
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    })
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
fn bounded(value: Value) -> Response {
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
fn respond(result: Result<Value, BusinessError>) -> Response {
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

async fn read(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<Value, BusinessError> = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        if unique_header(&headers, header::ORIGIN)?.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        target
            .backend
            .read(AgentsPublicReadRequest {
                host,
                holding_slug: &slug,
                holding_id: &target.holding_id,
                headers: &headers,
            })
            .await
    }
    .await;
    respond(result)
}

pub fn agents_public_read_router(
    targets: Vec<AgentsPublicReadTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || map
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route("/api/v1/holdings/{holding_slug}/agents", get(read))
        .with_state(Arc::new(StateData { targets: map })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }

    impl AgentsPublicReadBackend for Probe {
        fn read<'a>(
            &'a self,
            request: AgentsPublicReadRequest<'a>,
        ) -> AgentsPublicReadFuture<'a, Value> {
            let seen = self.seen.clone();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "AGENT_WORKSPACE_READ_UNAVAILABLE",
                        "The native Agent workspace read is unavailable.",
                    ));
                }
                seen.lock()
                    .unwrap()
                    .push(format!("{}:{}", request.holding_slug, request.holding_id));
                Ok(serde_json::json!({
                    "agents": [], "versions": [], "pools": [], "assignments": [],
                    "invocations": [{"id": "ai-1"}], "people": [], "members": [],
                    "poolHistory": [], "sessions": [{"id": "as-1"}],
                }))
            })
        }
    }

    fn target(
        backend: Arc<dyn AgentsPublicReadBackend>,
    ) -> AgentsPublicReadTarget {
        AgentsPublicReadTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
            holding_id: "holding-1".to_owned(),
            origin: "https://acme.example.test".to_owned(),
            backend,
        }
    }

    async fn exchange(addr: std::net::SocketAddr, request: String) -> (u16, Value) {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
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
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    async fn serve(router: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        (addr, server)
    }

    #[tokio::test]
    async fn public_agents_read_serves_full_workspace_and_fails_closed() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = agents_public_read_router(vec![target(Arc::new(Probe {
            seen: seen.clone(),
            fail: false,
        }))])
        .unwrap();
        let (addr, server) = serve(router).await;
        let ok = exchange(
            addr,
            "GET /api/v1/holdings/acme/agents HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nConnection: close\r\n\r\n".to_owned(),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.1["data"]["invocations"][0]["id"], "ai-1");
        assert_eq!(ok.1["data"]["sessions"][0]["id"], "as-1");
        assert_eq!(seen.lock().unwrap().as_slice(), &["acme:holding-1".to_owned()]);

        let wrong_host = exchange(
            addr,
            "GET /api/v1/holdings/acme/agents HTTP/1.1\r\nHost: other.example.test\r\nConnection: close\r\n\r\n".to_owned(),
        )
        .await;
        assert_eq!(wrong_host.0, 404);

        let wrong_origin = exchange(
            addr,
            "GET /api/v1/holdings/acme/agents HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nConnection: close\r\n\r\n".to_owned(),
        )
        .await;
        assert_eq!(wrong_origin.0, 403);
        server.abort();
    }

    #[tokio::test]
    async fn backend_failure_maps_to_503_envelope() {
        let router = agents_public_read_router(vec![target(Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        }))])
        .unwrap();
        let (addr, server) = serve(router).await;
        let response = exchange(
            addr,
            "GET /api/v1/holdings/acme/agents HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n".to_owned(),
        )
        .await;
        assert_eq!(response.0, 503);
        assert_eq!(
            response.1["error"]["code"],
            "AGENT_WORKSPACE_READ_UNAVAILABLE"
        );
        server.abort();
    }
}
