//! Native Marketing lead/asset HTTP read ingress.
//!
//! Transport facts only: registered Host/Holding targets, no query or body and
//! bounded business-error mapping. The backend supplies the normal Account
//! session, selected workspace, current permission and serializable owner unit.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

pub type MarketingLeadHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct MarketingLeadHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub candidate_id: Option<&'a str>,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct MarketingLeadHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl MarketingLeadHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait MarketingLeadHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: MarketingLeadHttpReadRequest<'a>,
    ) -> MarketingLeadHttpFuture<'a, MarketingLeadHttpResponse>;
}

pub struct MarketingLeadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn MarketingLeadHttpBackend>,
}

struct MarketingLeadHttpState {
    targets: BTreeMap<(String, String), MarketingLeadHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_LEAD_REQUEST_INVALID",
        "Use one bounded Marketing lead request.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_LEAD_CONFIGURATION_INVALID",
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
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
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
    state: &'a MarketingLeadHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a MarketingLeadHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn marketing_lead_http_router(
    targets: Vec<MarketingLeadHttpTarget>,
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
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/marketing/leads/{candidate_id}",
            get(read_detail),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/marketing/assets",
            get(read_assets),
        )
        .with_state(Arc::new(MarketingLeadHttpState {
            targets: configured,
        })))
}

async fn read_detail(
    State(state): State<Arc<MarketingLeadHttpState>>,
    Path((slug, candidate_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some() || !valid_id(&candidate_id) {
            return Err(malformed());
        }
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        target
            .backend
            .read(MarketingLeadHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                candidate_id: Some(&candidate_id),
                headers: &headers,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_assets(
    State(state): State<Arc<MarketingLeadHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        target
            .backend
            .read(MarketingLeadHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                candidate_id: None,
                headers: &headers,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
