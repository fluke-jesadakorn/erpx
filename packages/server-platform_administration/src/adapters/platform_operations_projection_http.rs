//! Owner-local HTTP ingress for the native operations projection read.
//!
//! Transport facts only: exact registered Platform Host, private response
//! headers and business-error mapping. The composition backend resolves the
//! normal Account session, current Platform assignment and Company scope before
//! calling the owner application. The route is read-only and never mints
//! authority.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Response, Router, State, StatusCode, get, header,
    serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type PlatformOperationsProjectionHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PlatformOperationsProjectionHttpReadRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct PlatformOperationsProjectionHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl PlatformOperationsProjectionHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, Platform role and owner adapter.
pub trait PlatformOperationsProjectionHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: PlatformOperationsProjectionHttpReadRequest<'a>,
    ) -> PlatformOperationsProjectionHttpFuture<'a, PlatformOperationsProjectionHttpResponse>;
}

pub struct PlatformOperationsProjectionHttpTarget {
    pub host: String,
    pub backend: Arc<dyn PlatformOperationsProjectionHttpBackend>,
}

pub struct PlatformOperationsProjectionHttpService {
    targets: BTreeMap<String, PlatformOperationsProjectionHttpTarget>,
}

struct PlatformOperationsProjectionHttpState {
    service: Arc<PlatformOperationsProjectionHttpService>,
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

impl PlatformOperationsProjectionHttpService {
    pub fn new(
        targets: Vec<PlatformOperationsProjectionHttpTarget>,
    ) -> Result<Self, BusinessError> {
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
                    "PLATFORM_OPERATIONS_PROJECTION_CONFIGURATION_INVALID",
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
    ) -> Result<&'a PlatformOperationsProjectionHttpTarget, BusinessError> {
        let mut values = headers.get_all(header::HOST).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "PLATFORM_OPERATIONS_PROJECTION_REQUEST_INVALID",
                "Use one unambiguous Platform Host.",
            ));
        }
        let host = first
            .map(|value| value.to_str())
            .transpose()
            .map_err(|_| {
                BusinessError::invalid(
                    "PLATFORM_OPERATIONS_PROJECTION_REQUEST_INVALID",
                    "Use a valid Platform Host.",
                )
            })?
            .ok_or_else(missing)?;
        self.targets.get(host).ok_or_else(missing)
    }

    pub async fn read(&self, headers: &HeaderMap) -> Response {
        let target = match self.target(headers) {
            Ok(target) => target,
            Err(error) => return error_response(error),
        };
        match target
            .backend
            .read(PlatformOperationsProjectionHttpReadRequest {
                host: &target.host,
                headers,
            })
            .await
        {
            Ok(reply) => bounded(reply.value()),
            Err(error) => error_response(error),
        }
    }
}

pub fn platform_operations_projection_router(
    targets: Vec<PlatformOperationsProjectionHttpTarget>,
) -> Result<Router, BusinessError> {
    let service = Arc::new(PlatformOperationsProjectionHttpService::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/platform/operations/projection",
            get(read_projection),
        )
        .with_state(Arc::new(PlatformOperationsProjectionHttpState { service })))
}

async fn read_projection(
    State(state): State<Arc<PlatformOperationsProjectionHttpState>>,
    headers: HeaderMap,
) -> Response {
    state.service.read(&headers).await
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
    impl PlatformOperationsProjectionHttpBackend for Probe {
        fn read<'a>(
            &'a self,
            request: PlatformOperationsProjectionHttpReadRequest<'a>,
        ) -> PlatformOperationsProjectionHttpFuture<'a, PlatformOperationsProjectionHttpResponse>
        {
            Box::pin(async move {
                self.seen.lock().unwrap().push(request.host.into());
                if self.fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PLATFORM_OPERATIONS_UNAVAILABLE",
                        "Synthetic unavailable.",
                    ));
                }
                Ok(PlatformOperationsProjectionHttpResponse {
                    data: serde_json::json!({
                        "generatedAt": 10,
                        "actorRole": "platform_admin",
                        "controlHealth": [],
                        "holdings": [],
                        "totals": {
                            "holdings": 0,
                            "pendingEvents": 0,
                            "deadLetterEvents": 0,
                            "claimedEvents": 0
                        }
                    }),
                    context: serde_json::json!({"userId":"user"}),
                })
            })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_hosts() {
        let backend = Arc::new(Probe::default());
        let target = || PlatformOperationsProjectionHttpTarget {
            host: "erpx-internal.example.test".into(),
            backend: backend.clone(),
        };
        assert!(platform_operations_projection_router(vec![target()]).is_ok());
        assert!(platform_operations_projection_router(vec![target(), target()]).is_err());
        assert!(
            platform_operations_projection_router(vec![PlatformOperationsProjectionHttpTarget {
                host: "https://erpx-internal.example.test".into(),
                ..target()
            }])
            .is_err()
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
    async fn actual_router_reads_only_the_projection_and_maps_errors() {
        let probe = Arc::new(Probe::default());
        let failing = Arc::new(Probe {
            seen: Mutex::new(Vec::new()),
            fail: true,
        });
        let router = platform_operations_projection_router(vec![
            PlatformOperationsProjectionHttpTarget {
                host: "erpx-internal.example.test".into(),
                backend: probe.clone(),
            },
            PlatformOperationsProjectionHttpTarget {
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
            "GET /api/v1/platform/operations/projection HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["totals"]["holdings"], 0);
        let (status, body) = http(
            addr,
            "GET /api/v1/platform/operations/projection HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 503);
        assert_eq!(body["error"]["code"], "PLATFORM_OPERATIONS_UNAVAILABLE");
        let (status, _) = http(
            addr,
            "GET /api/v1/platform/operations/projection HTTP/1.1\r\nHost: missing.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(
            probe.seen.lock().unwrap().clone(),
            vec!["erpx-internal.example.test"]
        );
        server.abort();
    }
}
