//! Owner-local HTTP ingress for the native Manufacturing accounting-profile
//! command (wave 3). Transport facts only: exact registered Host/Holding,
//! exact browser Origin for mutations, one Idempotency-Key, bounded JSON and
//! business-error mapping. Session, scope, the serializable transaction and
//! the owner repository are composition responsibilities behind
//! [`ProfileHttpBackend`]; this router never executes SQL.
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

pub type ProfileHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Typed command kind. Only the write side lives on this native route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileHttpKind {
    Create,
    Review { profile_id: String },
}

impl ProfileHttpKind {
    pub fn is_mutation(&self) -> bool {
        true
    }
}

pub struct ProfileHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: ProfileHttpKind,
    pub body: &'a serde_json::Value,
    pub key: Option<&'a CommandKey>,
}

#[derive(Clone, Debug)]
pub struct ProfileHttpResponse {
    pub data: serde_json::Value,
}

impl ProfileHttpResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

pub trait ProfileHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: ProfileHttpRequest<'a>,
    ) -> ProfileHttpFuture<'a, ProfileHttpResponse>;
}

pub struct ProfileHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn ProfileHttpBackend>,
}

struct ProfileHttpState {
    targets: BTreeMap<(String, String), ProfileHttpTarget>,
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_ACCOUNTING_REQUEST_INVALID",
        "Use one bounded accounting profile command.",
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
        "MANUFACTURING_ACCOUNTING_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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

fn valid_id(value: &str) -> bool {
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
    state: &'a ProfileHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a ProfileHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn accounting_profile_wave3_http_router(
    targets: Vec<ProfileHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/manufacturing/accounting-profile",
            post(create),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/manufacturing/accounting-profile/{profile_id}",
            post(review),
        )
        .with_state(Arc::new(ProfileHttpState {
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

async fn create(
    State(state): State<Arc<ProfileHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    dispatch(&state, &slug, request, ProfileHttpKind::Create).await
}

async fn review(
    State(state): State<Arc<ProfileHttpState>>,
    Path((slug, profile_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !valid_id(&profile_id) {
        return error_response(malformed());
    }
    dispatch(
        &state,
        &slug,
        request,
        ProfileHttpKind::Review { profile_id },
    )
    .await
}

async fn dispatch(
    state: &ProfileHttpState,
    slug: &str,
    request: Request,
    kind: ProfileHttpKind,
) -> Response {
    let result = async move {
        let target = target(state, request.headers(), slug)?;
        let origin = unique_header(request.headers(), ORIGIN_HEADER)?.ok_or_else(forbidden)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        let key = mutation_key(request.headers())?;
        let headers = request.headers().clone();
        let body = bounded_body(request).await?;
        target
            .backend
            .command(ProfileHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind,
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

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl ProfileHttpBackend for Probe {
        fn command<'a>(
            &'a self,
            request: ProfileHttpRequest<'a>,
        ) -> ProfileHttpFuture<'a, ProfileHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{:?}",
                    request.key.map(CommandKey::as_str).unwrap_or("-"),
                    request.kind
                ));
                Ok(ProfileHttpResponse {
                    data: serde_json::json!({"id":"profile-1","status":"draft","version":1}),
                })
            })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let target = || ProfileHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(accounting_profile_wave3_http_router(vec![target()]).is_ok());
        assert!(accounting_profile_wave3_http_router(vec![target(), target()]).is_err());
        assert!(
            accounting_profile_wave3_http_router(vec![ProfileHttpTarget {
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
    async fn actual_router_retains_target_origin_key_and_error_mapping() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = accounting_profile_wave3_http_router(vec![ProfileHttpTarget {
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
        let body = r#"{"chartId":"chart-1","inventoryAccountId":"inv","wipAccountId":"wip","laborAccountId":"labor","overheadAccountId":"overhead","scrapAccountId":"scrap"}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/manufacturing/accounting-profile HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: profile-key-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1["version"], 1);
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["profile-key-1:Create".to_owned()]
        );
        let forbidden = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/manufacturing/accounting-profile HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nIdempotency-Key: profile-key-2\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(forbidden.0, 403);
        let _ = server.abort();
    }
}
