//! Owner-local HTTP ingress for native Payroll summaries and run lists.
//!
//! Transport facts only: registered Host/Holding target, optional pay month,
//! bounded JSON and business-error mapping. Normal Account session resolution,
//! current Company scope, payroll purchase admission and the serializable owner
//! read remain composition responsibilities supplied through the backend trait.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type PayrollReadHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PayrollReadHttpKind {
    Summary { pay_month: Option<String> },
    Runs,
}

pub struct PayrollReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: PayrollReadHttpKind,
}

#[derive(Clone, Debug)]
pub struct PayrollReadHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl PayrollReadHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, purchase and owner-unit adapter.
/// The router never mints a session, actor, Company or purchase.
pub trait PayrollReadHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: PayrollReadHttpRequest<'a>,
    ) -> PayrollReadHttpFuture<'a, PayrollReadHttpResponse>;
}

pub struct PayrollReadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PayrollReadHttpBackend>,
}

struct PayrollReadHttpState {
    targets: BTreeMap<(String, String), PayrollReadHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_REQUEST_INVALID",
        "Use one unambiguous People Payroll request.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(request_invalid());
            };
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| request_invalid())
}

fn query_pay_month(request: &Request) -> Result<Option<String>, BusinessError> {
    let Some(query) = request.uri().query() else {
        return Ok(None);
    };
    let mut pay_month = None;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == "payMonth" {
            if pay_month.is_some() {
                return Err(request_invalid());
            }
            pay_month = Some(percent_decode(value)?);
        }
    }
    Ok(pay_month)
}

pub fn people_payroll_router(targets: Vec<PayrollReadHttpTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PEOPLE_PAYROLL_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people/payroll/summary",
            get(read_summary),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/payroll/runs",
            get(read_runs),
        )
        .with_state(Arc::new(PayrollReadHttpState {
            targets: configured,
        })))
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

fn target<'a>(
    state: &'a PayrollReadHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PayrollReadHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn read_summary(
    State(state): State<Arc<PayrollReadHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let pay_month = query_pay_month(&request)?;
        target
            .backend
            .read(PayrollReadHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                kind: PayrollReadHttpKind::Summary { pay_month },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_runs(
    State(state): State<Arc<PayrollReadHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        target
            .backend
            .read(PayrollReadHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                kind: PayrollReadHttpKind::Runs,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Noop;
    impl PayrollReadHttpBackend for Noop {
        fn read<'a>(
            &'a self,
            _: PayrollReadHttpRequest<'a>,
        ) -> PayrollReadHttpFuture<'a, PayrollReadHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }
    impl PayrollReadHttpBackend for Probe {
        fn read<'a>(
            &'a self,
            request: PayrollReadHttpRequest<'a>,
        ) -> PayrollReadHttpFuture<'a, PayrollReadHttpResponse> {
            Box::pin(async move {
                self.seen
                    .lock()
                    .unwrap()
                    .push(format!("read:{:?}", request.kind));
                if self.fail {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "PAYROLL_READ_CHANGED",
                        "Synthetic changed payroll source.",
                    ));
                }
                Ok(PayrollReadHttpResponse {
                    data: serde_json::json!([]),
                    context: serde_json::json!({"userId":"user"}),
                })
            })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Noop);
        let target = || PayrollReadHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(people_payroll_router(vec![target()]).is_ok());
        assert!(people_payroll_router(vec![target(), target()]).is_err());
        assert!(
            people_payroll_router(vec![PayrollReadHttpTarget {
                origin: "http://acme.example.test".into(),
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
    async fn actual_router_retains_targets_query_and_error_mapping() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Arc::new(Probe {
            seen: seen.clone(),
            fail: false,
        });
        let failing = Arc::new(Probe {
            seen: seen.clone(),
            fail: true,
        });
        let router = people_payroll_router(vec![
            PayrollReadHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                origin: "https://acme.example.test".into(),
                backend: probe,
            },
            PayrollReadHttpTarget {
                host: "fail.example.test".into(),
                holding_slug: "fail".into(),
                origin: "https://fail.example.test".into(),
                backend: failing,
            },
        ])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let get = |path: &str| {
            format!("GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n")
        };
        let (status, body) = http(addr, get("/api/v1/holdings/acme/people/payroll/summary")).await;
        assert_eq!(status, 200);
        assert_eq!(body["data"], serde_json::json!([]));
        let (status, _) = http(
            addr,
            get("/api/v1/holdings/acme/people/payroll/summary?payMonth=2026-10"),
        )
        .await;
        assert_eq!(status, 200);
        let (status, body) = http(
            addr,
            get("/api/v1/holdings/acme/people/payroll/summary?payMonth=2026-10&payMonth=2026-11"),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_REQUEST_INVALID");
        let (status, _) = http(addr, get("/api/v1/holdings/acme/people/payroll/runs")).await;
        assert_eq!(status, 200);
        let (status, _) = http(addr, get("/api/v1/holdings/missing/people/payroll/summary")).await;
        assert_eq!(status, 404);
        let (status, body) = http(
            addr,
            "GET /api/v1/holdings/fail/people/payroll/runs HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 409);
        assert_eq!(body["error"]["code"], "PAYROLL_READ_CHANGED");
        let seen = seen.lock().unwrap().clone();
        assert!(
            seen.iter()
                .any(|value| value == "read:Summary { pay_month: None }")
        );
        assert!(
            seen.iter()
                .any(|value| value == "read:Summary { pay_month: Some(\"2026-10\") }")
        );
        assert!(seen.iter().any(|value| value == "read:Runs"));
        server.abort();
    }

    #[tokio::test]
    async fn response_headers_are_private_and_bounded() {
        let probe = Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
            fail: false,
        });
        let router = people_payroll_router(vec![PayrollReadHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: probe,
        }])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                b"GET /api/v1/holdings/acme/people/payroll/summary HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
        assert!(text.contains("cache-control: private, no-store"));
        assert!(text.contains("referrer-policy: no-referrer"));
        assert!(text.contains("x-content-type-options: nosniff"));
        server.abort();
    }
}
