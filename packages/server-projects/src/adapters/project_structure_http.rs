//! Owner-local HTTP ingress for native Projects work-structure and handoff
//! commands.
//!
//! The router validates the exact Host/Holding target, Origin,
//! Idempotency-Key, media type, query and body bounds, parses camel-case
//! command input, then forwards only transport facts and normalized owner
//! input. Account/session, Company scope, purchase admission and the
//! serializable owner unit remain composition responsibilities supplied through
//! the backend trait.
use crate::domain::project_structure::ProjectStructureCommand;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type ProjectStructureHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ProjectStructureHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub project_id: &'a str,
    pub key: &'a str,
    pub headers: &'a HeaderMap,
    pub input: &'a ProjectStructureCommand,
}

#[derive(Clone, Debug)]
pub struct ProjectStructureHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl ProjectStructureHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait ProjectStructureHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: ProjectStructureHttpRequest<'a>,
    ) -> ProjectStructureHttpFuture<'a, ProjectStructureHttpResponse>;
}

pub struct ProjectStructureHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn ProjectStructureHttpBackend>,
}

struct ProjectStructureHttpState {
    targets: BTreeMap<(String, String), ProjectStructureHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECTS_REQUEST_INVALID",
        "Use one unambiguous Projects command request.",
    )
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
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

pub fn project_structure_router(
    targets: Vec<ProjectStructureHttpTarget>,
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
                "PROJECTS_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/structure",
            post(structure),
        )
        .with_state(Arc::new(ProjectStructureHttpState {
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
    state: &'a ProjectStructureHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a ProjectStructureHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn command_facts<'a>(
    target: &'a ProjectStructureHttpTarget,
    headers: &'a HeaderMap,
    key: Option<&'a str>,
) -> Result<&'a str, BusinessError> {
    let key = key.ok_or_else(command_key_required)?;
    let origin = unique_header(headers, "origin")?.ok_or_else(origin_forbidden)?;
    if origin != target.origin {
        return Err(origin_forbidden());
    }
    let content_type = unique_header(headers, "content-type")?
        .ok_or_else(request_invalid)?
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if content_type != "application/json" {
        return Err(request_invalid());
    }
    Ok(key)
}

async fn body<T: for<'de> serde::Deserialize<'de>>(request: Request) -> Result<T, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| request_invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| request_invalid())
}

async fn structure(
    State(state): State<Arc<ProjectStructureHttpState>>,
    Path((slug, project_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        let key = unique_header(request.headers(), "idempotency-key")?;
        let key = command_facts(target, request.headers(), key)?.to_owned();
        let headers = request.headers().clone();
        let project_id = RecordId::new(project_id).map_err(|_| request_invalid())?;
        let input: ProjectStructureCommand = body(request).await?;
        let input = input.normalize();
        input.validate()?;
        target
            .backend
            .execute(ProjectStructureHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                project_id: project_id.as_str(),
                key: &key,
                headers: &headers,
                input: &input,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
