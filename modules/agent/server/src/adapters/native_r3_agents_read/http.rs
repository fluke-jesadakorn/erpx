//! Owner-local axum ingress for the exact R3 Agents read path.
//!
//! Transport facts only: an exact registered Host/Holding target, an optional
//! matching browser Origin and the single `view=evidence` query. The caller
//! (composition) owns authority resolution and binds the backend that executes
//! the owner SQL; this router never opens SQL and never mints an identity.
//!
//! The wire envelope is exactly `{data: <agents payload>}` with
//! `Cache-Control: private, no-store`, matching the retained Next GET route.

use super::domain::{bounded, evidence_view, forbidden, invalid, not_found, valid_origin, valid_slug};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const BODY_LIMIT: usize = 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");

pub type AgentsReadHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct AgentsReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub evidence_view: bool,
}

/// Composition resolves the current work context and returns the exact payload.
pub trait AgentsReadBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: AgentsReadHttpRequest<'a>,
    ) -> AgentsReadHttpFuture<'a, Value>;
}

pub struct AgentsReadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn AgentsReadBackend>,
}

struct StateData {
    targets: BTreeMap<(String, String), AgentsReadHttpTarget>,
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "AGENTS_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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
            let mut response = (StatusCode::OK, Json(json!({ "data": data }))).into_response();
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
            Json(json!({"error":{"code":error.code,"message":error.message}})),
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
    let query = request.uri().query().map(str::to_owned);
    let result: Result<Value, BusinessError> = async {
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        if !body.is_empty() {
            return Err(invalid());
        }
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(not_found)?;
        let target = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(not_found)?;
        if unique_header(&headers, ORIGIN_HEADER.clone())? != Some(target.origin.as_str()) {
            return Err(forbidden());
        }
        let evidence = evidence_view(query.as_deref());
        target
            .backend
            .read(AgentsReadHttpRequest {
                host,
                holding_slug: &slug,
                headers: &headers,
                evidence_view: evidence,
            })
            .await
    }
    .await;
    respond(result)
}

/// Build the exact-path Agents read router from distinct registered targets.
/// The lead mounts the returned Router into the private gateway.
pub fn agents_read_router(
    targets: Vec<AgentsReadHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.host.len() > 255
            || !valid_slug(&target.holding_slug)
            || !bounded(&target.origin)
            || !valid_origin(&target.origin)
            || map
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if map.is_empty() {
        return Err(configuration_invalid());
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
        seen: Arc<Mutex<Vec<bool>>>,
    }

    impl AgentsReadBackend for Probe {
        fn read<'a>(
            &'a self,
            request: AgentsReadHttpRequest<'a>,
        ) -> AgentsReadHttpFuture<'a, Value> {
            self.seen.lock().unwrap().push(request.evidence_view);
            Box::pin(async { Ok(json!({"agents":[],"sessions":[]})) })
        }
    }

    fn target(backend: Arc<dyn AgentsReadBackend>) -> AgentsReadHttpTarget {
        AgentsReadHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        let backend: Arc<dyn AgentsReadBackend> = Arc::new(Probe {
            seen: Arc::new(Mutex::new(vec![])),
        });
        assert!(agents_read_router(vec![target(backend.clone())]).is_ok());
        assert!(agents_read_router(vec![target(backend.clone()), target(backend.clone())]).is_err());
        assert!(agents_read_router(vec![AgentsReadHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        }])
        .is_err());
        assert!(agents_read_router(vec![AgentsReadHttpTarget {
            holding_slug: "ACME".into(),
            ..target(backend)
        }])
        .is_err());
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, Value) {
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
        let headers = text.split("\r\n\r\n").next().unwrap_or("").to_ascii_lowercase();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (status, headers, serde_json::from_str(body).unwrap_or(Value::Null))
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
    async fn actual_router_returns_the_exact_envelope_and_headers() {
        let seen = Arc::new(Mutex::new(vec![]));
        let router = agents_read_router(vec![target(Arc::new(Probe { seen: seen.clone() }))])
            .unwrap();
        let (addr, server) = serve(router).await;
        let ok = http(
            addr,
            "GET /api/v1/holdings/acme/agents?view=evidence HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert!(ok.1.contains("cache-control: private, no-store"));
        assert_eq!(ok.2["data"]["agents"], json!([]));
        assert_eq!(seen.lock().unwrap().as_slice(), &[true]);

        let wrong_origin = http(
            addr,
            "GET /api/v1/holdings/acme/agents HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(wrong_origin.0, 403);
        assert_eq!(wrong_origin.2["error"]["code"], "ORIGIN_FORBIDDEN");

        let wrong_host = http(
            addr,
            "GET /api/v1/holdings/acme/agents HTTP/1.1\r\nHost: other.example.test\r\nOrigin: https://acme.example.test\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(wrong_host.0, 404);
        server.abort();
    }
}
