//! Owner-local HTTP ingress for the native Planning commands.
//!
//! Transport facts only: a registered Host/Holding target, one bounded planning
//! path, one strict JSON command, an Idempotency-Key and business-error
//! mapping. Normal Account session resolution, selected Company/branch scope,
//! purchase admission and the serializable owner unit remain composition
//! responsibilities behind [`PlanningCommandHttpBackend`]. The workspace read
//! keeps its own frozen router in [`super::planning_http`].
use crate::domain::scenario::request_invalid;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;

pub type PlanningCommandHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum PlanningCommandHttpCommand {
    Calculate {
        body: Value,
        idempotency_key: String,
    },
    Cancel {
        scenario_id: String,
        body: Value,
        idempotency_key: String,
    },
    Review {
        recommendation_id: String,
        body: Value,
        idempotency_key: String,
    },
    CreateResource {
        body: Value,
        idempotency_key: String,
    },
    UpdateResource {
        resource_id: String,
        body: Value,
        idempotency_key: String,
    },
    SaveCalendar {
        resource_id: String,
        body: Value,
        idempotency_key: String,
    },
}

pub struct PlanningCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: PlanningCommandHttpCommand,
}

#[derive(Clone, Debug)]
pub struct PlanningCommandHttpResponse {
    pub data: Value,
    pub context: Value,
}

impl PlanningCommandHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or purchase.
pub trait PlanningCommandHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PlanningCommandHttpRequest<'a>,
    ) -> PlanningCommandHttpFuture<'a, PlanningCommandHttpResponse>;
}

pub struct PlanningCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PlanningCommandHttpBackend>,
}

struct PlanningCommandHttpState {
    targets: BTreeMap<(String, String), PlanningCommandHttpTarget>,
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_REQUEST_INVALID",
        "Use one unambiguous Planning command.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
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

fn valid_id(value: &str) -> bool {
    RecordId::new(value.to_owned()).is_ok()
}

async fn strict_body(request: Request) -> Result<Value, BusinessError> {
    let content_type =
        unique_header(request.headers(), header::CONTENT_TYPE.as_str())?.ok_or_else(malformed)?;
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Err(malformed());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    if bytes.is_empty() {
        return Err(malformed());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if !value.is_object() {
        return Err(malformed());
    }
    Ok(value)
}

fn strict_key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let value = unique_header(headers, HeaderName::from_static("idempotency-key").as_str())?
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required for this Planning command.",
            )
        })?;
    CommandKey::new(value.to_owned())?;
    Ok(value.to_owned())
}

fn ensure_no_query(request: &Request) -> Result<(), BusinessError> {
    if request.uri().query().is_some() {
        return Err(request_invalid());
    }
    Ok(())
}

pub fn planning_command_router(
    targets: Vec<PlanningCommandHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/planning/scenarios",
            post(calculate_scenario),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/planning/scenarios/{scenario_id}",
            post(cancel_scenario),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/planning/recommendations/{recommendation_id}",
            post(review_recommendation),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/planning/resources",
            post(create_resource),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/planning/resources/{resource_id}",
            post(update_resource),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/planning/resources/{resource_id}/calendar",
            post(save_calendar),
        )
        .with_state(Arc::new(PlanningCommandHttpState {
            targets: configured,
        })))
}

fn target<'a>(
    state: &'a PlanningCommandHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PlanningCommandHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST.as_str())?
        .filter(|value| !value.is_empty())
        .ok_or_else(not_found)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(not_found)
}

async fn execute(
    state: Arc<PlanningCommandHttpState>,
    slug: String,
    headers: HeaderMap,
    command: impl FnOnce(String) -> PlanningCommandHttpCommand,
) -> Response {
    let result = async move {
        let configured = target(&state, &headers, &slug)?;
        let key = strict_key(&headers)?;
        configured
            .backend
            .execute(PlanningCommandHttpRequest {
                host: &configured.host,
                holding_slug: &configured.holding_slug,
                headers: &headers,
                command: command(key),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) if reply.data.is_object() && reply.context.is_object() => bounded(reply.value()),
        Ok(_) => error_response(BusinessError::new(
            FailureKind::Unavailable,
            "PLANNING_SOURCE_INVALID",
            "The current planning source is invalid.",
        )),
        Err(error) => error_response(error),
    }
}

async fn calculate_scenario(
    State(state): State<Arc<PlanningCommandHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    if ensure_no_query(&request).is_err() {
        return error_response(request_invalid());
    }
    let body = match strict_body(request).await {
        Ok(body) => body,
        Err(error) => return error_response(error),
    };
    execute(state, slug, headers, move |idempotency_key| {
        PlanningCommandHttpCommand::Calculate {
            body,
            idempotency_key,
        }
    })
    .await
}

async fn cancel_scenario(
    State(state): State<Arc<PlanningCommandHttpState>>,
    Path((slug, scenario_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    if ensure_no_query(&request).is_err() {
        return error_response(request_invalid());
    }
    if !valid_id(&scenario_id) {
        return error_response(not_found());
    }
    let body = match strict_body(request).await {
        Ok(body) => body,
        Err(error) => return error_response(error),
    };
    execute(state, slug, headers, move |idempotency_key| {
        PlanningCommandHttpCommand::Cancel {
            scenario_id,
            body,
            idempotency_key,
        }
    })
    .await
}

async fn review_recommendation(
    State(state): State<Arc<PlanningCommandHttpState>>,
    Path((slug, recommendation_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    if ensure_no_query(&request).is_err() {
        return error_response(request_invalid());
    }
    if !valid_id(&recommendation_id) {
        return error_response(not_found());
    }
    let body = match strict_body(request).await {
        Ok(body) => body,
        Err(error) => return error_response(error),
    };
    execute(state, slug, headers, move |idempotency_key| {
        PlanningCommandHttpCommand::Review {
            recommendation_id,
            body,
            idempotency_key,
        }
    })
    .await
}

async fn create_resource(
    State(state): State<Arc<PlanningCommandHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    if ensure_no_query(&request).is_err() {
        return error_response(request_invalid());
    }
    let body = match strict_body(request).await {
        Ok(body) => body,
        Err(error) => return error_response(error),
    };
    execute(state, slug, headers, move |idempotency_key| {
        PlanningCommandHttpCommand::CreateResource {
            body,
            idempotency_key,
        }
    })
    .await
}

async fn update_resource(
    State(state): State<Arc<PlanningCommandHttpState>>,
    Path((slug, resource_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    if ensure_no_query(&request).is_err() {
        return error_response(request_invalid());
    }
    if !valid_id(&resource_id) {
        return error_response(not_found());
    }
    let body = match strict_body(request).await {
        Ok(body) => body,
        Err(error) => return error_response(error),
    };
    execute(state, slug, headers, move |idempotency_key| {
        PlanningCommandHttpCommand::UpdateResource {
            resource_id,
            body,
            idempotency_key,
        }
    })
    .await
}

async fn save_calendar(
    State(state): State<Arc<PlanningCommandHttpState>>,
    Path((slug, resource_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    if ensure_no_query(&request).is_err() {
        return error_response(request_invalid());
    }
    if !valid_id(&resource_id) {
        return error_response(not_found());
    }
    let body = match strict_body(request).await {
        Ok(body) => body,
        Err(error) => return error_response(error),
    };
    execute(state, slug, headers, move |idempotency_key| {
        PlanningCommandHttpCommand::SaveCalendar {
            resource_id,
            body,
            idempotency_key,
        }
    })
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

fn bounded(value: Value) -> Response {
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
