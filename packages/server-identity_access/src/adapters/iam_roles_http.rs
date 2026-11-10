//! Native Identity Access role/review HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, bounded JSON commands,
//! strict Idempotency-Key and business-error mapping. Normal Account session
//! resolution, selected Company/branch/department scope, current IAM authority,
//! the permission catalogue and the serializable owner unit remain composition
//! responsibilities behind [`IamRolesReviewsHttpBackend`].
use crate::domain::iam_roles::{
    AccessReviewCampaignInput, AccessReviewCancelInput, AccessReviewDecisionInput,
    ReviewDecisionInput, RoleSaveInput, request_invalid,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type IamRolesReviewsHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum IamRolesReviewsHttpCommand {
    SaveRole {
        input: RoleSaveInput,
        idempotency_key: String,
    },
    DecideReview {
        review_id: String,
        input: ReviewDecisionInput,
        idempotency_key: String,
    },
    StartAccessReview {
        input: AccessReviewCampaignInput,
        idempotency_key: String,
    },
    DecideAccessReviewItem {
        item_id: String,
        input: AccessReviewDecisionInput,
        idempotency_key: String,
    },
    CancelAccessReview {
        campaign_id: String,
        input: AccessReviewCancelInput,
        idempotency_key: String,
    },
}

pub struct IamRolesReviewsHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: IamRolesReviewsHttpCommand,
}

#[derive(Clone, Debug)]
pub struct IamRolesReviewsHttpResponse {
    /// Owner data. The backend adds the validated current context.
    pub data: Value,
    pub context: Value,
}

impl IamRolesReviewsHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, catalogue and owner-unit adapter.
/// The router never mints a session, actor, Company, permission or role.
pub trait IamRolesReviewsHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: IamRolesReviewsHttpRequest<'a>,
    ) -> IamRolesReviewsHttpFuture<'a, IamRolesReviewsHttpResponse>;
}

pub struct IamRolesReviewsHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn IamRolesReviewsHttpBackend>,
}

struct IamRolesReviewsHttpState {
    targets: BTreeMap<(String, String), IamRolesReviewsHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "IAM_ROLES_REVIEWS_HTTP_CONFIGURATION_INVALID",
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
fn valid_id(value: &str) -> bool {
    RecordId::new(value.to_owned()).is_ok()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
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
fn bounded(body: Value) -> Response {
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
    state: &'a IamRolesReviewsHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a IamRolesReviewsHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn ensure_no_query(request: &Request) -> Result<(), BusinessError> {
    match request.uri().query() {
        None | Some("") => Ok(()),
        Some(_) => Err(malformed()),
    }
}
async fn strict_body(request: Request) -> Result<Value, BusinessError> {
    let content_type =
        unique_header(request.headers(), header::CONTENT_TYPE)?.ok_or_else(malformed)?;
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
    let value =
        unique_header(headers, HeaderName::from_static("idempotency-key"))?.ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required for this identity command.",
            )
        })?;
    CommandKey::new(value.to_owned())?;
    Ok(value.to_owned())
}

pub fn iam_roles_reviews_http_router(
    targets: Vec<IamRolesReviewsHttpTarget>,
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
        .route("/api/v1/holdings/{holding_slug}/iam/roles", post(save_role))
        .route(
            "/api/v1/holdings/{holding_slug}/iam/reviews/{review_id}/decide",
            post(decide_review),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/access-reviews",
            post(start_access_review),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/access-review-items/{item_id}/decide",
            post(decide_access_review_item),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/access-reviews/{campaign_id}/cancel",
            post(cancel_access_review),
        )
        .with_state(Arc::new(IamRolesReviewsHttpState {
            targets: configured,
        })))
}

async fn save_role(
    State(state): State<Arc<IamRolesReviewsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: RoleSaveInput = serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(IamRolesReviewsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: IamRolesReviewsHttpCommand::SaveRole {
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn decide_review(
    State(state): State<Arc<IamRolesReviewsHttpState>>,
    Path((slug, review_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&review_id) {
            return Err(missing());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: ReviewDecisionInput = serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(IamRolesReviewsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: IamRolesReviewsHttpCommand::DecideReview {
                    review_id,
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn start_access_review(
    State(state): State<Arc<IamRolesReviewsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: AccessReviewCampaignInput =
            serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(IamRolesReviewsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: IamRolesReviewsHttpCommand::StartAccessReview {
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn decide_access_review_item(
    State(state): State<Arc<IamRolesReviewsHttpState>>,
    Path((slug, item_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&item_id) {
            return Err(missing());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: AccessReviewDecisionInput =
            serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(IamRolesReviewsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: IamRolesReviewsHttpCommand::DecideAccessReviewItem {
                    item_id,
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn cancel_access_review(
    State(state): State<Arc<IamRolesReviewsHttpState>>,
    Path((slug, campaign_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&campaign_id) {
            return Err(missing());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: AccessReviewCancelInput =
            serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(IamRolesReviewsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: IamRolesReviewsHttpCommand::CancelAccessReview {
                    campaign_id,
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
