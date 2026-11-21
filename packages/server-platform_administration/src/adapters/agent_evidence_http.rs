//! Owner-local Agent evidence HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding target, one bounded
//! workspace or record selection and business-error mapping. The composition
//! backend resolves the normal Account session, selected Company/branch scope,
//! current authority and the serializable owner unit before calling the
//! application. The router never mints a session, actor, Company or permission.
use crate::domain::agent_evidence::AgentEvidenceQuery;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, get, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type AgentEvidenceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEvidenceHttpQuery {
    Workspace,
    Record(AgentEvidenceQuery),
}

pub struct AgentEvidenceHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: AgentEvidenceHttpQuery,
}

#[derive(Clone, Debug)]
pub struct AgentEvidenceHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl AgentEvidenceHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, selected Company scope and owner-unit
/// adapter. The router never reads or writes an Agent table itself.
pub trait AgentEvidenceHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: AgentEvidenceHttpReadRequest<'a>,
    ) -> AgentEvidenceHttpFuture<'a, AgentEvidenceHttpResponse>;
}

pub struct AgentEvidenceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn AgentEvidenceHttpBackend>,
}

struct AgentEvidenceHttpState {
    targets: BTreeMap<(String, String), AgentEvidenceHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "AGENT_EVIDENCE_REQUEST_INVALID",
        "Use one bounded Agent evidence request.",
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
    state: &'a AgentEvidenceHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a AgentEvidenceHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn origin(headers: &HeaderMap, target: &AgentEvidenceHttpTarget) -> Result<(), BusinessError> {
    if unique_header(headers, header::ORIGIN)?.is_some_and(|value| value != target.origin) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    Ok(())
}
fn no_query(request: &Request) -> Result<(), BusinessError> {
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return Err(malformed());
    }
    Ok(())
}
async fn empty_body(request: Request) -> Result<(), BusinessError> {
    to_bytes(request.into_body(), 0)
        .await
        .map(|_| ())
        .map_err(|_| malformed())
}
fn dispatch<'a>(
    backend: &'a dyn AgentEvidenceHttpBackend,
    host: &'a str,
    holding_slug: &'a str,
    headers: &'a HeaderMap,
    query: AgentEvidenceHttpQuery,
) -> AgentEvidenceHttpFuture<'a, AgentEvidenceHttpResponse> {
    backend.read(AgentEvidenceHttpReadRequest {
        host,
        holding_slug,
        headers,
        query,
    })
}

pub fn agent_evidence_http_router(
    targets: Vec<AgentEvidenceHttpTarget>,
) -> Result<Router, BusinessError> {
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
                "AGENT_EVIDENCE_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/agents/evidence",
            get(read_workspace),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/agents/evidence/{kind}/{id}",
            get(read_record),
        )
        .with_state(Arc::new(AgentEvidenceHttpState {
            targets: configured,
        })))
}

async fn read_workspace(
    State(state): State<Arc<AgentEvidenceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        no_query(&request)?;
        origin(request.headers(), target)?;
        let headers = request.headers().clone();
        empty_body(request).await?;
        dispatch(
            target.backend.as_ref(),
            &target.host,
            &target.holding_slug,
            &headers,
            AgentEvidenceHttpQuery::Workspace,
        )
        .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_record(
    State(state): State<Arc<AgentEvidenceHttpState>>,
    Path((slug, kind, id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let query = AgentEvidenceQuery::parse(&kind, &id)?;
        no_query(&request)?;
        origin(request.headers(), target)?;
        let headers = request.headers().clone();
        empty_body(request).await?;
        dispatch(
            target.backend.as_ref(),
            &target.host,
            &target.holding_slug,
            &headers,
            AgentEvidenceHttpQuery::Record(query),
        )
        .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
