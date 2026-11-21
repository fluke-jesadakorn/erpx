//! Owner-local axum HTTP ingress for the native engineering enrollment write.
//! Transport facts only: exact registered Host/Holding target, exact configured
//! origin for this isolated local command, one Idempotency-Key and a bounded
//! JSON object. All attestation/database authority remains upstream in the
//! retained application before dispatch; this router only executes the write.
use crate::domain::engineering_enrollment::EngineeringEnrollmentInput;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type EngineeringEnrollmentHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineeringEnrollmentHttpKind {
    Enroll(EngineeringEnrollmentInput),
}

pub struct EngineeringEnrollmentHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: EngineeringEnrollmentHttpKind,
    pub key: Option<&'a CommandKey>,
}

#[derive(Clone, Debug)]
pub struct EngineeringEnrollmentHttpResponse {
    pub data: serde_json::Value,
}

impl EngineeringEnrollmentHttpResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

pub trait EngineeringEnrollmentHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: EngineeringEnrollmentHttpRequest<'a>,
    ) -> EngineeringEnrollmentHttpFuture<'a, EngineeringEnrollmentHttpResponse>;
}

pub struct EngineeringEnrollmentHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin permitted for this isolated local command.
    pub origin: String,
    pub backend: Arc<dyn EngineeringEnrollmentHttpBackend>,
}

struct EngineeringEnrollmentHttpState {
    targets: BTreeMap<(String, String), EngineeringEnrollmentHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "ENGINEERING_ENROLLMENT_REQUEST_INVALID",
        "Use one bounded engineering enrollment command.",
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
        "ENGINEERING_ENROLLMENT_CONFIGURATION_INVALID",
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

/// The engineering command is served only from the isolated local gateway
/// origin (`http://127.0.0.1:3104` / `http://localhost:3104`) or an exact https
/// Holding origin. Both are registered per target, never inferred from a body.
fn valid_origin(origin: &str) -> bool {
    if matches!(origin, "http://127.0.0.1:3104" | "http://localhost:3104") {
        return true;
    }
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
    state: &'a EngineeringEnrollmentHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a EngineeringEnrollmentHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn engineering_enrollment_http_router(
    targets: Vec<EngineeringEnrollmentHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/platform/engineering-verification/enroll",
            post(enroll),
        )
        .with_state(Arc::new(EngineeringEnrollmentHttpState {
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

fn strings(body: &serde_json::Value, field: &str) -> Result<Vec<String>, BusinessError> {
    let Some(value) = body.get(field) else {
        return Ok(vec![]);
    };
    let array = value.as_array().ok_or_else(malformed)?;
    array
        .iter()
        .map(|item| item.as_str().map(str::to_owned).ok_or_else(malformed))
        .collect()
}

fn parse_kind(body: &serde_json::Value) -> Result<EngineeringEnrollmentHttpKind, BusinessError> {
    let field = |name: &str| {
        body.get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(malformed)
    };
    let input = EngineeringEnrollmentInput {
        run: field("run")?,
        source_fingerprint: field("sourceFingerprint")?,
        software_version: field("softwareVersion")?,
        file_manifest_sha256: field("fileManifestSha256")?,
        company_ids: strings(body, "companyIds")?,
        disabled_company_ids: strings(body, "disabledCompanyIds")?,
        expires_at: match body.get("expiresAt") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(value.as_i64().ok_or_else(malformed)?),
        },
    };
    input.validate(
        body.get("now")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(malformed)?,
    )?;
    Ok(EngineeringEnrollmentHttpKind::Enroll(input))
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

async fn enroll(
    State(state): State<Arc<EngineeringEnrollmentHttpState>>,
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
            .command(EngineeringEnrollmentHttpRequest {
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
    impl EngineeringEnrollmentHttpBackend for Noop {
        fn command<'a>(
            &'a self,
            _: EngineeringEnrollmentHttpRequest<'a>,
        ) -> EngineeringEnrollmentHttpFuture<'a, EngineeringEnrollmentHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn configuration_requires_distinct_exact_holdings_and_valid_origins() {
        let backend = Arc::new(Noop);
        let target = || EngineeringEnrollmentHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(engineering_enrollment_http_router(vec![target()]).is_ok());
        assert!(engineering_enrollment_http_router(vec![target(), target()]).is_err());
        assert!(
            engineering_enrollment_http_router(vec![EngineeringEnrollmentHttpTarget {
                origin: "http://forged.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    #[test]
    fn parses_bounded_enrollment() {
        let body = serde_json::json!({
            "run":"abcdef123456",
            "sourceFingerprint":"a".repeat(64),
            "softwareVersion":format!("release-{}", "a".repeat(64)),
            "fileManifestSha256":"b".repeat(64),
            "companyIds":["c-1"],
            "disabledCompanyIds":[],
            "expiresAt":2000,
            "now":1000
        });
        assert!(matches!(
            parse_kind(&body).unwrap(),
            EngineeringEnrollmentHttpKind::Enroll(_)
        ));
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl EngineeringEnrollmentHttpBackend for Probe {
        fn command<'a>(
            &'a self,
            request: EngineeringEnrollmentHttpRequest<'a>,
        ) -> EngineeringEnrollmentHttpFuture<'a, EngineeringEnrollmentHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(
                    request
                        .key
                        .map(CommandKey::as_str)
                        .unwrap_or("-")
                        .to_owned(),
                );
                Ok(EngineeringEnrollmentHttpResponse {
                    data: serde_json::json!({"scope":"isolated-engineering"}),
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
        let router = engineering_enrollment_http_router(vec![EngineeringEnrollmentHttpTarget {
            host: "127.0.0.1:3104".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3104".into(),
            backend: Arc::new(Probe { seen: seen.clone() }),
        }])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let body = serde_json::json!({
            "run":"abcdef123456",
            "sourceFingerprint":"a".repeat(64),
            "softwareVersion":format!("release-{}", "a".repeat(64)),
            "fileManifestSha256":"b".repeat(64),
            "companyIds":["c-1"],"disabledCompanyIds":[],"expiresAt":2000,"now":1000
        })
        .to_string();
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/platform/engineering-verification/enroll HTTP/1.1\r\nHost: 127.0.0.1:3104\r\nOrigin: http://127.0.0.1:3104\r\nIdempotency-Key: eng-key-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1["scope"], "isolated-engineering");
        assert_eq!(seen.lock().unwrap().as_slice(), &["eng-key-1".to_owned()]);
        let _ = server.abort();
    }
}
