//! Owner-local HTTP ingress for the native Projects detail and WBS read.
//!
//! Transport facts only: registered Host/Holding target, project identity,
//! bounded private JSON and business-error mapping. Normal Account session
//! resolution, current Company scope, purchase admission and the serializable
//! owner read remain composition responsibilities supplied through the backend
//! trait.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type ProjectWbsHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ProjectWbsHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub project_id: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct ProjectWbsHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl ProjectWbsHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, purchase and owner-unit adapter.
/// The router never mints a session, actor, Company project membership or
/// purchase.
pub trait ProjectWbsHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: ProjectWbsHttpRequest<'a>,
    ) -> ProjectWbsHttpFuture<'a, ProjectWbsHttpResponse>;
}

pub struct ProjectWbsHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn ProjectWbsHttpBackend>,
}

struct ProjectWbsHttpState {
    targets: BTreeMap<(String, String), ProjectWbsHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECTS_REQUEST_INVALID",
        "Use one unambiguous Projects work-structure request.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
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

pub fn projects_router(targets: Vec<ProjectWbsHttpTarget>) -> Result<Router, BusinessError> {
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
                "PROJECTS_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/wbs",
            get(read_wbs),
        )
        .with_state(Arc::new(ProjectWbsHttpState {
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
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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
    state: &'a ProjectWbsHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a ProjectWbsHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn read_wbs(
    State(state): State<Arc<ProjectWbsHttpState>>,
    Path((slug, project_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        let project_id = RecordId::new(project_id).map_err(|_| request_invalid())?;
        target
            .backend
            .read(ProjectWbsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                project_id: project_id.as_str(),
                headers: request.headers(),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
