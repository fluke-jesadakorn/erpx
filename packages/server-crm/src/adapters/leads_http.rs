//! Native CRM lead / opportunity command HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, the bounded typed
//! command body and business-error mapping. Current Account session resolution,
//! selected Company/branch/department scope, current IAM authority, package
//! purchase admission, owner CAS effects and Workflow receipts/audit/outbox
//! remain composition responsibilities behind [`CrmLeadCommandHttpBackend`].
use crate::domain::leads_native::{
    LeadChangeInput, OpportunityChangeInput, not_native,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;

pub type CrmLeadCommandHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, BusinessError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrmLeadCommandKind {
    Lead,
    Opportunity,
}

impl CrmLeadCommandKind {
    fn reject(&self, action: &str) -> Result<(), BusinessError> {
        let native = match self {
            Self::Lead => crate::domain::leads_native::native_lead_action(action),
            Self::Opportunity => crate::domain::leads_native::native_opportunity_action(action),
        };
        if native {
            Ok(())
        } else {
            Err(not_native(action))
        }
    }
}

pub struct CrmLeadCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub lead_id: &'a str,
    pub key: &'a CommandKey,
    pub body: &'a serde_json::Value,
}

/// Composition-owned Account session, scope, authority, purchase and owner
/// transaction adapter. The router never mints a session, actor, Company,
/// permission or entitlement.
pub trait CrmLeadCommandHttpBackend: Send + Sync {
    fn change_lead<'a>(
        &'a self,
        request: CrmLeadCommandHttpRequest<'a>,
    ) -> CrmLeadCommandHttpFuture<'a>;
    fn change_opportunity<'a>(
        &'a self,
        request: CrmLeadCommandHttpRequest<'a>,
    ) -> CrmLeadCommandHttpFuture<'a>;
}

pub struct CrmLeadCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CrmLeadCommandHttpBackend>,
}

struct CrmLeadCommandHttpState {
    targets: BTreeMap<(String, String), CrmLeadCommandHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "CRM_COMMAND_REQUEST_INVALID",
        "Use one bounded CRM command.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_COMMAND_CONFIGURATION_INVALID",
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
    state: &'a CrmLeadCommandHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CrmLeadCommandHttpTarget, BusinessError> {
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
    target: &CrmLeadCommandHttpTarget,
) -> Result<ParsedCommand, BusinessError> {
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
            BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller CRM command.")
        })?;
    let body = serde_json::from_slice::<serde_json::Value>(&body).map_err(|_| malformed())?;
    Ok(ParsedCommand { headers, key, body })
}

fn parse_lead(body: &serde_json::Value) -> Result<LeadChangeInput, BusinessError> {
    let input: LeadChangeInput =
        serde_json::from_value(body.clone()).map_err(|_| malformed())?;
    CrmLeadCommandKind::Lead.reject(&input.action)?;
    Ok(input)
}

fn parse_opportunity(body: &serde_json::Value) -> Result<OpportunityChangeInput, BusinessError> {
    let input: OpportunityChangeInput =
        serde_json::from_value(body.clone()).map_err(|_| malformed())?;
    CrmLeadCommandKind::Opportunity.reject(&input.action)?;
    Ok(input)
}

pub fn crm_lead_command_http_router(
    targets: Vec<CrmLeadCommandHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/crm/leads/{lead_id}/changes",
            post(change_lead),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/crm/leads/{lead_id}/opportunity/changes",
            post(change_opportunity),
        )
        .with_state(Arc::new(CrmLeadCommandHttpState {
            targets: configured,
        })))
}

async fn change_lead(
    State(state): State<Arc<CrmLeadCommandHttpState>>,
    Path((slug, lead_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let parsed = parse_command(request, target).await?;
        let lead_id = RecordId::new(lead_id)?;
        parse_lead(&parsed.body)?;
        target
            .backend
            .change_lead(CrmLeadCommandHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &parsed.headers,
                lead_id: lead_id.as_str(),
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

async fn change_opportunity(
    State(state): State<Arc<CrmLeadCommandHttpState>>,
    Path((slug, lead_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let parsed = parse_command(request, target).await?;
        let lead_id = RecordId::new(lead_id)?;
        parse_opportunity(&parsed.body)?;
        target
            .backend
            .change_opportunity(CrmLeadCommandHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &parsed.headers,
                lead_id: lead_id.as_str(),
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
