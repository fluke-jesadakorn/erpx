//! Native Benefits executive evidence HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding target, one bounded JSON
//! body, an evidence action (`search` or `open`) and business-error mapping.
//! Normal Account session resolution, current authority, package admission and
//! the serializable owner unit remain composition responsibilities behind
//! [`BenefitsExecutiveEvidenceHttpBackend`].
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenefitsExecutiveEvidenceAction {
    Search,
    Open,
}

impl BenefitsExecutiveEvidenceAction {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "search" => Ok(Self::Search),
            "open" => Ok(Self::Open),
            _ => Err(malformed()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Open => "open",
        }
    }
}

pub type BenefitsExecutiveEvidenceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BenefitsExecutiveEvidenceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub action: BenefitsExecutiveEvidenceAction,
    pub body: &'a serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct BenefitsExecutiveEvidenceHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl BenefitsExecutiveEvidenceHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account/IAM/package/owner-unit adapter. The router never
/// mints a session, actor, Company, permission or package entitlement.
pub trait BenefitsExecutiveEvidenceHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: BenefitsExecutiveEvidenceHttpRequest<'a>,
    ) -> BenefitsExecutiveEvidenceHttpFuture<'a, BenefitsExecutiveEvidenceHttpResponse>;
}

pub struct BenefitsExecutiveEvidenceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn BenefitsExecutiveEvidenceHttpBackend>,
}

struct BenefitsExecutiveEvidenceHttpState {
    targets: BTreeMap<(String, String), BenefitsExecutiveEvidenceHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "BENEFITS_EXECUTIVE_EVIDENCE_REQUEST_INVALID",
        "Use one bounded Benefits executive evidence request.",
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
        "BENEFITS_EXECUTIVE_EVIDENCE_CONFIGURATION_INVALID",
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
    state: &'a BenefitsExecutiveEvidenceHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a BenefitsExecutiveEvidenceHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn benefits_executive_evidence_http_router(
    targets: Vec<BenefitsExecutiveEvidenceHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/benefits/executive-evidence/{action}",
            post(run),
        )
        .with_state(Arc::new(BenefitsExecutiveEvidenceHttpState {
            targets: configured,
        })))
}

async fn run(
    State(state): State<Arc<BenefitsExecutiveEvidenceHttpState>>,
    Path((slug, action)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let action = BenefitsExecutiveEvidenceAction::parse(&action)?;
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        // Origin is optional but, when present, must match the registered target.
        if let Some(origin) =
            unique_header(request.headers(), HeaderName::from_static("origin"))?
        {
            if origin != target.origin {
                return Err(forbidden());
            }
        }
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let body: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
        if !body.is_object() {
            return Err(malformed());
        }
        target
            .backend
            .execute(BenefitsExecutiveEvidenceHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                action,
                body: &body,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}