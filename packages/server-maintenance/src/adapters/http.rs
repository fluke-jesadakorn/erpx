//! Native HTTP ingress for Maintenance work orders and preventive plans.
//!
//! Transport facts only: registered Host/Holding target, browser origin,
//! idempotency, bounded JSON and business-error mapping. Normal Account session
//! resolution, current Company/branch/department scope, Commercial admission and
//! the serializable owner command unit remain composition responsibilities.
use crate::application::work_commands::{
    MaintenanceCommandInputJson, MaintenanceWorkCommand, NormalizedMaintenanceChange,
    NormalizedMaintenanceCreate,
};
use crate::adapters::work_wire::{normalize_maintenance_change, normalize_maintenance_create};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
};

const BODY_LIMIT: usize = 64 * 1024;

pub type MaintenanceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum MaintenanceHttpReadKind {
    Workspace,
    Detail(String),
    Plans,
    PlanDetail(String),
}

pub struct MaintenanceHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: MaintenanceHttpReadKind,
}

#[derive(Clone, Debug)]
pub enum MaintenanceHttpCommand {
    Create {
        key: CommandKey,
        input: NormalizedMaintenanceCreate,
    },
    Change {
        key: CommandKey,
        id: RecordId,
        input: NormalizedMaintenanceChange,
    },
}
impl MaintenanceHttpCommand {
    pub fn key(&self) -> &CommandKey {
        match self {
            Self::Create { key, .. } | Self::Change { key, .. } => key,
        }
    }
    pub fn change_input(&self) -> Option<(&RecordId, &NormalizedMaintenanceChange)> {
        match self {
            Self::Change { id, input, .. } => Some((id, input)),
            Self::Create { .. } => None,
        }
    }
    pub fn create_input(&self) -> Option<&NormalizedMaintenanceCreate> {
        match self {
            Self::Create { input, .. } => Some(input),
            Self::Change { .. } => None,
        }
    }
    /// Exact canonical receipt bytes, before any owner writes.
    pub fn receipt_input(&self) -> &MaintenanceCommandInputJson {
        match self {
            Self::Create { input, .. } => input.receipt_input(),
            Self::Change { input, .. } => input.receipt_input(),
        }
    }
    /// Composition dispatch value for every work transition, including
    /// issue-part. Creation returns `None` until its native composition exists.
    pub fn change_command(&self) -> Option<MaintenanceWorkCommand> {
        match self {
            Self::Change { key, id, input } => {
                Some(input.clone().into_command(key.clone(), id.clone()))
            }
            Self::Create { .. } => None,
        }
    }
}

pub struct MaintenanceHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: MaintenanceHttpCommand,
}

#[derive(Clone, Debug)]
pub struct MaintenanceHttpResponse {
    /// Owner read/command data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl MaintenanceHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or purchase.
pub trait MaintenanceHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: MaintenanceHttpReadRequest<'a>,
    ) -> MaintenanceHttpFuture<'a, MaintenanceHttpResponse>;
    fn command<'a>(
        &'a self,
        request: MaintenanceHttpCommandRequest<'a>,
    ) -> MaintenanceHttpFuture<'a, MaintenanceHttpResponse>;
}

pub struct MaintenanceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn MaintenanceHttpBackend>,
}

struct MaintenanceHttpState {
    targets: BTreeMap<(String, String), MaintenanceHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_COMMAND_REQUIRED",
        "Choose a current Maintenance work or preventive plan action.",
    )
}
fn unique_header(
    headers: &HeaderMap,
    name: HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::invalid(
            "MAINTENANCE_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    first
        .map(|value| {
            value.to_str().map_err(|_| {
                BusinessError::invalid(
                    "MAINTENANCE_REQUEST_INVALID",
                    "Use a valid request header.",
                )
            })
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
    state: &'a MaintenanceHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a MaintenanceHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn query_id(request: &Request) -> Result<Option<String>, BusinessError> {
    let Some(query) = request.uri().query() else {
        return Ok(None);
    };
    let mut id = None;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == "id" {
            if id.is_some() {
                return Err(BusinessError::invalid(
                    "MAINTENANCE_REQUEST_INVALID",
                    "Use one Maintenance record identifier.",
                ));
            }
            let decoded = percent_decode(value)?;
            id = Some(RecordId::new(decoded)?.as_str().to_owned());
        }
    }
    Ok(id)
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
                return Err(BusinessError::invalid(
                    "MAINTENANCE_REQUEST_INVALID",
                    "Use a valid Maintenance record identifier.",
                ));
            };
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| {
        BusinessError::invalid(
            "MAINTENANCE_REQUEST_INVALID",
            "Use a valid Maintenance record identifier.",
        )
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

pub fn maintenance_router(targets: Vec<MaintenanceHttpTarget>) -> Result<Router, BusinessError> {
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
                "MAINTENANCE_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/maintenance",
            get(read_work).post(command_work),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/maintenance/plans",
            get(read_plans),
        )
        .with_state(Arc::new(MaintenanceHttpState {
            targets: configured,
        })))
}

async fn read_work(
    State(state): State<Arc<MaintenanceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let id = query_id(&request)?;
        let kind = id
            .map(MaintenanceHttpReadKind::Detail)
            .unwrap_or(MaintenanceHttpReadKind::Workspace);
        target
            .backend
            .read(MaintenanceHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                kind,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_plans(
    State(state): State<Arc<MaintenanceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let id = query_id(&request)?;
        let kind = id
            .map(MaintenanceHttpReadKind::PlanDetail)
            .unwrap_or(MaintenanceHttpReadKind::Plans);
        target
            .backend
            .read(MaintenanceHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                kind,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn command_work(
    State(state): State<Arc<MaintenanceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if unique_header(request.headers(), header::ORIGIN)?
            .is_some_and(|value| value != target.origin)
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = unique_header(
            request.headers(),
            HeaderName::from_static("idempotency-key"),
        )?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
        })
        .and_then(CommandKey::new)?;
        if !unique_header(request.headers(), header::CONTENT_TYPE)?.is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        }) {
            return Err(malformed());
        }
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| {
                BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller Maintenance command.")
            })?;
        let command = decode_command(&body, key)?;
        target
            .backend
            .command(MaintenanceHttpCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

/// Decode the original create/change body at the transport boundary. A body
/// with a nonempty `id` is a change; otherwise it is a create. No SQL or owner
/// effect is performed here.
fn decode_command(
    body: &[u8],
    key: CommandKey,
) -> Result<MaintenanceHttpCommand, BusinessError> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| malformed())?;
    let id = value
        .as_object()
        .and_then(|object| object.get("id"))
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty());
    match id {
        Some(id) => {
            let id = RecordId::new(id)?;
            let input = normalize_maintenance_change(body).map_err(wire_error)?;
            Ok(MaintenanceHttpCommand::Change { key, id, input })
        }
        None => {
            let input = normalize_maintenance_create(body).map_err(wire_error)?;
            Ok(MaintenanceHttpCommand::Create { key, input })
        }
    }
}

fn wire_error(error: crate::adapters::work_wire::MaintenanceWireFailure) -> BusinessError {
    if error.invalid_json {
        return malformed();
    }
    let first = error
        .issues
        .first()
        .map(|issue| issue.code)
        .unwrap_or("VALIDATION_FAILED");
    let message = match first {
        "invalid_type" => "Review the Maintenance input types.",
        "too_small" => "A Maintenance value is below its allowed size.",
        "too_big" => "A Maintenance value exceeds its allowed size.",
        "invalid_enum_value" | "invalid_value" => "Choose a supported Maintenance value.",
        _ => "Review the Maintenance input.",
    };
    BusinessError::invalid("MAINTENANCE_INPUT_INVALID", message)
}

/// The transport is the only place that sees an untrusted command body. The
/// backend receives the canonical normalized value, not the raw JSON.
pub fn decoded_operation(command: &MaintenanceHttpCommand) -> String {
    match command {
        MaintenanceHttpCommand::Create { .. } => "maintenance.create".into(),
        MaintenanceHttpCommand::Change { input, .. } => {
            format!("maintenance.{}", input.input().action.action())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> CommandKey {
        CommandKey::new("maintenance-http-key").unwrap()
    }

    #[test]
    fn create_and_change_bodies_are_decoded_before_the_backend() {
        let create = br#"{"assetId":"asset","assetVersion":1,"title":"Synthetic work","priority":"normal","ownerId":"user","scheduledAt":null,"reason":"Synthetic reason"}"#;
        let command = decode_command(create, key()).unwrap();
        assert_eq!(decoded_operation(&command), "maintenance.create");
        assert!(command.create_input().is_some());
        assert!(command.change_input().is_none());

        let change = br#"{"id":"work","action":"complete","expectedVersion":2,"resultingCondition":"good","evidence":"Synthetic evidence","reason":"Synthetic reason"}"#;
        let command = decode_command(change, key()).unwrap();
        assert_eq!(decoded_operation(&command), "maintenance.complete");
        let (id, input) = command.change_input().unwrap();
        assert_eq!(id.as_str(), "work");
        assert_eq!(input.input().action.action(), "complete");
        assert!(command.create_input().is_none());
    }

    #[test]
    fn malformed_and_invalid_bodies_fail_before_any_backend_call() {
        assert!(decode_command(br#"{"id":"work"}"#, key()).is_err());
        assert!(decode_command(br#"{"assetId":"","assetVersion":0,"title":"x","priority":"urgent","ownerId":"","reason":""}"#, key()).is_err());
        assert!(decode_command(b"not-json", key()).is_err());
    }

    #[test]
    fn every_work_action_selects_the_native_operation_and_change_command() {
        for (body, action) in [
            (
                br#"{"id":"work","action":"schedule","expectedVersion":2,"ownerId":"user","scheduledAt":1700000000000,"reason":"Synthetic reason"}"#.as_slice(),
                "schedule",
            ),
            (
                br#"{"id":"work","action":"start","expectedVersion":2,"reason":"Synthetic reason"}"#.as_slice(),
                "start",
            ),
            (
                br#"{"id":"work","action":"block","expectedVersion":2,"reason":"Synthetic reason"}"#.as_slice(),
                "block",
            ),
            (
                br#"{"id":"work","action":"resume","expectedVersion":2,"reason":"Synthetic reason"}"#.as_slice(),
                "resume",
            ),
            (
                br#"{"id":"work","action":"complete","expectedVersion":2,"resultingCondition":"good","evidence":"Synthetic evidence","reason":"Synthetic reason"}"#.as_slice(),
                "complete",
            ),
            (
                br#"{"id":"work","action":"issue-part","expectedVersion":2,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"entryDate":"2026-11-06","reason":"Synthetic reason"}"#.as_slice(),
                "issue-part",
            ),
            (
                br#"{"id":"work","action":"cancel","expectedVersion":2,"reason":"Synthetic reason"}"#.as_slice(),
                "cancel",
            ),
        ] {
            let command = decode_command(body, key()).unwrap();
            assert_eq!(decoded_operation(&command), format!("maintenance.{action}"));
            let work = command.change_command().unwrap();
            assert_eq!(work.key().as_str(), "maintenance-http-key");
            assert_eq!(work.work_id().as_str(), "work");
            assert_eq!(work.operation(), format!("maintenance.{action}"));
            assert_eq!(work.input().action.action(), action);
            assert_eq!(work.receipt_input(), command.receipt_input());
            assert!(command.create_input().is_none());
        }
    }

    #[test]
    fn creation_has_no_native_change_command_until_its_composition_exists() {
        let create = br#"{"assetId":"asset","assetVersion":1,"title":"Synthetic work","priority":"normal","ownerId":"user","scheduledAt":null,"reason":"Synthetic reason"}"#;
        let command = decode_command(create, key()).unwrap();
        assert!(command.change_command().is_none());
        assert!(command.change_input().is_none());
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let backend = std::sync::Arc::new(Noop);
        let target = || MaintenanceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(maintenance_router(vec![target()]).is_ok());
        assert!(maintenance_router(vec![target(), target()]).is_err());
        assert!(
            maintenance_router(vec![MaintenanceHttpTarget {
                origin: "http://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    struct Noop;
    impl MaintenanceHttpBackend for Noop {
        fn read<'a>(
            &'a self,
            _: MaintenanceHttpReadRequest<'a>,
        ) -> MaintenanceHttpFuture<'a, MaintenanceHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
        fn command<'a>(
            &'a self,
            _: MaintenanceHttpCommandRequest<'a>,
        ) -> MaintenanceHttpFuture<'a, MaintenanceHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    struct Probe {
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        fail: bool,
    }
    impl MaintenanceHttpBackend for Probe {
        fn read<'a>(
            &'a self,
            request: MaintenanceHttpReadRequest<'a>,
        ) -> MaintenanceHttpFuture<'a, MaintenanceHttpResponse> {
            Box::pin(async move {
                self.seen
                    .lock()
                    .unwrap()
                    .push(format!("read:{:?}", request.kind));
                if self.fail {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "MAINTENANCE_READ_CHANGED",
                        "Synthetic changed source.",
                    ));
                }
                Ok(MaintenanceHttpResponse {
                    data: serde_json::json!({"items":[]}),
                    context: serde_json::json!({"userId":"user"}),
                })
            })
        }
        fn command<'a>(
            &'a self,
            request: MaintenanceHttpCommandRequest<'a>,
        ) -> MaintenanceHttpFuture<'a, MaintenanceHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "command:{}:{}:{}",
                    request.command.key().as_str(),
                    decoded_operation(&request.command),
                    request.command.change_input().map(|(id, _)| id.as_str()).unwrap_or("create")
                ));
                Ok(MaintenanceHttpResponse {
                    data: serde_json::json!({"id":"work","version":2,"status":"completed"}),
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
        (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn actual_router_retains_targets_origin_key_body_and_error_mapping() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let probe = std::sync::Arc::new(Probe {
            seen: seen.clone(),
            fail: false,
        });
        let failing = std::sync::Arc::new(Probe {
            seen: seen.clone(),
            fail: true,
        });
        let router = maintenance_router(vec![
            MaintenanceHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                origin: "https://acme.example.test".into(),
                backend: probe,
            },
            MaintenanceHttpTarget {
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
            format!(
                "GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n"
            )
        };
        let (status, body) = http(addr, get("/api/v1/holdings/acme/maintenance")).await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["items"], serde_json::json!([]));
        let (status, _) = http(addr, get("/api/v1/holdings/acme/maintenance?id=work")).await;
        assert_eq!(status, 200);
        let (status, _) = http(addr, get("/api/v1/holdings/acme/maintenance/plans?id=plan")).await;
        assert_eq!(status, 200);
        let (status, body) = http(
            addr,
            get("/api/v1/holdings/acme/maintenance?id=work&id=other"),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "MAINTENANCE_REQUEST_INVALID");
        let (status, _) = http(addr, get("/api/v1/holdings/missing/maintenance")).await;
        assert_eq!(status, 404);
        let (status, body) = http(
            addr,
            "GET /api/v1/holdings/fail/maintenance HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 409);
        assert_eq!(body["error"]["code"], "MAINTENANCE_READ_CHANGED");

        let post = |host: &str, origin: &str, key: Option<&str>, content: &str, body: &str| {
            format!(
                "POST /api/v1/holdings/{}/maintenance HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: {content}\r\n{}Connection: close\r\nContent-Length: {}\r\n\r\n{body}",
                if host.starts_with("fail") { "fail" } else { "acme" },
                key.map(|key| format!("Idempotency-Key: {key}\r\n")).unwrap_or_default(),
                body.len()
            )
        };
        let change = serde_json::json!({"id":"work","action":"complete","expectedVersion":1,"resultingCondition":"good","evidence":"Repaired","reason":"Completed repairs"}).to_string();
        let (status, body) = http(
            addr,
            post("acme.example.test", "https://acme.example.test", Some("maintenance-key-1"), "application/json", &change),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["status"], "completed");
        let create = serde_json::json!({"assetId":"asset","assetVersion":1,"title":"Synthetic work","priority":"normal","ownerId":"user","scheduledAt":null,"reason":"Synthetic reason"}).to_string();
        let (status, _) = http(
            addr,
            post("acme.example.test", "https://acme.example.test", Some("maintenance-key-2"), "application/json", &create),
        )
        .await;
        assert_eq!(status, 200);
        let (status, body) = http(
            addr,
            post("acme.example.test", "https://evil.example.test", Some("maintenance-key-3"), "application/json", &change),
        )
        .await;
        assert_eq!(status, 403);
        assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");
        let (status, body) = http(
            addr,
            post("acme.example.test", "https://acme.example.test", None, "application/json", &change),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");
        let (status, body) = http(
            addr,
            post("acme.example.test", "https://acme.example.test", Some("maintenance-key-3"), "text/plain", &change),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "MAINTENANCE_COMMAND_REQUIRED");
        let oversized = format!(r#"{{"id":"work","reason":"{}"}}"#, "x".repeat(70_000));
        let (status, body) = http(
            addr,
            post("acme.example.test", "https://acme.example.test", Some("maintenance-key-4"), "application/json", &oversized),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "BODY_TOO_LARGE");
        let seen = seen.lock().unwrap().clone();
        assert!(seen.iter().any(|value| value == "read:Workspace"));
        assert!(seen.iter().any(|value| value == "read:Detail(\"work\")"));
        assert!(seen.iter().any(|value| value == "read:PlanDetail(\"plan\")"));
        assert!(seen.iter().any(|value| value == "command:maintenance-key-1:maintenance.complete:work"));
        assert!(seen.iter().any(|value| value == "command:maintenance-key-2:maintenance.create:create"));
        server.abort();
    }
}

