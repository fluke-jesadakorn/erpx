//! Owner-local axum HTTP ingress for the native sandbox email delivery write.
//! Transport facts only: exact registered Host/Holding target, exact browser
//! origin, one Idempotency-Key and a bounded JSON object.
use crate::domain::email_delivery::EmailDeliveryInput;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 192 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type EmailDeliveryHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmailDeliveryHttpKind {
    Reserve(EmailDeliveryInput),
}

pub struct EmailDeliveryHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: EmailDeliveryHttpKind,
    pub key: Option<&'a CommandKey>,
}

#[derive(Clone, Debug)]
pub struct EmailDeliveryHttpResponse {
    pub data: serde_json::Value,
}

impl EmailDeliveryHttpResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

pub trait EmailDeliveryHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: EmailDeliveryHttpRequest<'a>,
    ) -> EmailDeliveryHttpFuture<'a, EmailDeliveryHttpResponse>;
}

pub struct EmailDeliveryHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn EmailDeliveryHttpBackend>,
}

struct EmailDeliveryHttpState {
    targets: BTreeMap<(String, String), EmailDeliveryHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "EMAIL_DELIVERY_REQUEST_INVALID",
        "Use one bounded sandbox email delivery command.",
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
        "EMAIL_DELIVERY_CONFIGURATION_INVALID",
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

fn bounded_response(body: serde_json::Value) -> Response {
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
    state: &'a EmailDeliveryHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a EmailDeliveryHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn email_delivery_http_router(
    targets: Vec<EmailDeliveryHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/manage/sandbox-email/delivery",
            post(reserve),
        )
        .with_state(Arc::new(EmailDeliveryHttpState {
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

fn parse_kind(body: &serde_json::Value) -> Result<EmailDeliveryHttpKind, BusinessError> {
    let input = EmailDeliveryInput {
        id: text(body, "id")?.to_owned(),
        delivery_key: text(body, "deliveryKey")?.to_owned(),
        recipient: text(body, "recipient")?.to_owned(),
        subject: text(body, "subject")?.to_owned(),
        body_text: text(body, "bodyText")?.to_owned(),
        body_ciphertext: match body.get("bodyCiphertext") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(value.as_str().ok_or_else(malformed)?.to_owned()),
        },
        provider_id: text(body, "providerId")?.to_owned(),
        created_at: body
            .get("createdAt")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(malformed)?,
        metadata_json: text(body, "metadataJson")?.to_owned(),
    };
    input.validate().map_err(|_| malformed())?;
    Ok(EmailDeliveryHttpKind::Reserve(input))
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

async fn reserve(
    State(state): State<Arc<EmailDeliveryHttpState>>,
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
            .command(EmailDeliveryHttpRequest {
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
        Ok(reply) => bounded_response(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Noop;
    impl EmailDeliveryHttpBackend for Noop {
        fn command<'a>(
            &'a self,
            _: EmailDeliveryHttpRequest<'a>,
        ) -> EmailDeliveryHttpFuture<'a, EmailDeliveryHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Noop);
        let target = || EmailDeliveryHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(email_delivery_http_router(vec![target()]).is_ok());
        assert!(email_delivery_http_router(vec![target(), target()]).is_err());
    }

    #[test]
    fn parses_bounded_delivery_and_rejects_bad_metadata() {
        let body = serde_json::json!({
            "id":"email-1","deliveryKey":"key-1","recipient":"to@example.test",
            "subject":"S","bodyText":"body","bodyCiphertext":null,
            "providerId":"sandbox-key-1","createdAt":1,"metadataJson":"{}"
        });
        assert!(matches!(
            parse_kind(&body).unwrap(),
            EmailDeliveryHttpKind::Reserve(_)
        ));
        let mut bad = body;
        bad["metadataJson"] = serde_json::json!("[]");
        assert!(parse_kind(&bad).is_err());
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl EmailDeliveryHttpBackend for Probe {
        fn command<'a>(
            &'a self,
            request: EmailDeliveryHttpRequest<'a>,
        ) -> EmailDeliveryHttpFuture<'a, EmailDeliveryHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(
                    request
                        .key
                        .map(CommandKey::as_str)
                        .unwrap_or("-")
                        .to_owned(),
                );
                Ok(EmailDeliveryHttpResponse {
                    data: serde_json::json!({"deliveryKey":"key-1"}),
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
        let router = email_delivery_http_router(vec![EmailDeliveryHttpTarget {
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
        let body = r#"{"id":"email-1","deliveryKey":"key-1","recipient":"to@example.test","subject":"S","bodyText":"body","bodyCiphertext":null,"providerId":"sandbox-key-1","createdAt":1,"metadataJson":"{}"}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/manage/sandbox-email/delivery HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: email-key-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1["deliveryKey"], "key-1");
        assert_eq!(seen.lock().unwrap().as_slice(), &["email-key-1".to_owned()]);
        let _ = server.abort();
    }
}
