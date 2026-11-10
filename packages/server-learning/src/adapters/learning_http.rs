//! Native Learning HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded Learning
//! path, one strict JSON command and business-error mapping. Normal Account
//! session resolution, selected Company/branch/department scope, purchase
//! admission and the serializable owner unit remain composition
//! responsibilities behind [`LearningHttpBackend`].
use crate::domain::eligibility_facts::{BenefitConsumptionCommand, EligibilityQuery};
use crate::domain::learning::{
    AssignmentChange, AssignmentRequest, CertificateRevocation, CourseChange, CourseRequest,
    request_invalid,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode,
    body::to_bytes,
    header,
    routing::{get, post},
    serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 32 * 1024;

pub type LearningHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum LearningHttpCommand {
    Workspace {
        personal: bool,
    },
    Detail {
        assignment_id: String,
    },
    EligibilityFacts {
        query: EligibilityQuery,
    },
    CreateCourse {
        input: CourseRequest,
        idempotency_key: String,
    },
    ChangeCourse {
        course_id: String,
        input: CourseChange,
        idempotency_key: String,
    },
    CreateAssignment {
        input: AssignmentRequest,
        idempotency_key: String,
    },
    ChangeAssignment {
        assignment_id: String,
        input: AssignmentChange,
        idempotency_key: String,
    },
    RevokeCertificate {
        certificate_id: String,
        input: CertificateRevocation,
        idempotency_key: String,
    },
    ConsumeBenefit {
        input: BenefitConsumptionCommand,
        idempotency_key: String,
    },
}

pub struct LearningHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: LearningHttpCommand,
}

#[derive(Clone, Debug)]
pub struct LearningHttpResponse {
    /// Owner data. The backend adds the validated current context.
    pub data: Value,
    pub context: Value,
}

impl LearningHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or purchase.
pub trait LearningHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: LearningHttpRequest<'a>,
    ) -> LearningHttpFuture<'a, LearningHttpResponse>;
}

pub struct LearningHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn LearningHttpBackend>,
}

struct LearningHttpState {
    targets: BTreeMap<(String, String), LearningHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LEARNING_CONFIGURATION_INVALID",
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
fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(malformed());
            };
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
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
    state: &'a LearningHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a LearningHttpTarget, BusinessError> {
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
fn personal(request: &Request) -> Result<bool, BusinessError> {
    let Some(query) = request.uri().query().filter(|value| !value.is_empty()) else {
        return Ok(false);
    };
    let mut personal = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').ok_or_else(malformed)?;
        if key != "personal" || personal.is_some() {
            return Err(malformed());
        }
        personal = Some(match value {
            "true" => true,
            "false" => false,
            _ => return Err(malformed()),
        });
    }
    Ok(personal.unwrap_or(false))
}
fn eligibility_query(
    request: &Request,
    person_id: &str,
) -> Result<EligibilityQuery, BusinessError> {
    let Some(query) = request.uri().query().filter(|value| !value.is_empty()) else {
        return Err(malformed());
    };
    if query.len() > 8192 {
        return Err(malformed());
    }
    let mut code = None;
    let mut effective_date = None;
    for pair in query.split('&') {
        if pair.is_empty() {
            return Err(malformed());
        }
        let (key, value) = pair.split_once('=').ok_or_else(malformed)?;
        if key == "code" && code.is_none() {
            code = Some(percent_decode(value)?);
        } else if key == "effectiveDate" && effective_date.is_none() {
            effective_date = Some(percent_decode(value)?);
        } else {
            return Err(malformed());
        }
    }
    EligibilityQuery {
        person_id: person_id.to_owned(),
        eligibility_code: code.ok_or_else(malformed)?,
        effective_date: effective_date.ok_or_else(malformed)?,
    }
    .validate()
    .map_err(|_| malformed())
}
async fn ensure_empty_body(request: Request) -> Result<(), BusinessError> {
    let bytes = to_bytes(request.into_body(), 0)
        .await
        .map_err(|_| malformed())?;
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(malformed())
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
                "Idempotency-Key is required for this Learning command.",
            )
        })?;
    CommandKey::new(value.to_owned())?;
    Ok(value.to_owned())
}

pub fn learning_http_router(targets: Vec<LearningHttpTarget>) -> Result<Router, BusinessError> {
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
            "/api/v1/holdings/{holding_slug}/learning/workspace",
            get(read_workspace),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/learning/assignments/{assignment_id}",
            get(read_detail).post(change_assignment),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/learning/eligibility/{person_id}",
            get(read_eligibility),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/learning/courses",
            post(create_course),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/learning/courses/{course_id}",
            post(change_course),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/learning/assignments",
            post(create_assignment),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/learning/certificates/{certificate_id}/revoke",
            post(revoke_certificate),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/learning/benefits/consumption",
            post(consume_benefit),
        )
        .with_state(Arc::new(LearningHttpState {
            targets: configured,
        })))
}

async fn read_workspace(
    State(state): State<Arc<LearningHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let personal = personal(&request)?;
        let headers = request.headers().clone();
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::Workspace { personal },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_detail(
    State(state): State<Arc<LearningHttpState>>,
    Path((slug, assignment_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&assignment_id) {
            return Err(missing());
        }
        let headers = request.headers().clone();
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::Detail { assignment_id },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_eligibility(
    State(state): State<Arc<LearningHttpState>>,
    Path((slug, person_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if !valid_id(&person_id) {
            return Err(missing());
        }
        let query = eligibility_query(&request, &person_id)?;
        let headers = request.headers().clone();
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::EligibilityFacts { query },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn create_course(
    State(state): State<Arc<LearningHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: CourseRequest = serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::CreateCourse {
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

async fn change_course(
    State(state): State<Arc<LearningHttpState>>,
    Path((slug, course_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&course_id) {
            return Err(missing());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: CourseChange = serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::ChangeCourse {
                    course_id,
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

async fn create_assignment(
    State(state): State<Arc<LearningHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: AssignmentRequest = serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::CreateAssignment {
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

async fn change_assignment(
    State(state): State<Arc<LearningHttpState>>,
    Path((slug, assignment_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&assignment_id) {
            return Err(missing());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: AssignmentChange = serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::ChangeAssignment {
                    assignment_id,
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

async fn revoke_certificate(
    State(state): State<Arc<LearningHttpState>>,
    Path((slug, certificate_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&certificate_id) {
            return Err(missing());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: CertificateRevocation = serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::RevokeCertificate {
                    certificate_id,
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

async fn consume_benefit(
    State(state): State<Arc<LearningHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input: BenefitConsumptionCommand =
            serde_json::from_value(body).map_err(|_| malformed())?;
        target
            .backend
            .execute(LearningHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: LearningHttpCommand::ConsumeBenefit {
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
