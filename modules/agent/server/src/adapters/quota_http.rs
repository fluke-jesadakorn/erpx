//! Owner-local axum HTTP ingress for the native Agent AI-quota command.
//!
//! Transport facts only: exact registered Host/Holding target, exact browser
//! origin for mutations, one Idempotency-Key and a bounded JSON object. The
//! caller (composition) owns session/authority resolution, the serializable
//! Holding transaction and the `PgCompanyQuota` adapter. This router never
//! executes SQL and never mints an identity.
use crate::domain::quota::{QuotaFinish, QuotaOutcome, QuotaReservation};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type AgentQuotaHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Typed write kind. Only the command side lives here (CQRS).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentQuotaHttpKind {
    Reserve(QuotaReservation),
    Finish(QuotaFinish),
}

impl AgentQuotaHttpKind {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Reserve(_) => "agent.ai_quota.reserve",
            Self::Finish(_) => "agent.ai_quota.finish",
        }
    }
}

pub struct AgentQuotaHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: AgentQuotaHttpKind,
    pub key: Option<&'a CommandKey>,
}

#[derive(Clone, Debug)]
pub struct AgentQuotaHttpResponse {
    pub data: serde_json::Value,
}

impl AgentQuotaHttpResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

/// Composition-owned session/authority/transaction adapter.
pub trait AgentQuotaHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: AgentQuotaHttpRequest<'a>,
    ) -> AgentQuotaHttpFuture<'a, AgentQuotaHttpResponse>;
}

pub struct AgentQuotaHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn AgentQuotaHttpBackend>,
}

struct AgentQuotaHttpState {
    targets: BTreeMap<(String, String), AgentQuotaHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "AI_QUOTA_REQUEST_INVALID",
        "Use one bounded AI quota command.",
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
        "AI_QUOTA_CONFIGURATION_INVALID",
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
    state: &'a AgentQuotaHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a AgentQuotaHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

/// Build the owner router bound to a backend. The lead mounts the result.
pub fn agent_quota_http_router(
    targets: Vec<AgentQuotaHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/agent/ai/quota",
            post(command),
        )
        .with_state(Arc::new(AgentQuotaHttpState {
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

fn text<'a>(body: &'a serde_json::Value, field: &str) -> Result<&'a str, BusinessError> {
    body.get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(malformed)
}

fn integer(body: &serde_json::Value, field: &str) -> Result<i64, BusinessError> {
    body.get(field)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(malformed)
}

fn optional_text<'a>(
    body: &'a serde_json::Value,
    field: &str,
) -> Result<Option<&'a str>, BusinessError> {
    match body.get(field) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value.as_str().map(Some).ok_or_else(malformed),
    }
}

fn parse_kind(body: &serde_json::Value) -> Result<AgentQuotaHttpKind, BusinessError> {
    match text(body, "kind")? {
        "reserve" => {
            let reservation = QuotaReservation {
                request_id: text(body, "requestId")?.to_owned(),
                usage_id: text(body, "usageId")?.to_owned(),
                holding_id: text(body, "holdingId")?.to_owned(),
                company_id: text(body, "companyId")?.to_owned(),
                branch_id: optional_text(body, "branchId")?.map(str::to_owned),
                connection_id: optional_text(body, "connectionId")?.map(str::to_owned),
                funding_source: match text(body, "fundingSource")? {
                    "erpx" => crate::domain::quota::FundingSource::Erpx,
                    "byok" => crate::domain::quota::FundingSource::Byok,
                    _ => return Err(malformed()),
                },
                provider: text(body, "provider")?.to_owned(),
                model: text(body, "model")?.to_owned(),
                feature: text(body, "feature")?.to_owned(),
                reservation_satang: integer(body, "reservationSatang")?,
                monthly_request_limit: integer(body, "monthlyRequestLimit")?,
                window_start: integer(body, "windowStart")?,
                window_end: integer(body, "windowEnd")?,
                now: integer(body, "now")?,
            };
            reservation.validate().map_err(|_| malformed())?;
            Ok(AgentQuotaHttpKind::Reserve(reservation))
        }
        "finish" => {
            let outcome = match text(body, "outcome")? {
                "succeeded" => QuotaOutcome::Succeeded,
                "failed" => QuotaOutcome::Failed,
                _ => return Err(malformed()),
            };
            let mut units = BTreeMap::new();
            if let Some(serde_json::Value::Object(map)) = body.get("units") {
                for (key, value) in map {
                    let amount = value.as_i64().ok_or_else(malformed)?;
                    if key.is_empty() || key.len() > 80 || amount < 0 || units.contains_key(key) {
                        return Err(malformed());
                    }
                    units.insert(key.clone(), amount);
                }
            }
            let finish = QuotaFinish {
                request_id: text(body, "requestId")?.to_owned(),
                company_id: text(body, "companyId")?.to_owned(),
                outcome,
                input_tokens: integer(body, "inputTokens")?,
                output_tokens: integer(body, "outputTokens")?,
                units,
                now: integer(body, "now")?,
            };
            finish.validate().map_err(|_| malformed())?;
            Ok(AgentQuotaHttpKind::Finish(finish))
        }
        _ => Err(malformed()),
    }
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

async fn command(
    State(state): State<Arc<AgentQuotaHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let origin = unique_header(request.headers(), ORIGIN_HEADER)?.ok_or_else(forbidden)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        let key = mutation_key(request.headers())?;
        let headers = request.headers().clone();
        let body = bounded_body(request).await?;
        let kind = parse_kind(&body)?;
        target
            .backend
            .command(AgentQuotaHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind,
                key: Some(&key),
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
    impl AgentQuotaHttpBackend for Noop {
        fn command<'a>(
            &'a self,
            _: AgentQuotaHttpRequest<'a>,
        ) -> AgentQuotaHttpFuture<'a, AgentQuotaHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Noop);
        let target = || AgentQuotaHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(agent_quota_http_router(vec![target()]).is_ok());
        assert!(agent_quota_http_router(vec![target(), target()]).is_err());
        assert!(
            agent_quota_http_router(vec![AgentQuotaHttpTarget {
                origin: "http://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    #[test]
    fn parses_bounded_reserve_and_finish_kinds() {
        let reserve = serde_json::json!({
            "kind":"reserve","requestId":"req-1","usageId":"use-1","holdingId":"h-1",
            "companyId":"c-1","branchId":null,"connectionId":null,"fundingSource":"erpx",
            "provider":"openai","model":"gpt","feature":"assistant","reservationSatang":0,
            "monthlyRequestLimit":100,"windowStart":1,"windowEnd":2,"now":1
        });
        assert!(matches!(
            parse_kind(&reserve).unwrap(),
            AgentQuotaHttpKind::Reserve(_)
        ));
        let finish = serde_json::json!({
            "kind":"finish","requestId":"req-1","companyId":"c-1","outcome":"succeeded",
            "inputTokens":1,"outputTokens":2,"units":{"tokens":3},"now":1
        });
        assert!(matches!(
            parse_kind(&finish).unwrap(),
            AgentQuotaHttpKind::Finish(_)
        ));
        assert!(parse_kind(&serde_json::json!({"kind":"unknown"})).is_err());
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl AgentQuotaHttpBackend for Probe {
        fn command<'a>(
            &'a self,
            request: AgentQuotaHttpRequest<'a>,
        ) -> AgentQuotaHttpFuture<'a, AgentQuotaHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}",
                    request.key.map(CommandKey::as_str).unwrap_or("-"),
                    request.kind.operation()
                ));
                Ok(AgentQuotaHttpResponse {
                    data: serde_json::json!({"status":"reserved"}),
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
    async fn actual_router_retains_target_origin_key_and_maps_errors() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = agent_quota_http_router(vec![AgentQuotaHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe { seen: seen.clone() }),
        }])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let body = r#"{"kind":"finish","requestId":"req-1","companyId":"c-1","outcome":"succeeded","inputTokens":1,"outputTokens":2,"units":{},"now":1}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/agent/ai/quota HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: quota-key-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1["status"], "reserved");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["quota-key-1:agent.ai_quota.finish".to_owned()]
        );
        let forbidden = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/agent/ai/quota HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nIdempotency-Key: quota-key-2\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(forbidden.0, 403);
        let _ = server.abort();
    }
}
