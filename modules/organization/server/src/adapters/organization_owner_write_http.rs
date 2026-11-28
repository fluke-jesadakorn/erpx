//! Single native axum command ingress for all Organization owner writes.
//!
//! Transport facts only: exact registered Host/Holding target, exact browser
//! origin for mutations, one Idempotency-Key, a typed allow-listed `kind` and
//! bounded positional values. The router never supplies SQL text or mints an
//! actor; composition resolves the current session/authority and runs the
//! owner executor on one serializable unit with receipt/audit/outbox.
use crate::adapters::organization_owner_write::{supported, OwnerWriteResult};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

/// One owner write command. `kind` is an owner allow-list discriminator and
/// `values` are positional arguments; caller input never carries SQL text.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerWriteCommand {
    pub kind: String,
    pub values: Vec<serde_json::Value>,
    #[serde(default)]
    pub now: i64,
    /// `execute` (default) runs the write and ledger on the owner unit.
    /// `statement` returns authoritative owner SQL for a caller that must keep
    /// it inside its own cross-owner transaction.
    #[serde(default)]
    pub mode: Option<String>,
}

impl OwnerWriteCommand {
    pub fn statement_mode(&self) -> bool {
        self.mode.as_deref() == Some("statement")
    }
}

pub type OwnerWriteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<OwnerWriteResult, BusinessError>> + Send + 'a>>;

pub struct OwnerWriteRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: &'a OwnerWriteCommand,
    pub key: &'a CommandKey,
}

/// Composition-owned session/authority/unit adapter.
pub trait OrganizationOwnerWriteBackend: Send + Sync {
    fn write<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a>;
}

pub struct OrganizationOwnerWriteTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn OrganizationOwnerWriteBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), OrganizationOwnerWriteTarget>,
    company_targets: BTreeMap<String, Arc<dyn OrganizationOwnerWriteBackend>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_OWNER_WRITE_INVALID",
        "Use one bounded organization owner write.",
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
        "ORGANIZATION_OWNER_WRITE_CONFIG_INVALID",
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
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a OrganizationOwnerWriteTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let target = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    let origin = unique_header(headers, ORIGIN_HEADER)?.ok_or_else(forbidden)?;
    if origin != target.origin {
        return Err(forbidden());
    }
    Ok(target)
}

fn decode(raw: serde_json::Value) -> Result<OwnerWriteCommand, BusinessError> {
    let command: OwnerWriteCommand = serde_json::from_value(raw).map_err(|_| malformed())?;
    if !supported(&command.kind) || command.values.is_empty() || command.values.len() > 64 {
        return Err(malformed());
    }
    Ok(command)
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

/// The one owner-write router. The composition root mounts it after supplying
/// exact registered Holding targets and a backend that resolves the session,
/// authority and serializable owner unit. System/worker callers that hold only
/// a Company id use the company-keyed route built by
/// [`organization_owner_write_company_router`].
pub fn organization_owner_write_router(
    targets: Vec<OrganizationOwnerWriteTarget>,
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
            "/api/v1/holdings/{holding_slug}/organization/owner-write",
            post(write),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            company_targets: BTreeMap::new(),
        })))
}

/// Company-keyed system route for workers holding a Company id but no browser
/// WorkContext. Reachable only on the private native API.
pub fn organization_owner_write_company_router(
    pairs: Vec<(String, Arc<dyn OrganizationOwnerWriteBackend>)>,
) -> Result<Router, BusinessError> {
    let mut company_targets = BTreeMap::new();
    for (company, backend) in pairs {
        if company.is_empty() || company.len() > 200 || company_targets.insert(company, backend).is_some() {
            return Err(configuration_invalid());
        }
    }
    if company_targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/organization/owner-write",
            post(write_company),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            company_targets,
        })))
}

async fn write_company(
    State(state): State<Arc<AppState>>,
    Path(company): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let backend = state
            .company_targets
            .get(&company)
            .ok_or_else(missing)?;
        let mut headers = request.headers().clone();
        headers.insert(
            HeaderName::from_static("x-erpx-context-company"),
            HeaderValue::from_str(&company).map_err(|_| malformed())?,
        );
        let headers = headers;
        let key = mutation_key(&headers)?;
        let command = decode(bounded_body(request).await?)?;
        backend
            .write(OwnerWriteRequest {
                host: "",
                holding_slug: "",
                headers: &headers,
                command: &command,
                key: &key,
            })
            .await
    }
    .await;
    match result {
        Ok(result) => bounded(serde_json::json!({"data": result})),
        Err(error) => error_response(error),
    }
}

async fn write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let target = target(&state, request.headers(), &slug)?;
        let headers = request.headers().clone();
        let key = mutation_key(&headers)?;
        let command = decode(bounded_body(request).await?)?;
        target
            .backend
            .write(OwnerWriteRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: &command,
                key: &key,
            })
            .await
    }
    .await;
    match result {
        Ok(result) => bounded(serde_json::json!({"data": result})),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Noop;
    impl OrganizationOwnerWriteBackend for Noop {
        fn write<'a>(&'a self, _: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Noop);
        let target = || OrganizationOwnerWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(organization_owner_write_router(vec![target()]).is_ok());
        assert!(organization_owner_write_router(vec![target(), target()]).is_err());
        assert!(
            organization_owner_write_router(vec![OrganizationOwnerWriteTarget {
                origin: "http://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    #[test]
    fn decode_rejects_unknown_kinds_and_empty_values() {
        assert!(decode(serde_json::json!({"kind":"not_a_kind","values":[1]})).is_err());
        assert!(decode(serde_json::json!({"kind":"node_archive","values":[]})).is_err());
        assert!(decode(serde_json::json!({"kind":"node_archive","values":["n","acme"],"now":1})).is_ok());
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl OrganizationOwnerWriteBackend for Probe {
        fn write<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}",
                    request.key.as_str(),
                    request.command.kind
                ));
                Ok(OwnerWriteResult {
                    kind: request.command.kind.clone(),
                    rows: 1,
                    version: 1,
                    sql: None,
                    values: Vec::new(),
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
    async fn actual_router_retains_target_origin_key_and_kind() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = organization_owner_write_router(vec![OrganizationOwnerWriteTarget {
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
        let body = r#"{"kind":"node_archive","values":["node","acme"],"now":1}"#;
        let response = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/organization/owner-write HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: command-key-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(response.1["data"]["rows"], 1);
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["command-key-1:node_archive".to_owned()]
        );
        let forbidden = http(
            addr,
            format!(
                "POST /api/v1/holdings/acme/organization/owner-write HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://other.example.test\r\nIdempotency-Key: command-key-2\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(forbidden.0, 403);
        let _ = server.abort();
    }
}
