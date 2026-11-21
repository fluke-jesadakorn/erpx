//! Owner-local HTTP ingress for Commercial package grant preview/apply and
//! reviewed package change batches.
//!
//! Transport facts only: registered Platform Host, exact browser Origin,
//! Idempotency-Key, bounded JSON and business-error mapping. The composition
//! backend resolves the normal Account session, the current Platform assignment
//! and the target Holding/Company before invoking the owner application. This
//! router never mints a session, Platform role, Holding, Company, review or
//! entitlement.
use crate::application::grant_batches::{GrantBatch, normalize_lifecycle_batch};
use crate::application::grant_commands::GrantChange;
use crate::domain::packages::js_trim;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 800 * 1024;
const BODY_CHARS: usize = 200_000;

pub type PackageGrantHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum PackageGrantHttpCommand {
    Preview(GrantChange),
    Apply(GrantChange),
    PreviewBatch(GrantBatch),
    ApplyBatch { input: GrantBatch, token: String },
}

impl PackageGrantHttpCommand {
    pub fn is_apply(&self) -> bool {
        matches!(self, Self::Apply(_) | Self::ApplyBatch { .. })
    }

    pub fn idempotency_key(&self) -> &str {
        match self {
            Self::Preview(input) | Self::Apply(input) => &input.idempotency_key,
            Self::PreviewBatch(input) | Self::ApplyBatch { input, .. } => &input.idempotency_key,
        }
    }

    pub fn company_id(&self) -> &str {
        match self {
            Self::Preview(input) | Self::Apply(input) => &input.grant.company_id,
            Self::PreviewBatch(input) | Self::ApplyBatch { input, .. } => &input.company_id,
        }
    }
}

pub struct PackageGrantHttpRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
    /// The Holding selected by the reviewed request body, never by a route
    /// parameter, query flag or client-supplied authority.
    pub holding_id: &'a str,
    pub command: PackageGrantHttpCommand,
}

#[derive(Clone, Debug)]
pub struct PackageGrantHttpResponse {
    /// Owner reply data. The composition backend adds the validated context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl PackageGrantHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, Platform authority and owner-unit adapter.
/// The router never mints a session, actor, Holding, Company or review token.
pub trait PackageGrantsHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PackageGrantHttpRequest<'a>,
    ) -> PackageGrantHttpFuture<'a, PackageGrantHttpResponse>;
}

pub struct PackageGrantsHttpTarget {
    pub host: String,
    /// Exact browser origin for the registered Platform Host, for example
    /// `https://erpx-internal.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PackageGrantsHttpBackend>,
}

struct PackageGrantHttpState {
    targets: BTreeMap<String, PackageGrantsHttpTarget>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GrantRequest {
    holding_id: String,
    request: GrantChange,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchRequest {
    holding_id: String,
    request: GrantBatch,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchApplyRequest {
    holding_id: String,
    request: GrantBatch,
    token: String,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_REQUEST_INVALID",
        "Use one unambiguous Commercial package request.",
    )
}

fn input_invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Use the reviewed Company package request.",
    )
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

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn bounded_text(value: &str, min: usize, max: usize) -> bool {
    let count = value.encode_utf16().count();
    count >= min && count <= max
}

fn valid_holding(value: &str) -> bool {
    bounded_text(value, 1, 100) && !value.chars().any(char::is_control)
}

fn valid_grant(grant: &GrantChange) -> bool {
    let change = &grant.grant;
    grant.expected_version >= 0
        && bounded_text(&change.company_id, 1, 100)
        && bounded_text(&change.package_id, 1, 100)
        && change.package_version >= 1
        && change.version >= 1
        && change.starts_at >= 0
        && change.ends_at.is_none_or(|end| end > change.starts_at)
        && matches!(change.state.as_str(), "active" | "draining" | "inactive")
        && bounded_text(js_trim(&change.reason), 3, 1000)
        && bounded_text(js_trim(&change.evidence), 3, 1000)
}

fn status(error: &BusinessError) -> StatusCode {
    match error.code {
        "JSON_REQUIRED" => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "REQUEST_TOO_LARGE" => StatusCode::PAYLOAD_TOO_LARGE,
        "STALE_PACKAGE_BATCH"
        | "STALE_PACKAGE_GRANT"
        | "IDEMPOTENCY_CONFLICT"
        | "PACKAGE_DEPENDENCY_REQUIRED"
        | "PACKAGE_DEPENDENCY_PERIOD_REQUIRED"
        | "PACKAGE_DEPENDENCY_IN_USE"
        | "PACKAGE_DEPENDENCY_PERIOD_CONFLICT"
        | "DRAIN_PACKAGE_FIRST"
        | "PACKAGE_OBLIGATIONS_REMAIN"
        | "PACKAGE_OBLIGATION_SOURCES_REQUIRED" => StatusCode::CONFLICT,
        _ => match error.kind {
            FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
            FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
            FailureKind::Forbidden => StatusCode::FORBIDDEN,
            FailureKind::NotFound => StatusCode::NOT_FOUND,
            FailureKind::Conflict => StatusCode::CONFLICT,
            FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        },
    }
}

fn private_headers(mut response: Response) -> Response {
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

fn error_response(error: BusinessError) -> Response {
    private_headers(
        (
            status(&error),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
    )
}

fn bounded(value: serde_json::Value) -> Response {
    private_headers((StatusCode::OK, Json(value)).into_response())
}

impl PackageGrantsHttpTarget {
    fn service(targets: Vec<Self>) -> Result<BTreeMap<String, Self>, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || !valid_origin(&target.origin)
                || target.origin.strip_prefix("https://") != Some(target.host.as_str())
                || configured.insert(target.host.clone(), target).is_some()
            {
                return Err(BusinessError::invalid(
                    "COMMERCIAL_CONFIGURATION_INVALID",
                    "Use exact distinct registered Platform Host targets.",
                ));
            }
        }
        Ok(configured)
    }

    fn find<'a>(
        targets: &'a BTreeMap<String, Self>,
        headers: &HeaderMap,
    ) -> Result<&'a Self, BusinessError> {
        let host = unique_header(headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        targets.get(host).ok_or_else(missing)
    }
}

pub fn package_grants_router(
    targets: Vec<PackageGrantsHttpTarget>,
) -> Result<Router, BusinessError> {
    let targets = PackageGrantsHttpTarget::service(targets)?;
    Ok(Router::new()
        .route(
            "/api/v1/platform/commercial/package-grants/preview",
            post(preview),
        )
        .route(
            "/api/v1/platform/commercial/package-grants/apply",
            post(apply),
        )
        .route(
            "/api/v1/platform/commercial/package-grants/batches/preview",
            post(preview_batch),
        )
        .route(
            "/api/v1/platform/commercial/package-grants/batches/apply",
            post(apply_batch),
        )
        .with_state(Arc::new(PackageGrantHttpState { targets })))
}

async fn preview(State(state): State<Arc<PackageGrantHttpState>>, request: Request) -> Response {
    submit(state, request, Operation::Preview).await
}

async fn apply(State(state): State<Arc<PackageGrantHttpState>>, request: Request) -> Response {
    submit(state, request, Operation::Apply).await
}

async fn preview_batch(
    State(state): State<Arc<PackageGrantHttpState>>,
    request: Request,
) -> Response {
    submit(state, request, Operation::PreviewBatch).await
}

async fn apply_batch(
    State(state): State<Arc<PackageGrantHttpState>>,
    request: Request,
) -> Response {
    submit(state, request, Operation::ApplyBatch).await
}

#[derive(Clone, Copy)]
enum Operation {
    Preview,
    Apply,
    PreviewBatch,
    ApplyBatch,
}

impl Operation {
    fn apply(self) -> bool {
        matches!(self, Self::Apply | Self::ApplyBatch)
    }
}

async fn submit(
    state: Arc<PackageGrantHttpState>,
    request: Request,
    operation: Operation,
) -> Response {
    match submit_result(state, request, operation).await {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn submit_result(
    state: Arc<PackageGrantHttpState>,
    request: Request,
    operation: Operation,
) -> Result<PackageGrantHttpResponse, BusinessError> {
    let (parts, body) = request.into_parts();
    let target = PackageGrantsHttpTarget::find(&state.targets, &parts.headers)?;
    if unique_header(&parts.headers, header::ORIGIN)? != Some(target.origin.as_str()) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use ERPX Administration to submit this request.",
        ));
    }
    if unique_header(&parts.headers, header::CONTENT_TYPE)?.is_none_or(|value| {
        !value
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return Err(BusinessError::invalid(
            "JSON_REQUIRED",
            "Submit a JSON request.",
        ));
    }
    let bytes = to_bytes(body, BODY_LIMIT)
        .await
        .map_err(|_| BusinessError::invalid("REQUEST_TOO_LARGE", "This request is too large."))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Submit a valid JSON request."))?;
    if text.encode_utf16().count() > BODY_CHARS {
        return Err(BusinessError::invalid(
            "REQUEST_TOO_LARGE",
            "This request is too large.",
        ));
    }
    let key = unique_header(&parts.headers, HeaderName::from_static("idempotency-key"))?
        .map(CommandKey::new)
        .transpose()?;
    let command = decode(text, operation, key.as_ref())?;
    target
        .backend
        .execute(PackageGrantHttpRequest {
            host: &target.host,
            headers: &parts.headers,
            holding_id: &command.0,
            command: command.1,
        })
        .await
}

fn decode(
    text: &str,
    operation: Operation,
    key: Option<&CommandKey>,
) -> Result<(String, PackageGrantHttpCommand), BusinessError> {
    if operation.apply() && key.is_none() {
        return Err(BusinessError::invalid(
            "IDEMPOTENCY_KEY_REQUIRED",
            "Use the reviewed Idempotency-Key.",
        ));
    }
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Submit a valid JSON request."))?;
    let command = match operation {
        Operation::Preview => {
            let request: GrantRequest =
                serde_json::from_value(value).map_err(|_| input_invalid())?;
            if !valid_holding(&request.holding_id) || !valid_grant(&request.request) {
                return Err(input_invalid());
            }
            (
                request.holding_id,
                PackageGrantHttpCommand::Preview(request.request),
            )
        }
        Operation::Apply => {
            let request: GrantRequest =
                serde_json::from_value(value).map_err(|_| input_invalid())?;
            if !valid_holding(&request.holding_id) || !valid_grant(&request.request) {
                return Err(input_invalid());
            }
            (
                request.holding_id,
                PackageGrantHttpCommand::Apply(request.request),
            )
        }
        Operation::PreviewBatch => {
            let request: BatchRequest =
                serde_json::from_value(value).map_err(|_| input_invalid())?;
            if !valid_holding(&request.holding_id) {
                return Err(input_invalid());
            }
            let input = normalize_lifecycle_batch(request.request)?;
            (
                request.holding_id,
                PackageGrantHttpCommand::PreviewBatch(input),
            )
        }
        Operation::ApplyBatch => {
            let request: BatchApplyRequest =
                serde_json::from_value(value).map_err(|_| input_invalid())?;
            if !valid_holding(&request.holding_id)
                || request.token.is_empty()
                || request.token.len() > 8192
            {
                return Err(input_invalid());
            }
            let input = normalize_lifecycle_batch(request.request)?;
            (
                request.holding_id,
                PackageGrantHttpCommand::ApplyBatch {
                    input,
                    token: request.token,
                },
            )
        }
    };
    if operation.apply() && key.map(CommandKey::as_str) != Some(command.1.idempotency_key()) {
        return Err(BusinessError::invalid(
            "IDEMPOTENCY_KEY_REQUIRED",
            "Use the reviewed Idempotency-Key.",
        ));
    }
    Ok(command)
}
