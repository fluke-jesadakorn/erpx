//! Owner-local HTTP ingress for native payroll run commands and detail.
//!
//! Transport facts only: exact registered Host/Holding target, exact browser
//! origin for mutations, one Idempotency-Key, bounded JSON object and business
//! error mapping. Normal Account session resolution, current IAM, package
//! admission, the serializable owner transaction and the Finance journal port
//! remain composition responsibilities behind [`PayrollCommandHttpBackend`].
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type PayrollCommandHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PayrollCommandHttpKind {
    Create,
    Calculate { run_id: String },
    Cancel { run_id: String },
    Post { run_id: String },
    Detail { run_id: String },
}

impl PayrollCommandHttpKind {
    pub fn run_id(&self) -> Option<&str> {
        match self {
            Self::Create => None,
            Self::Calculate { run_id }
            | Self::Cancel { run_id }
            | Self::Post { run_id }
            | Self::Detail { run_id } => Some(run_id),
        }
    }

    pub fn is_mutation(&self) -> bool {
        !matches!(self, Self::Detail { .. })
    }
}

pub struct PayrollCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: PayrollCommandHttpKind,
    pub body: &'a serde_json::Value,
    pub key: Option<&'a CommandKey>,
}

#[derive(Clone, Debug)]
pub struct PayrollCommandHttpResponse {
    /// Owner command/read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl PayrollCommandHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account/IAM/package/owner-unit adapter. The router never
/// mints a session, actor, Company, permission, purchase or Finance journal.
pub trait PayrollCommandHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: PayrollCommandHttpRequest<'a>,
    ) -> PayrollCommandHttpFuture<'a, PayrollCommandHttpResponse>;
}

pub struct PayrollCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn PayrollCommandHttpBackend>,
}

struct PayrollCommandHttpState {
    targets: BTreeMap<(String, String), PayrollCommandHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_COMMAND_REQUEST_INVALID",
        "Use one bounded People Payroll command.",
    )
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered application origin.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_COMMAND_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
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
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_run_id(value: &str) -> bool {
    erpx_server_shared::public::RecordId::new(value).is_ok()
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

fn target<'a>(
    state: &'a PayrollCommandHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PayrollCommandHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn people_payroll_command_router(
    targets: Vec<PayrollCommandHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
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
            "/api/v1/holdings/{holding_slug}/people/payroll/runs",
            post(create_run),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/payroll/runs/{run_id}",
            get(detail_run),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/payroll/runs/{run_id}/calculate",
            post(calculate_run),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/payroll/runs/{run_id}/cancel",
            post(cancel_run),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/payroll/runs/{run_id}/post",
            post(post_run),
        )
        .with_state(Arc::new(PayrollCommandHttpState {
            targets: configured,
        })))
}

fn mutation_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    unique_header(headers, KEY_HEADER)?
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Use a valid Idempotency-Key for this action.",
            )
        })
        .and_then(CommandKey::new)
}

async fn bounded_body(request: Request) -> Result<serde_json::Value, BusinessError> {
    if request.uri().query().is_some() {
        return Err(malformed());
    }
    let body = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    let body: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
    if !body.is_object() {
        return Err(malformed());
    }
    Ok(body)
}

async fn create_run(
    State(state): State<Arc<PayrollCommandHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    dispatch(state, slug, request, PayrollCommandHttpKind::Create, true).await
}

async fn calculate_run(
    State(state): State<Arc<PayrollCommandHttpState>>,
    Path((slug, run_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !valid_run_id(&run_id) {
        return error_response(missing());
    }
    dispatch(
        state,
        slug,
        request,
        PayrollCommandHttpKind::Calculate { run_id },
        true,
    )
    .await
}

async fn cancel_run(
    State(state): State<Arc<PayrollCommandHttpState>>,
    Path((slug, run_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !valid_run_id(&run_id) {
        return error_response(missing());
    }
    dispatch(
        state,
        slug,
        request,
        PayrollCommandHttpKind::Cancel { run_id },
        true,
    )
    .await
}

async fn post_run(
    State(state): State<Arc<PayrollCommandHttpState>>,
    Path((slug, run_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !valid_run_id(&run_id) {
        return error_response(missing());
    }
    dispatch(
        state,
        slug,
        request,
        PayrollCommandHttpKind::Post { run_id },
        true,
    )
    .await
}

async fn detail_run(
    State(state): State<Arc<PayrollCommandHttpState>>,
    Path((slug, run_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !valid_run_id(&run_id) {
        return error_response(missing());
    }
    dispatch(
        state,
        slug,
        request,
        PayrollCommandHttpKind::Detail { run_id },
        false,
    )
    .await
}

async fn dispatch(
    state: Arc<PayrollCommandHttpState>,
    slug: String,
    request: Request,
    kind: PayrollCommandHttpKind,
    mutation: bool,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let headers = request.headers().clone();
        let (body, key) = if mutation {
            let origin = unique_header(&headers, ORIGIN_HEADER)?.ok_or_else(forbidden)?;
            if origin != target.origin {
                return Err(forbidden());
            }
            let key = mutation_key(&headers)?;
            let body = bounded_body(request).await?;
            (body, Some(key))
        } else {
            if request.uri().query().is_some() {
                return Err(malformed());
            }
            (serde_json::json!({}), None)
        };
        target
            .backend
            .command(PayrollCommandHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind,
                body: &body,
                key: key.as_ref(),
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
    impl PayrollCommandHttpBackend for Noop {
        fn command<'a>(
            &'a self,
            _: PayrollCommandHttpRequest<'a>,
        ) -> PayrollCommandHttpFuture<'a, PayrollCommandHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Noop);
        let target = || PayrollCommandHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(people_payroll_command_router(vec![target()]).is_ok());
        assert!(people_payroll_command_router(vec![target(), target()]).is_err());
        assert!(
            people_payroll_command_router(vec![PayrollCommandHttpTarget {
                origin: "http://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }
    impl PayrollCommandHttpBackend for Probe {
        fn command<'a>(
            &'a self,
            request: PayrollCommandHttpRequest<'a>,
        ) -> PayrollCommandHttpFuture<'a, PayrollCommandHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{:?}",
                    request.key.map(CommandKey::as_str).unwrap_or("-"),
                    request.kind
                ));
                if self.fail {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "PAYROLL_RUN_CHANGED",
                        "Synthetic changed payroll run.",
                    ));
                }
                Ok(PayrollCommandHttpResponse {
                    data: serde_json::json!({"id":"run"}),
                    context: serde_json::json!({"userId":"user"}),
                })
            })
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

    #[tokio::test]
    async fn actual_router_retains_target_origin_key_and_error_mapping() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = people_payroll_command_router(vec![PayrollCommandHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: false,
            }),
        }])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let body = r#"{"payGroupId":"group","periodStart":"2026-10-01","periodEnd":"2026-10-31","payDate":"2026-10-31","currency":"THB","mode":"preview"}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/people/payroll/runs HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: command-key-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1["data"]["id"], "run");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["command-key-1:Create".to_owned()]
        );
        let forbidden = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/people/payroll/runs HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nIdempotency-Key: command-key-2\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(forbidden.0, 403);
        let detail = http(
            addr,
            "GET /api/v1/holdings/acme/people/payroll/runs/run HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(detail.0, 200);
        assert_eq!(
            seen.lock().unwrap().last().unwrap(),
            "-:Detail { run_id: \"run\" }"
        );
        let _ = server.abort();
    }
}
