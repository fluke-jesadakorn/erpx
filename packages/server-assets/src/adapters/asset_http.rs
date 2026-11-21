//! Native HTTP ingress for the physical Asset register.
//!
//! Transport facts only: registered Host/Holding targets, origin, idempotency,
//! bounded JSON and error mapping. Normal Account session resolution, current
//! Company/branch/department scope and the serializable owner unit are supplied
//! by the composition backend, which must implement `AssetHttpBackend`.
use crate::application::asset::{AssetDetail, AssetWorkspace};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type AssetHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRenderedContext {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub assignment_id: String,
    pub assignment_version: i64,
    pub revision: i64,
    pub role: String,
    pub assignment_scope_kind: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssetWorkspaceResponse {
    pub data: AssetWorkspace,
    pub context: AssetRenderedContext,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssetDetailResponse {
    pub data: AssetDetail,
    pub context: AssetRenderedContext,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssetCommandResponse {
    pub data: Value,
    pub context: AssetRenderedContext,
}

pub enum AssetHttpReadReply {
    Workspace(Box<AssetWorkspaceResponse>),
    Detail(Box<AssetDetailResponse>),
}

pub struct AssetHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: Option<&'a str>,
}

pub struct AssetHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    /// Bounded raw JSON bytes. The composition backend owns parsing so it can
    /// retain the original UTF-16 units used by the Workflow fingerprint.
    pub body: &'a [u8],
    pub key: &'a CommandKey,
}

/// Composition-owned authentication, scope and owner-unit adapter. The owner
/// router never mints an actor or a Company.
pub trait AssetHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: AssetHttpReadRequest<'a>,
    ) -> AssetHttpFuture<'a, AssetHttpReadReply>;
    fn command<'a>(
        &'a self,
        request: AssetHttpCommandRequest<'a>,
    ) -> AssetHttpFuture<'a, AssetCommandResponse>;
}

pub struct AssetHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn AssetHttpBackend>,
}

struct AssetHttpState {
    targets: BTreeMap<(String, String), AssetHttpTarget>,
}

#[derive(Deserialize)]
struct AssetReadQuery {
    id: Option<String>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "ASSET_COMMAND_REQUIRED",
        "Choose a current Asset registration or lifecycle action.",
    )
}

fn unique_header(
    headers: &HeaderMap,
    name: axum::http::header::HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::invalid(
            "ASSET_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    first
        .map(|value| {
            value.to_str().map_err(|_| {
                BusinessError::invalid("ASSET_REQUEST_INVALID", "Use a valid request header.")
            })
        })
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        if rest.is_empty()
            || rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@'))
        {
            return false;
        }
        // The keyed development container serves exact Holding origins on its
        // local HTTPS port. Production origins remain port-free.
        if let Some((host, port)) = rest.rsplit_once(':') {
            return host.ends_with(".erpx.localhost") && port == "3443";
        }
        true
    })
}

pub fn asset_router(targets: Vec<AssetHttpTarget>) -> Result<Router, BusinessError> {
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
                "ASSETS_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/assets",
            get(read).post(command),
        )
        .with_state(Arc::new(AssetHttpState {
            targets: configured,
        })))
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
    if error.code == "ASSET_REGISTRATION_MONEY_INTERNAL" {
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":{"code":"INTERNAL_ERROR","message":"The request could not be completed."}})))
            .into_response();
    }
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded<T: Serialize>(body: &T) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, axum::http::HeaderValue::from_static(value));
    }
    response
}

fn target<'a>(
    state: &'a AssetHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a AssetHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn read(
    State(state): State<Arc<AssetHttpState>>,
    Path(slug): Path<String>,
    Query(query): Query<AssetReadQuery>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let id = query.id.map(RecordId::new).transpose()?;
        target
            .backend
            .read(AssetHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                id: id.as_ref().map(RecordId::as_str),
            })
            .await
    }
    .await;
    match result {
        Ok(AssetHttpReadReply::Workspace(reply)) => bounded(&reply),
        Ok(AssetHttpReadReply::Detail(reply)) => bounded(&reply),
        Err(error) => error_response(error),
    }
}

async fn command(
    State(state): State<Arc<AssetHttpState>>,
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
            axum::http::HeaderName::from_static("idempotency-key"),
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
                BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller Asset command.")
            })?;
        target
            .backend
            .command(AssetHttpCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                body: &body,
                key: &key,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}
