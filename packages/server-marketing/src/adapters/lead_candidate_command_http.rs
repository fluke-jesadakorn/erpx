//! Native Marketing lead engagement command HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, the bounded typed
//! engagement body and business-error mapping. Current Account session
//! resolution, selected Company/branch/department scope, current IAM authority,
//! same-unit purchase admission and the serializable owner command remain
//! composition responsibilities behind [`MarketingLeadCommandHttpBackend`].
use crate::domain::lead_candidate::MarketingLeadActionInput;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;

pub type MarketingLeadCommandHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, BusinessError>> + Send + 'a>>;

pub struct MarketingLeadCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub candidate_id: &'a str,
    pub key: &'a CommandKey,
    pub body: &'a serde_json::Value,
}

/// Composition-owned Account session, scope, authority, purchase and owner
/// transaction adapter. The router never mints a session, actor, Company,
/// permission or entitlement.
pub trait MarketingLeadCommandHttpBackend: Send + Sync {
    fn record_engagement<'a>(
        &'a self,
        request: MarketingLeadCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a>;
}

pub struct MarketingLeadCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn MarketingLeadCommandHttpBackend>,
}

struct MarketingLeadCommandHttpState {
    targets: BTreeMap<(String, String), MarketingLeadCommandHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_COMMAND_REQUEST_INVALID",
        "Use one bounded Marketing engagement command.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_COMMAND_CONFIGURATION_INVALID",
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
                (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
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
    state: &'a MarketingLeadCommandHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a MarketingLeadCommandHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

struct ParsedCommand {
    headers: HeaderMap,
    key: CommandKey,
    body: serde_json::Value,
}

async fn parse_command(
    request: Request,
    target: &MarketingLeadCommandHttpTarget,
) -> Result<ParsedCommand, BusinessError> {
    if request.uri().query().is_some() {
        return Err(malformed());
    }
    if unique_header(request.headers(), header::ORIGIN)?.is_some_and(|value| value != target.origin)
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
    .ok_or_else(|| BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required."))
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
            BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller Marketing command.")
        })?;
    let body = serde_json::from_slice::<serde_json::Value>(&body).map_err(|_| malformed())?;
    Ok(ParsedCommand { headers, key, body })
}

fn parse_engagement(body: &serde_json::Value) -> Result<MarketingLeadActionInput, BusinessError> {
    let input: MarketingLeadActionInput =
        serde_json::from_value(body.clone()).map_err(|_| malformed())?;
    if input.action != "record-engagement" {
        return Err(BusinessError::invalid(
            "MARKETING_COMMAND_NOT_NATIVE",
            "Only the native Marketing engagement command is available.",
        ));
    }
    Ok(input)
}

/// Native Marketing engagement command ingress. Other candidate, asset,
/// campaign, cost and CRM handoff commands remain on the retained owner path.
pub fn marketing_lead_command_http_router(
    targets: Vec<MarketingLeadCommandHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/marketing/leads/{candidate_id}/changes",
            post(record_engagement),
        )
        .with_state(Arc::new(MarketingLeadCommandHttpState {
            targets: configured,
        })))
}

async fn record_engagement(
    State(state): State<Arc<MarketingLeadCommandHttpState>>,
    Path((slug, candidate_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let parsed = parse_command(request, target).await?;
        let candidate_id = RecordId::new(candidate_id)?;
        parse_engagement(&parsed.body)?;
        target
            .backend
            .record_engagement(MarketingLeadCommandHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &parsed.headers,
                candidate_id: candidate_id.as_str(),
                key: &parsed.key,
                body: &parsed.body,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply),
        Err(error) => error_response(error),
    }
}
