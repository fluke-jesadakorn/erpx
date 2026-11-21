//! Native axum command ingress for the POS owner-wide write slice.
//!
//! Transport facts only: typed allow-listed kind, positional values, exact
//! registered Holding Host/Origin (browser route), one Idempotency-Key and a
//! bounded body. The router never mints a session or authority; composition
//! resolves the current Company and opens the serializable owner unit. The
//! returned owner statement is executed there together with receipt/audit/outbox.
use crate::adapters::owner_writes::{supported, PosOwnerStatement};
use erpx_server_runtime::public::http_support::{
    header, routing::post, HeaderMap, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, body::to_bytes,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 256 * 1024;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "POS_OWNER_WRITE_INVALID",
        "Review the POS owner write request.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

/// One owner write command. `kind` is an owner allow-listed discriminator;
/// `values` are positional arguments and never SQL text.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosOwnerWriteCommand {
    pub company: String,
    pub kind: String,
    pub values: Vec<Value>,
}

pub type PosOwnerWriteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PosOwnerStatement, BusinessError>> + Send + 'a>>;

pub struct PosOwnerWriteRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub company: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: &'a str,
    pub values: &'a [Value],
    pub key: &'a str,
}

/// Composition-owned session, Company and serializable-unit adapter.
pub trait PosOwnerWriteBackend: Send + Sync {
    fn statement<'a>(&'a self, request: PosOwnerWriteRequest<'a>) -> PosOwnerWriteFuture<'a>;
}

#[derive(Clone)]
pub struct PosOwnerWriteTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn PosOwnerWriteBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), PosOwnerWriteTarget>,
    company_backends: BTreeMap<String, Arc<dyn PosOwnerWriteBackend>>,
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
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
            && !host.bytes().any(|b| matches!(b, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len()) && port.bytes().all(|b| b.is_ascii_digit())
            })
    })
}

fn decode(body: &[u8]) -> Result<PosOwnerWriteCommand, BusinessError> {
    if body.len() > BODY_LIMIT {
        return Err(invalid());
    }
    let command: PosOwnerWriteCommand = serde_json::from_slice(body).map_err(|_| invalid())?;
    if command.company.is_empty()
        || command.company.len() > 200
        || !supported(&command.kind)
        || command.values.is_empty()
        || command.values.len() > 64
    {
        return Err(invalid());
    }
    Ok(command)
}

fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    unique_header(headers, "idempotency-key")?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Use a valid Idempotency-Key for this action.",
            )
        })
        .and_then(CommandKey::new)
}

fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PosOwnerWriteTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .map(|value| value.split(':').next().unwrap_or_default().to_owned())
        .ok_or_else(missing)?;
    let target = state
        .targets
        .get(&(host, slug.to_owned()))
        .ok_or_else(missing)?;
    if unique_header(headers, "origin")?.is_some_and(|value| value != target.origin) {
        return Err(origin_forbidden());
    }
    Ok(target)
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

fn bounded(value: PosOwnerStatement) -> Response {
    let body = serde_json::json!({ "data": { "sql": value.sql, "values": value.values } });
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, header::HeaderValue::from_static(value));
    }
    response
}

struct Parsed {
    headers: HeaderMap,
    command: PosOwnerWriteCommand,
    key: CommandKey,
}

async fn parse(request: Request) -> Result<Parsed, BusinessError> {
    let headers = request.headers().clone();
    let content_type = unique_header(&headers, "content-type")?
        .ok_or_else(invalid)?
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if content_type != "application/json" {
        return Err(invalid());
    }
    let key = command_key(&headers)?;
    let body = to_bytes(request.into_body(), BODY_LIMIT + 1)
        .await
        .map_err(|_| invalid())?;
    let command = decode(&body)?;
    Ok(Parsed { headers, command, key })
}

async fn write(State(state): State<Arc<AppState>>, Path(slug): Path<String>, request: Request) -> Response {
    let result = async {
        let parsed = parse(request).await?;
        let target = target(&state, &parsed.headers, &slug)?;
        target
            .backend
            .statement(PosOwnerWriteRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                company: &parsed.command.company,
                headers: &parsed.headers,
                kind: &parsed.command.kind,
                values: &parsed.command.values,
                key: parsed.key.as_str(),
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => bounded(statement),
        Err(error) => error_response(error),
    }
}

async fn company_write(State(state): State<Arc<AppState>>, Path(company): Path<String>, request: Request) -> Response {
    let result = async {
        let parsed = parse(request).await?;
        let backend = state
            .company_backends
            .get(&company)
            .ok_or_else(missing)?
            .clone();
        backend
            .statement(PosOwnerWriteRequest {
                host: "",
                holding_slug: "",
                company: &company,
                headers: &parsed.headers,
                kind: &parsed.command.kind,
                values: &parsed.command.values,
                key: parsed.key.as_str(),
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => bounded(statement),
        Err(error) => error_response(error),
    }
}

/// Browser-facing router: exact registered Holding host/origin + one key.
pub fn pos_owner_write_router(targets: Vec<PosOwnerWriteTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || target.holding_slug.is_empty()
            || target.holding_slug.len() > 128
            || !target.holding_slug.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')
            })
            || !valid_origin(&target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "POS_OWNER_WRITE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pos/owner-write",
            post(write),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            company_backends: BTreeMap::new(),
        })))
}

/// System router keyed by exact Company id (private native API only).
pub fn pos_owner_write_company_router(
    pairs: Vec<(String, Arc<dyn PosOwnerWriteBackend>)>,
) -> Result<Router, BusinessError> {
    let mut company_backends = BTreeMap::new();
    for (company, backend) in pairs {
        if company.is_empty() || company.len() > 200 || company_backends.insert(company, backend).is_some() {
            return Err(BusinessError::invalid(
                "POS_OWNER_WRITE_CONFIG_INVALID",
                "Use exact distinct registered Company targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/pos/owner-write",
            post(company_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            company_backends,
        })))
}
