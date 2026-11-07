//! Owner-local HTTP ingress for the native Planning workspace read.
//!
//! Transport facts only: a registered Host/Holding target, exact origin
//! validation, no query ambiguity and bounded JSON/error mapping. Normal
//! Account session resolution, the selected Company/branch scope, package
//! admission and the serializable owner read remain composition
//! responsibilities behind [`PlanningHttpBackend`].
use crate::domain::planning::source_invalid;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type PlanningHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PlanningHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct PlanningHttpResponse {
    /// Owner workspace data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl PlanningHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or cross-owner label.
pub trait PlanningHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: PlanningHttpRequest<'a>,
    ) -> PlanningHttpFuture<'a, PlanningHttpResponse>;
}

pub struct PlanningHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PlanningHttpBackend>,
}

pub struct PlanningHttpState {
    targets: BTreeMap<(String, String), PlanningHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_REQUEST_INVALID",
        "Use one unambiguous Planning workspace request.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
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

fn valid_data(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 5
            && object
                .get("scenarios")
                .is_some_and(serde_json::Value::is_array)
            && object
                .get("recommendations")
                .is_some_and(serde_json::Value::is_array)
            && object
                .get("resources")
                .is_some_and(serde_json::Value::is_array)
            && object
                .get("calendar")
                .is_some_and(serde_json::Value::is_array)
            && object
                .get("allocations")
                .is_some_and(serde_json::Value::is_array)
    })
}

impl PlanningHttpState {
    pub fn new(targets: Vec<PlanningHttpTarget>) -> Result<Self, BusinessError> {
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
        Ok(Self {
            targets: configured,
        })
    }

    /// Exact GET handler used by both the Router and owner tests.
    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Response {
        match self.read_inner(headers, holding_slug, query).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Result<serde_json::Value, BusinessError> {
        if query.is_some() {
            return Err(request_invalid());
        }
        let host = unique_header(headers, header::HOST.as_str())?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let response = target
            .backend
            .read(PlanningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers,
            })
            .await?;
        if !valid_data(&response.data) || !response.context.is_object() {
            return Err(source_invalid());
        }
        Ok(response.value())
    }
}

pub fn planning_router(targets: Vec<PlanningHttpTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/planning/workspace",
            get(read_workspace),
        )
        .with_state(Arc::new(PlanningHttpState::new(targets)?)))
}

async fn read_workspace(
    State(state): State<Arc<PlanningHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    state
        .read(request.headers(), &slug, request.uri().query())
        .await
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
