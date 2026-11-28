//! One axum command router that EXECUTES identity owner writes.
//!
//! Transport facts only: exact registered Host/Holding target and browser
//! origin for the Holding route, one bounded Idempotency-Key, a typed owner
//! allow-listed `kind` and a full-parity positional `values` array. The backend
//! (composition) resolves Company/actor and runs the statement inside one
//! serializable owner unit. No SQL is executed here and no TS SQL mirror exists.
use crate::adapters::identity_write::{OwnerWriteOutcome, supported};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 256 * 1024;
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type IdentityWriteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<OwnerWriteOutcome, BusinessError>> + Send + 'a>>;

/// One native owner-write command. `kind` is the owner allow-list discriminator;
/// `values` are positional arguments and never SQL text.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityWriteCommand {
    pub kind: String,
    pub values: Vec<Value>,
    #[serde(default)]
    pub now: i64,
    #[serde(default)]
    pub company_id: String,
}

pub struct IdentityWriteRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub company_id: &'a str,
    pub command: &'a IdentityWriteCommand,
    pub key: &'a CommandKey,
}

/// Composition-owned session/Company/serializable-unit adapter.
pub trait IdentityWriteBackend: Send + Sync {
    fn execute<'a>(&'a self, request: IdentityWriteRequest<'a>) -> IdentityWriteFuture<'a>;
}

#[derive(Clone)]
pub struct IdentityWriteTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn IdentityWriteBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), IdentityWriteTarget>,
    company_targets: BTreeMap<String, Arc<dyn IdentityWriteBackend>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "IDENTITY_OWNER_WRITE_INVALID",
        "Review the identity write request.",
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
        "IDENTITY_OWNER_WRITE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
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

fn bounded(outcome: OwnerWriteOutcome) -> Response {
    let mut response = (StatusCode::OK, Json(serde_json::json!({"data": outcome}))).into_response();
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

fn host_of(headers: &HeaderMap) -> Result<String, BusinessError> {
    unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .map(|value| value.split(':').next().unwrap_or_default().to_owned())
        .ok_or_else(missing)
}

fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a IdentityWriteTarget, BusinessError> {
    let host = host_of(headers)?;
    let target = state
        .targets
        .get(&(host, slug.to_owned()))
        .ok_or_else(missing)?;
    match unique_header(headers, header::ORIGIN)? {
        Some(origin) if origin == target.origin => Ok(()),
        _ => Err(forbidden()),
    }
    .map(|()| target)
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

async fn bounded_body(request: Request) -> Result<IdentityWriteCommand, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    let command: IdentityWriteCommand = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if !supported(&command.kind) || command.values.is_empty() || command.values.len() > 64 {
        return Err(invalid());
    }
    Ok(command)
}

/// Browser-facing Holding route: exact Host/Origin + one Idempotency-Key.
pub fn identity_write_router(targets: Vec<IdentityWriteTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
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
            "/api/v1/holdings/{holding_slug}/identity/owner-write",
            post(write),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            company_targets: BTreeMap::new(),
        })))
}

/// Company-keyed private route for internal workers without a browser context.
pub fn identity_write_company_router(
    companies: Vec<(String, Arc<dyn IdentityWriteBackend>)>,
) -> Result<Router, BusinessError> {
    let mut company_targets = BTreeMap::new();
    for (company, backend) in companies {
        if company.is_empty()
            || company.len() > 200
            || company_targets.insert(company, backend).is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if company_targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/identity/owner-write",
            post(company_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            company_targets,
        })))
}

async fn write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let key = mutation_key(&headers)?;
        let command = bounded_body(request).await?;
        let company = if command.company_id.is_empty() {
            unique_header(&headers, HeaderName::from_static("x-erpx-context-company"))?
                .filter(|value| !value.is_empty())
                .ok_or_else(invalid)?
        } else {
            command.company_id.as_str()
        };
        target
            .backend
            .execute(IdentityWriteRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                company_id: company,
                command: &command,
                key: &key,
            })
            .await
    }
    .await;
    match result {
        Ok(outcome) => bounded(outcome),
        Err(error) => error_response(error),
    }
}

async fn company_write(
    State(state): State<Arc<AppState>>,
    Path(company): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state.company_targets.get(&company).ok_or_else(missing)?;
        let key = mutation_key(&headers)?;
        let command = bounded_body(request).await?;
        backend
            .execute(IdentityWriteRequest {
                host: "",
                holding_slug: "",
                headers: &headers,
                company_id: &company,
                command: &command,
                key: &key,
            })
            .await
    }
    .await;
    match result {
        Ok(outcome) => bounded(outcome),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Noop;
    impl IdentityWriteBackend for Noop {
        fn execute<'a>(&'a self, _: IdentityWriteRequest<'a>) -> IdentityWriteFuture<'a> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Noop);
        let target = || IdentityWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(identity_write_router(vec![target()]).is_ok());
        assert!(identity_write_router(vec![target(), target()]).is_err());
        assert!(
            identity_write_router(vec![IdentityWriteTarget {
                origin: "http://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl IdentityWriteBackend for Probe {
        fn execute<'a>(&'a self, request: IdentityWriteRequest<'a>) -> IdentityWriteFuture<'a> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}",
                    request.key.as_str(),
                    request.command.kind
                ));
                Ok(OwnerWriteOutcome {
                    kind: request.command.kind.clone(),
                    rows: 1,
                })
            })
        }
    }

    #[tokio::test]
    async fn actual_router_executes_with_target_origin_and_key() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = identity_write_router(vec![IdentityWriteTarget {
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
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let body = r#"{"kind":"iam_throttle_increment","values":["key",1,0,0],"companyId":"c1"}"#;
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(format!("POST /api/v1/holdings/acme/identity/owner-write HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nIdempotency-Key: identity-key-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("200 OK"), "{text}");
        assert!(text.contains("iam_throttle_increment"), "{text}");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["identity-key-1:iam_throttle_increment".to_owned()]
        );
        let _ = server.abort();
    }
}
