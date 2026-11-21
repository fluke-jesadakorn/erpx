//! Owner-local HTTP ingress for the native Account time-preference command.
//!
//! Transport facts only: exact registered Host/Holding target, exact browser
//! origin for mutations, one Idempotency-Key, bounded JSON object and business
//! error mapping. Account session resolution, selected scope, the serializable
//! Control transaction and the owner repository are composition
//! responsibilities behind [`AccountTimePreferenceHttpBackend`]. The router
//! never executes SQL itself.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type AccountTimePreferenceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Typed command kind. Only the write side lives on this native route; the
/// retained read stays on the existing Control read path (CQRS separation).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountTimePreferenceHttpKind {
    Change { user_id: String },
}

impl AccountTimePreferenceHttpKind {
    pub fn is_mutation(&self) -> bool {
        matches!(self, Self::Change { .. })
    }
}

pub struct AccountTimePreferenceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: AccountTimePreferenceHttpKind,
    pub body: &'a serde_json::Value,
    pub key: Option<&'a CommandKey>,
}

#[derive(Clone, Debug)]
pub struct AccountTimePreferenceHttpResponse {
    pub data: serde_json::Value,
}

impl AccountTimePreferenceHttpResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

/// Composition-owned session/scope/transaction adapter. The router never mints
/// a session, resolves an identity, or opens a transaction.
pub trait AccountTimePreferenceHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: AccountTimePreferenceHttpRequest<'a>,
    ) -> AccountTimePreferenceHttpFuture<'a, AccountTimePreferenceHttpResponse>;
}

pub struct AccountTimePreferenceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn AccountTimePreferenceHttpBackend>,
}

struct AccountTimePreferenceHttpState {
    targets: BTreeMap<(String, String), AccountTimePreferenceHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "ACCOUNT_TIME_PREFERENCE_REQUEST_INVALID",
        "Use one bounded account time-preference command.",
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
        "ACCOUNT_TIME_PREFERENCE_CONFIGURATION_INVALID",
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

fn valid_user_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
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
    state: &'a AccountTimePreferenceHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a AccountTimePreferenceHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn account_time_preference_http_router(
    targets: Vec<AccountTimePreferenceHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/account/time-preference",
            post(change),
        )
        .with_state(Arc::new(AccountTimePreferenceHttpState {
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

async fn change(
    State(state): State<Arc<AccountTimePreferenceHttpState>>,
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
        // The aggregate key is carried in the body by the private caller; the
        // registered Holding target bounds which backend may resolve it.
        let body = bounded_body(request).await?;
        let user_id = match body.get("userId") {
            Some(serde_json::Value::String(value)) if valid_user_id(value) => value.clone(),
            _ => return Err(malformed()),
        };
        target
            .backend
            .command(AccountTimePreferenceHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: AccountTimePreferenceHttpKind::Change { user_id },
                body: &body,
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
    impl AccountTimePreferenceHttpBackend for Noop {
        fn command<'a>(
            &'a self,
            _: AccountTimePreferenceHttpRequest<'a>,
        ) -> AccountTimePreferenceHttpFuture<'a, AccountTimePreferenceHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Noop);
        let target = || AccountTimePreferenceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(account_time_preference_http_router(vec![target()]).is_ok());
        assert!(account_time_preference_http_router(vec![target(), target()]).is_err());
        assert!(
            account_time_preference_http_router(vec![AccountTimePreferenceHttpTarget {
                origin: "http://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl AccountTimePreferenceHttpBackend for Probe {
        fn command<'a>(
            &'a self,
            request: AccountTimePreferenceHttpRequest<'a>,
        ) -> AccountTimePreferenceHttpFuture<'a, AccountTimePreferenceHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{:?}",
                    request.key.map(CommandKey::as_str).unwrap_or("-"),
                    request.kind
                ));
                Ok(AccountTimePreferenceHttpResponse {
                    data: serde_json::json!({"version": 1}),
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
        let router = account_time_preference_http_router(vec![AccountTimePreferenceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe { seen: seen.clone() }),
        }])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let body =
            r#"{"userId":"user-1","mode":"system","timeZone":null,"hourCycle":"h23","version":0}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/account/time-preference HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: time-pref-key-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1["version"], 1);
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["time-pref-key-1:Change { user_id: \"user-1\" }".to_owned()]
        );
        let forbidden = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/account/time-preference HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nIdempotency-Key: time-pref-key-2\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(forbidden.0, 403);
        let _ = server.abort();
    }
}
