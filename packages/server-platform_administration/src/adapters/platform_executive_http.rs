//! Owner-local HTTP ingress for the Platform Executive read.
//!
//! Transport facts only: exact registered Platform Host, bounded query parsing,
//! private response headers and business-error mapping. The composition backend
//! resolves the normal Account session, current Platform assignment, Company
//! scope and `platform.executive.read` before calling the owner application.
//! The router never mints a session, Platform role, Company scope or permission.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Query, Response, Router, State, StatusCode, get,
    header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type PlatformExecutiveHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Parsed, bounded query for the single owner read endpoint. Snapshot mode has
/// no metric; source mode requires a metric and group plus an explicit range.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlatformExecutiveHttpQuery {
    pub metric: Option<String>,
    pub group: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<String>,
}

impl PlatformExecutiveHttpQuery {
    pub fn from_map(values: BTreeMap<String, String>) -> Result<Self, BusinessError> {
        let mut query = Self::default();
        for (key, value) in values {
            match key.as_str() {
                "metric" => query.metric = Some(value),
                "group" => query.group = Some(value),
                "from" => query.from = Some(value),
                "to" => query.to = Some(value),
                "page" => query.page = Some(value),
                _ => {
                    return Err(BusinessError::invalid(
                        "PLATFORM_EXECUTIVE_QUERY_INVALID",
                        "Use a supported Executive query.",
                    ));
                }
            }
        }
        if query.metric.is_none() && (query.group.is_some() || query.page.is_some()) {
            return Err(BusinessError::invalid(
                "PLATFORM_EXECUTIVE_QUERY_INVALID",
                "Choose an Executive metric before requesting its sources.",
            ));
        }
        Ok(query)
    }

    pub fn is_source(&self) -> bool {
        self.metric.is_some()
    }
}

pub struct PlatformExecutiveHttpReadRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
    pub query: &'a PlatformExecutiveHttpQuery,
}

#[derive(Clone, Debug)]
pub struct PlatformExecutiveHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl PlatformExecutiveHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, Platform role, Company scope and owner
/// adapter. The composition selects Programme authority; this port only reads.
pub trait PlatformExecutiveHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: PlatformExecutiveHttpReadRequest<'a>,
    ) -> PlatformExecutiveHttpFuture<'a, PlatformExecutiveHttpResponse>;
}

pub struct PlatformExecutiveHttpTarget {
    pub host: String,
    pub backend: Arc<dyn PlatformExecutiveHttpBackend>,
}

pub struct PlatformExecutiveHttpService {
    targets: BTreeMap<String, PlatformExecutiveHttpTarget>,
}

struct PlatformExecutiveHttpState {
    service: Arc<PlatformExecutiveHttpService>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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
fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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

impl PlatformExecutiveHttpService {
    pub fn new(targets: Vec<PlatformExecutiveHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target
                    .host
                    .bytes()
                    .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
                || configured.insert(target.host.clone(), target).is_some()
            {
                return Err(BusinessError::invalid(
                    "PLATFORM_EXECUTIVE_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Platform Host targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &HeaderMap,
    ) -> Result<&'a PlatformExecutiveHttpTarget, BusinessError> {
        let mut values = headers.get_all(header::HOST).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "PLATFORM_EXECUTIVE_QUERY_INVALID",
                "Use one unambiguous Platform Host.",
            ));
        }
        let host = first
            .map(|value| value.to_str())
            .transpose()
            .map_err(|_| {
                BusinessError::invalid(
                    "PLATFORM_EXECUTIVE_QUERY_INVALID",
                    "Use a valid Platform Host.",
                )
            })?
            .ok_or_else(missing)?;
        self.targets.get(host).ok_or_else(missing)
    }

    pub async fn read(&self, headers: &HeaderMap, query: &PlatformExecutiveHttpQuery) -> Response {
        let target = match self.target(headers) {
            Ok(target) => target,
            Err(error) => return error_response(error),
        };
        match target
            .backend
            .read(PlatformExecutiveHttpReadRequest {
                host: &target.host,
                headers,
                query,
            })
            .await
        {
            Ok(reply) => bounded(reply.value()),
            Err(error) => error_response(error),
        }
    }
}

pub fn platform_executive_router(
    targets: Vec<PlatformExecutiveHttpTarget>,
) -> Result<Router, BusinessError> {
    let service = Arc::new(PlatformExecutiveHttpService::new(targets)?);
    Ok(Router::new()
        .route("/api/v1/platform/executive", get(read_executive))
        .with_state(Arc::new(PlatformExecutiveHttpState { service })))
}

async fn read_executive(
    State(state): State<Arc<PlatformExecutiveHttpState>>,
    headers: HeaderMap,
    Query(values): Query<BTreeMap<String, String>>,
) -> Response {
    let query = match PlatformExecutiveHttpQuery::from_map(values) {
        Ok(query) => query,
        Err(error) => return error_response(error),
    };
    state.service.read(&headers, &query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Probe {
        seen: Mutex<Vec<String>>,
        fail: bool,
    }
    impl PlatformExecutiveHttpBackend for Probe {
        fn read<'a>(
            &'a self,
            request: PlatformExecutiveHttpReadRequest<'a>,
        ) -> PlatformExecutiveHttpFuture<'a, PlatformExecutiveHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{:?}:{:?}",
                    request.host, request.query.metric, request.query.page
                ));
                if self.fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PLATFORM_EXECUTIVE_UNAVAILABLE",
                        "Synthetic unavailable.",
                    ));
                }
                Ok(PlatformExecutiveHttpResponse {
                    data: serde_json::json!({"scope":"platform","metrics":[]}),
                    context: serde_json::json!({"userId":"user"}),
                })
            })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_hosts() {
        let backend = Arc::new(Probe::default());
        let target = || PlatformExecutiveHttpTarget {
            host: "erpx-internal.example.test".into(),
            backend: backend.clone(),
        };
        assert!(platform_executive_router(vec![target()]).is_ok());
        assert!(platform_executive_router(vec![target(), target()]).is_err());
        assert!(
            platform_executive_router(vec![PlatformExecutiveHttpTarget {
                host: "https://erpx-internal.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    #[test]
    fn query_parsing_keeps_only_bounded_snapshot_and_source_fields() {
        let snapshot = PlatformExecutiveHttpQuery::from_map(BTreeMap::from([
            ("from".into(), "2026-06-01".into()),
            ("to".into(), "2026-06-30".into()),
        ]))
        .unwrap();
        assert!(!snapshot.is_source());
        assert_eq!(snapshot.from.as_deref(), Some("2026-06-01"));
        let source = PlatformExecutiveHttpQuery::from_map(BTreeMap::from([
            ("metric".into(), "failed-provisioning".into()),
            ("group".into(), "2026-06".into()),
            ("from".into(), "2026-06-01".into()),
            ("to".into(), "2026-06-30".into()),
            ("page".into(), "2".into()),
        ]))
        .unwrap();
        assert!(source.is_source());
        assert_eq!(source.page.as_deref(), Some("2"));
        assert_eq!(
            PlatformExecutiveHttpQuery::from_map(BTreeMap::from([("extra".into(), "x".into())]))
                .unwrap_err()
                .code,
            "PLATFORM_EXECUTIVE_QUERY_INVALID"
        );
        assert_eq!(
            PlatformExecutiveHttpQuery::from_map(BTreeMap::from([(
                "group".into(),
                "2026-06".into()
            )]))
            .unwrap_err()
            .code,
            "PLATFORM_EXECUTIVE_QUERY_INVALID"
        );
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

    #[tokio::test]
    async fn actual_router_retains_host_query_and_error_mapping() {
        let probe = Arc::new(Probe::default());
        let failing = Arc::new(Probe {
            seen: Mutex::new(Vec::new()),
            fail: true,
        });
        let router = platform_executive_router(vec![
            PlatformExecutiveHttpTarget {
                host: "erpx-internal.example.test".into(),
                backend: probe.clone(),
            },
            PlatformExecutiveHttpTarget {
                host: "fail.example.test".into(),
                backend: failing,
            },
        ])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let (status, body) = http(
            addr,
            "GET /api/v1/platform/executive HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["scope"], "platform");
        let (status, _) = http(
            addr,
            "GET /api/v1/platform/executive?metric=failed-provisioning&group=2026-06&from=2026-06-01&to=2026-06-30&page=2 HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 200);
        let (status, body) = http(
            addr,
            "GET /api/v1/platform/executive?extra=1 HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PLATFORM_EXECUTIVE_QUERY_INVALID");
        let (status, body) = http(
            addr,
            "GET /api/v1/platform/executive HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 503);
        assert_eq!(body["error"]["code"], "PLATFORM_EXECUTIVE_UNAVAILABLE");
        let (status, _) = http(
            addr,
            "GET /api/v1/platform/executive HTTP/1.1\r\nHost: missing.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 404);
        let seen = probe.seen.lock().unwrap().clone();
        assert!(
            seen.iter()
                .any(|value| value == "erpx-internal.example.test:None:None")
        );
        assert!(
            seen.iter().any(|value| value
                == "erpx-internal.example.test:Some(\"failed-provisioning\"):Some(\"2\")")
        );
        server.abort();
    }
}
