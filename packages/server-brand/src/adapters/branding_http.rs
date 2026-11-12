//! Native Branding HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded GET/POST
//! and the idempotency/origin contract. Current Account authority, Branding
//! policy, snapshots, receipts, audit and outbox remain composition
//! responsibilities behind the backend trait.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const KEY_LIMIT: usize = 180;
/// The verified Account context's Holding display label. Presentation only.
const PRESENTATION_NAME_HEADER: &str = "x-erpx-holding-name";

pub type BrandingHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BrandingHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct BrandingHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub body: serde_json::Value,
}

/// One retained Brand presentation read. Label initials, media URL precedence,
/// storefront overlay and document mode stay in the owner; this transport only
/// selects which native projection the current authority may read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrandingPresentationReadKind {
    Workspace,
    Document,
    Profiles,
    Storefront,
    Evidence,
}

impl BrandingPresentationReadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Document => "document",
            Self::Profiles => "profiles",
            Self::Storefront => "storefront",
            Self::Evidence => "evidence",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "workspace" => Some(Self::Workspace),
            "document" => Some(Self::Document),
            "profiles" => Some(Self::Profiles),
            "storefront" => Some(Self::Storefront),
            "evidence" => Some(Self::Evidence),
            _ => None,
        }
    }
}

/// Presentation facts the Brand owner cannot derive. The label is the Holding
/// name the verified Account context already carries for this request; it is a
/// display fact only and never selects a Company, Branch or media source.
pub struct BrandingPresentationHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub holding_name: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: BrandingPresentationReadKind,
    pub branch_id: Option<&'a str>,
    pub requested_mode: Option<&'a str>,
}

pub trait BrandingHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: BrandingHttpReadRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value>;
    fn presentation<'a>(
        &'a self,
        request: BrandingPresentationHttpRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value>;
    fn command<'a>(
        &'a self,
        request: BrandingHttpCommandRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value>;
}

pub struct BrandingHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn BrandingHttpBackend>,
}

struct BrandingHttpState {
    targets: BTreeMap<(String, String), BrandingHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "BRANDING_REQUEST_INVALID",
        "Use one bounded Branding request.",
    )
}
fn key_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
    )
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "BRANDING_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
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
fn respond(result: Result<serde_json::Value, BusinessError>) -> Response {
    match result {
        Ok(data) => bounded(serde_json::json!({"data": data})),
        Err(error) => error_response(error),
    }
}
fn target<'a>(
    state: &'a BrandingHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a BrandingHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn require_origin(
    target: &BrandingHttpTarget,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<(), BusinessError> {
    let origin = unique_header(headers, header::ORIGIN)?;
    if mutation && origin != Some(target.origin.as_str())
        || origin.is_some_and(|value| value != target.origin)
    {
        return Err(origin_forbidden());
    }
    Ok(())
}

async fn read(
    State(state): State<Arc<BrandingHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, false)?;
        target
            .backend
            .read(BrandingHttpReadRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
            })
            .await
    }
    .await;
    respond(result)
}

#[derive(serde::Deserialize)]
struct PresentationQuery {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

fn presentation_text(value: &str, max: usize, code: &'static str) -> Result<(), BusinessError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            code,
            "Use one bounded Branding presentation read.",
        ));
    }
    Ok(())
}

async fn presentation(
    State(state): State<Arc<BrandingHttpState>>,
    Path(slug): Path<String>,
    Query(query): Query<PresentationQuery>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, false)?;
        let invalid = || malformed();
        let kind = query
            .kind
            .as_deref()
            .and_then(BrandingPresentationReadKind::parse)
            .ok_or_else(invalid)?;
        let holding_name = unique_header(&headers, HeaderName::from_static(PRESENTATION_NAME_HEADER))?
            .ok_or_else(invalid)?;
        if holding_name.trim().is_empty()
            || holding_name.trim().chars().count() > 200
            || holding_name.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        for value in [query.branch.as_deref(), query.mode.as_deref()]
            .into_iter()
            .flatten()
        {
            presentation_text(value, 200, "BRANDING_REQUEST_INVALID")?;
        }
        let branch_id = match kind {
            BrandingPresentationReadKind::Evidence => {
                Some(query.branch.as_deref().ok_or_else(invalid)?)
            }
            BrandingPresentationReadKind::Document => query.branch.as_deref(),
            BrandingPresentationReadKind::Workspace
            | BrandingPresentationReadKind::Profiles
            | BrandingPresentationReadKind::Storefront => {
                if query.branch.is_some() || query.mode.is_some() {
                    return Err(invalid());
                }
                None
            }
        };
        target
            .backend
            .presentation(BrandingPresentationHttpRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                holding_name: holding_name.trim(),
                headers: &headers,
                kind,
                branch_id,
                requested_mode: if kind == BrandingPresentationReadKind::Document {
                    query.mode.as_deref()
                } else {
                    None
                },
            })
            .await
    }
    .await;
    respond(result)
}

async fn command(
    State(state): State<Arc<BrandingHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(malformed()),
    };
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, true)?;
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?
            .ok_or_else(key_required)?;
        if key.len() < 8 || key.len() > KEY_LIMIT || key.chars().any(char::is_control) {
            return Err(key_required());
        }
        if unique_header(&headers, header::CONTENT_TYPE)?
            .is_none_or(|value| !value.starts_with("application/json"))
        {
            return Err(malformed());
        }
        let value: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
        if !value.is_object() {
            return Err(malformed());
        }
        target
            .backend
            .command(BrandingHttpCommandRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
                key,
                body: value,
            })
            .await
    }
    .await;
    respond(result)
}

pub fn branding_http_router(targets: Vec<BrandingHttpTarget>) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || map
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/branding",
            get(read).post(command),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/branding/presentation",
            get(presentation),
        )
        .with_state(Arc::new(BrandingHttpState { targets: map })))
}
