//! Owner-local HTTP ingress for Platform Administration.
//!
//! Transport facts only: exact registered Platform Host, browser Origin,
//! idempotency, bounded JSON and business-error mapping. The composition
//! backend resolves the normal Account session, current Platform assignment and
//! Holding `erpx` scope before calling the owner application. The router never
//! mints a session, Platform role, Holding scope or permission.
use crate::domain::platform_operations::{
    ProvisionHoldingInput, RecoverDeadLetterInput, RunHoldingMigrationInput,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Request, Response, Router, State,
    StatusCode, body::to_bytes, get, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type PlatformHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoverDeadLetterRequest {
    holding_id: String,
    event_id: String,
    expected_attempts: i64,
    expected_last_error: Option<String>,
    reason: String,
}

#[derive(Clone, Debug)]
pub enum PlatformHttpCommand {
    Provision(ProvisionHoldingInput),
    Recover {
        holding_id: String,
        input: RecoverDeadLetterInput,
    },
    Migrate(RunHoldingMigrationInput),
}

pub struct PlatformHttpReadRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct PlatformHttpCommandRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a CommandKey,
    pub command: PlatformHttpCommand,
}

#[derive(Clone, Debug)]
pub struct PlatformHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl PlatformHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, Platform role and owner-unit adapter.
pub trait PlatformHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: PlatformHttpReadRequest<'a>,
    ) -> PlatformHttpFuture<'a, PlatformHttpResponse>;
    fn command<'a>(
        &'a self,
        request: PlatformHttpCommandRequest<'a>,
    ) -> PlatformHttpFuture<'a, PlatformHttpResponse>;
}

pub struct PlatformHttpTarget {
    pub host: String,
    /// Exact browser origin for the registered Platform Holding, for example
    /// `https://erpx-internal.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PlatformHttpBackend>,
}

pub struct PlatformHttpService {
    targets: BTreeMap<String, PlatformHttpTarget>,
}

struct PlatformHttpState {
    service: Arc<PlatformHttpService>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed(
            "PLATFORM_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| malformed("PLATFORM_REQUEST_INVALID", "Use a valid request header."))
        })
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

impl PlatformHttpService {
    pub fn new(targets: Vec<PlatformHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || !valid_origin(&target.origin)
                || target.origin.strip_prefix("https://") != Some(target.host.as_str())
                || configured.insert(target.host.clone(), target).is_some()
            {
                return Err(malformed(
                    "PLATFORM_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Platform Host targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(&'a self, headers: &HeaderMap) -> Result<&'a PlatformHttpTarget, BusinessError> {
        let host = unique_header(headers, header::HOST)?.ok_or_else(missing)?;
        self.targets.get(host).ok_or_else(missing)
    }

    pub async fn read(&self, headers: &HeaderMap) -> Response {
        let target = match self.target(headers) {
            Ok(target) => target,
            Err(error) => return error_response(error),
        };
        match target
            .backend
            .read(PlatformHttpReadRequest {
                host: &target.host,
                headers,
            })
            .await
        {
            Ok(reply) => bounded(reply.value()),
            Err(error) => error_response(error),
        }
    }

    pub async fn command(
        &self,
        headers: &HeaderMap,
        body: &[u8],
        operation: PlatformCommandOperation,
    ) -> Response {
        match self.command_result(headers, body, operation).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn command_result(
        &self,
        headers: &HeaderMap,
        body: &[u8],
        operation: PlatformCommandOperation,
    ) -> Result<Response, BusinessError> {
        let target = self.target(headers)?;
        let origin = unique_header(headers, header::ORIGIN)?;
        if origin != Some(target.origin.as_str()) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = unique_header(headers, HeaderName::from_static("idempotency-key"))?
            .ok_or_else(|| malformed("COMMAND_KEY_REQUIRED", "Idempotency-Key is required."))
            .and_then(CommandKey::new)?;
        if !unique_header(headers, header::CONTENT_TYPE)?.is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        }) {
            return Err(malformed(
                "PLATFORM_INPUT_INVALID",
                "Use a JSON Platform request body.",
            ));
        }
        if body.len() > BODY_LIMIT {
            return Err(malformed(
                "BODY_TOO_LARGE",
                "Use a smaller Platform request.",
            ));
        }
        let command = match operation {
            PlatformCommandOperation::Provision => {
                serde_json::from_slice::<ProvisionHoldingInput>(body)
                    .map(PlatformHttpCommand::Provision)
                    .map_err(|_| {
                        malformed(
                            "PLATFORM_INPUT_INVALID",
                            "Review the Holding provisioning request.",
                        )
                    })?
            }
            PlatformCommandOperation::Recover => {
                let request =
                    serde_json::from_slice::<RecoverDeadLetterRequest>(body).map_err(|_| {
                        malformed(
                            "PLATFORM_INPUT_INVALID",
                            "Review the failed job recovery request.",
                        )
                    })?;
                PlatformHttpCommand::Recover {
                    holding_id: request.holding_id,
                    input: RecoverDeadLetterInput {
                        event_id: request.event_id,
                        expected_attempts: request.expected_attempts,
                        expected_last_error: request.expected_last_error,
                        reason: request.reason,
                    },
                }
            }
            PlatformCommandOperation::Migrate => {
                serde_json::from_slice::<RunHoldingMigrationInput>(body)
                    .map(PlatformHttpCommand::Migrate)
                    .map_err(|_| {
                        malformed(
                            "PLATFORM_INPUT_INVALID",
                            "Review the Holding migration request.",
                        )
                    })?
            }
        };
        let reply = target
            .backend
            .command(PlatformHttpCommandRequest {
                host: &target.host,
                headers,
                key: &key,
                command,
            })
            .await?;
        Ok(bounded(reply.value()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformCommandOperation {
    Provision,
    Recover,
    Migrate,
}

pub fn platform_router(targets: Vec<PlatformHttpTarget>) -> Result<Router, BusinessError> {
    let service = Arc::new(PlatformHttpService::new(targets)?);
    Ok(Router::new()
        .route("/api/v1/platform/operations", get(read_operations))
        .route("/api/v1/platform/operations/provision", post(provision))
        .route("/api/v1/platform/operations/recover", post(recover))
        .route("/api/v1/platform/operations/migrate", post(migrate))
        .with_state(Arc::new(PlatformHttpState { service })))
}

async fn read_operations(
    State(state): State<Arc<PlatformHttpState>>,
    headers: HeaderMap,
) -> Response {
    state.service.read(&headers).await
}
async fn provision(
    State(state): State<Arc<PlatformHttpState>>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(malformed(
                "BODY_TOO_LARGE",
                "Use a smaller Platform request.",
            ));
        }
    };
    state
        .service
        .command(&headers, &body, PlatformCommandOperation::Provision)
        .await
}
async fn recover(
    State(state): State<Arc<PlatformHttpState>>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(malformed(
                "BODY_TOO_LARGE",
                "Use a smaller Platform request.",
            ));
        }
    };
    state
        .service
        .command(&headers, &body, PlatformCommandOperation::Recover)
        .await
}
async fn migrate(
    State(state): State<Arc<PlatformHttpState>>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(malformed(
                "BODY_TOO_LARGE",
                "Use a smaller Platform request.",
            ));
        }
    };
    state
        .service
        .command(&headers, &body, PlatformCommandOperation::Migrate)
        .await
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
    impl PlatformHttpBackend for Probe {
        fn read<'a>(
            &'a self,
            _: PlatformHttpReadRequest<'a>,
        ) -> PlatformHttpFuture<'a, PlatformHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push("read".into());
                if self.fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PLATFORM_OPERATIONS_UNAVAILABLE",
                        "Synthetic unavailable.",
                    ));
                }
                Ok(PlatformHttpResponse {
                    data: serde_json::json!({"provisioning":{"rows":[],"availableSlots":[]},"operations":{"holdings":[]}}),
                    context: serde_json::json!({"userId":"user"}),
                })
            })
        }
        fn command<'a>(
            &'a self,
            request: PlatformHttpCommandRequest<'a>,
        ) -> PlatformHttpFuture<'a, PlatformHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "command:{}:{:?}",
                    request.key.as_str(),
                    request.command
                ));
                Ok(PlatformHttpResponse {
                    data: serde_json::json!({"id":"provisioning","version":2,"status":"active"}),
                    context: serde_json::json!({"userId":"user"}),
                })
            })
        }
    }

    fn key() -> CommandKey {
        CommandKey::new("platform-http-key-01").unwrap()
    }

    #[test]
    fn target_configuration_requires_distinct_https_hosts() {
        let backend = Arc::new(Probe::default());
        let target = || PlatformHttpTarget {
            host: "erpx-internal.example.test".into(),
            origin: "https://erpx-internal.example.test".into(),
            backend: backend.clone(),
        };
        assert!(platform_router(vec![target()]).is_ok());
        assert!(platform_router(vec![target(), target()]).is_err());
        assert!(
            platform_router(vec![PlatformHttpTarget {
                origin: "http://erpx-internal.example.test".into(),
                ..target()
            }])
            .is_err()
        );
        assert!(
            platform_router(vec![PlatformHttpTarget {
                host: "other.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    #[test]
    fn command_decoding_rejects_unknown_or_invalid_fields_before_the_backend() {
        let provision = serde_json::from_slice::<ProvisionHoldingInput>(
            br#"{"holdingId":"h004","slug":"northstar","displayName":"Northstar","writeBinding":"H004_DB","readBinding":"H004_READ_DB","filesBinding":"H004_FILES","reason":"Reviewed onboarding and verified health."}"#,
        );
        assert!(provision.is_ok());
        let unknown = serde_json::from_slice::<ProvisionHoldingInput>(
            br#"{"holdingId":"h004","slug":"northstar","displayName":"Northstar","writeBinding":"H004_DB","readBinding":"H004_READ_DB","filesBinding":"H004_FILES","reason":"Reviewed onboarding and verified health.","extra":true}"#,
        );
        assert!(unknown.is_err());
        let recovery = serde_json::from_slice::<RecoverDeadLetterRequest>(
            br#"{"holdingId":"h001","eventId":"platform-recovery-event","expectedAttempts":6,"expectedLastError":"PROJECTOR_TIMEOUT","reason":"Provider is healthy."}"#,
        );
        assert!(recovery.is_ok());
        let missing = serde_json::from_slice::<RecoverDeadLetterRequest>(
            br#"{"eventId":"platform-recovery-event","expectedAttempts":6,"reason":"Provider is healthy."}"#,
        );
        assert!(missing.is_err());
        let domain = serde_json::from_slice::<RecoverDeadLetterInput>(
            br#"{"eventId":"platform-recovery-event","expectedAttempts":6,"expectedLastError":"PROJECTOR_TIMEOUT","reason":"Provider is healthy."}"#,
        );
        assert!(domain.is_ok());
        let unknown = serde_json::from_slice::<RecoverDeadLetterRequest>(
            br#"{"holdingId":"h001","eventId":"platform-recovery-event","expectedAttempts":6,"expectedLastError":"PROJECTOR_TIMEOUT","reason":"Provider is healthy.","extra":true}"#,
        );
        assert!(unknown.is_err());
        let migration = serde_json::from_slice::<RunHoldingMigrationInput>(
            br#"{"holdingId":"h001","track":"holding-write","reason":"Apply the reviewed release schema."}"#,
        );
        assert!(migration.is_ok());
        let wrong_track = serde_json::from_slice::<RunHoldingMigrationInput>(
            br#"{"holdingId":"h001","track":"holding-read","reason":"Apply the reviewed release schema."}"#,
        );
        assert!(wrong_track.is_ok());
        let unknown = serde_json::from_slice::<RunHoldingMigrationInput>(
            br#"{"holdingId":"h001","track":"holding-write","reason":"Apply the reviewed release schema.","extra":true}"#,
        );
        assert!(unknown.is_err());
        assert_eq!(key().as_str(), "platform-http-key-01");
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
    async fn actual_router_retains_target_origin_key_body_and_error_mapping() {
        let probe = Arc::new(Probe::default());
        let failing = Arc::new(Probe {
            seen: Mutex::new(Vec::new()),
            fail: true,
        });
        let router = platform_router(vec![
            PlatformHttpTarget {
                host: "erpx-internal.example.test".into(),
                origin: "https://erpx-internal.example.test".into(),
                backend: probe.clone(),
            },
            PlatformHttpTarget {
                host: "fail.example.test".into(),
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
        let (status, body) = http(
            addr,
            "GET /api/v1/platform/operations HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["provisioning"]["rows"], serde_json::json!([]));
        let (status, body) = http(
            addr,
            "GET /api/v1/platform/operations HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 503);
        assert_eq!(body["error"]["code"], "PLATFORM_OPERATIONS_UNAVAILABLE");
        let (status, _) = http(
            addr,
            "GET /api/v1/platform/operations HTTP/1.1\r\nHost: missing.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 404);

        let post = |host: &str, origin: &str, key: Option<&str>, content: &str, body: &str| {
            format!(
                "POST /api/v1/platform/operations/provision HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: {content}\r\n{}Connection: close\r\nContent-Length: {}\r\n\r\n{body}",
                key.map(|key| format!("Idempotency-Key: {key}\r\n"))
                    .unwrap_or_default(),
                body.len()
            )
        };
        let provision = serde_json::json!({
            "holdingId":"h004","slug":"northstar","displayName":"Northstar",
            "writeBinding":"H004_DB","readBinding":"H004_READ_DB","filesBinding":"H004_FILES",
            "reason":"Reviewed onboarding and verified health."
        })
        .to_string();
        let (status, _) = http(
            addr,
            post(
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                Some("platform-http-key-01"),
                "application/json",
                &provision,
            ),
        )
        .await;
        assert_eq!(status, 200);
        let (status, _) = http(
            addr,
            post(
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                Some("platform-http-key-02"),
                "application/json",
                &provision,
            ),
        )
        .await;
        assert_eq!(status, 200);
        let migration = serde_json::json!({
            "holdingId":"h001","track":"holding-write",
            "reason":"Apply the reviewed release schema before enabling the next workflow."
        })
        .to_string();
        let (status, _) = http(
            addr,
            format!(
                "POST /api/v1/platform/operations/migrate HTTP/1.1\r\nHost: erpx-internal.example.test\r\nOrigin: https://erpx-internal.example.test\r\nContent-Type: application/json\r\nIdempotency-Key: platform-http-migration-01\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{migration}",
                migration.len()
            ),
        )
        .await;
        assert_eq!(status, 200);
        let (status, body) = http(
            addr,
            post(
                "erpx-internal.example.test",
                "https://evil.example.test",
                Some("platform-http-key-03"),
                "application/json",
                &provision,
            ),
        )
        .await;
        assert_eq!(status, 403);
        assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");
        let (status, body) = http(
            addr,
            post(
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                None,
                "application/json",
                &provision,
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");
        let (status, body) = http(
            addr,
            post(
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                Some("platform-http-key-03"),
                "text/plain",
                &provision,
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PLATFORM_INPUT_INVALID");
        let oversized = format!(r#"{{"reason":"{}"}}"#, "x".repeat(70_000));
        let (status, body) = http(
            addr,
            post(
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                Some("platform-http-key-04"),
                "application/json",
                &oversized,
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "BODY_TOO_LARGE");
        let seen = probe.seen.lock().unwrap().clone();
        assert!(seen.iter().any(|value| value == "read"));
        assert!(
            seen.iter()
                .any(|value| value.starts_with("command:platform-http-key-01"))
        );
        assert!(
            seen.iter()
                .any(|value| value.starts_with("command:platform-http-migration-01"))
        );
        assert!(
            seen.iter()
                .any(|value| value.contains("Migrate(RunHoldingMigrationInput"))
        );
        server.abort();
    }
}
