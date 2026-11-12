//! Owner-local HTTP ingress for the native Projects attachment intake.
//!
//! The router validates exact registered Host/Holding targets, Origin,
//! Idempotency-Key, media type, ids, query and body bounds, then forwards only
//! transport facts. Account/session resolution, Company scope, Projects
//! purchase admission, the retained serializable unit, the Files ledger and the
//! private store all remain composition responsibilities behind
//! [`ProjectAttachmentIntakeHttpBackend`].
use serde::Deserialize;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::get, routing::post, routing::put, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
};

const CREATE_LIMIT: usize = 8 * 1024;
const RESERVE_LIMIT: usize = 64 * 1024;
const UPLOAD_LIMIT: usize = 10 * 1024 * 1024;
const MAX_MEDIA_TYPE: usize = 100;
const PROJECT_ATTACHMENT_PURPOSES: [&str; 3] = ["project_document", "deliverable", "handoff"];

/// Exact retained create body for the closed `project` target.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAttachmentSetBody {
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    #[serde(default)]
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_target_version: i64,
}

/// Exact retained reserve body.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAttachmentFileBody {
    pub filename: String,
    pub media_type: String,
    pub bytes: i64,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub supersedes_link_id: Option<String>,
}

pub type ProjectAttachmentIntakeHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ProjectAttachmentSetCreateHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub input: &'a ProjectAttachmentSetBody,
}

pub struct ProjectAttachmentReserveHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub set_id: &'a str,
    pub input: &'a ProjectAttachmentFileBody,
}

pub struct ProjectAttachmentCompleteHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub upload_id: &'a str,
    pub content_type: &'a str,
    pub bytes: &'a [u8],
}

pub struct ProjectAttachmentUploadStatusHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub upload_id: &'a str,
}

#[derive(Clone, Debug)]
pub struct ProjectAttachmentIntakeHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl ProjectAttachmentIntakeHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, purchase and owner-unit adapter.
/// The router never mints a session, actor, Company, version or storage key.
pub trait ProjectAttachmentIntakeHttpBackend: Send + Sync {
    fn create<'a>(
        &'a self,
        request: ProjectAttachmentSetCreateHttpRequest<'a>,
    ) -> ProjectAttachmentIntakeHttpFuture<'a, ProjectAttachmentIntakeHttpResponse>;
    fn reserve<'a>(
        &'a self,
        request: ProjectAttachmentReserveHttpRequest<'a>,
    ) -> ProjectAttachmentIntakeHttpFuture<'a, ProjectAttachmentIntakeHttpResponse>;
    fn complete<'a>(
        &'a self,
        request: ProjectAttachmentCompleteHttpRequest<'a>,
    ) -> ProjectAttachmentIntakeHttpFuture<'a, ProjectAttachmentIntakeHttpResponse>;
    fn status<'a>(
        &'a self,
        request: ProjectAttachmentUploadStatusHttpRequest<'a>,
    ) -> ProjectAttachmentIntakeHttpFuture<'a, ProjectAttachmentIntakeHttpResponse>;
}

pub struct ProjectAttachmentIntakeHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn ProjectAttachmentIntakeHttpBackend>,
}

struct ProjectAttachmentIntakeHttpState {
    targets: BTreeMap<(String, String), ProjectAttachmentIntakeHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECTS_REQUEST_INVALID",
        "Use one unambiguous Project file intake request.",
    )
}

fn policy_unknown() -> BusinessError {
    BusinessError::invalid(
        "ATTACHMENT_POLICY_UNKNOWN",
        "This file purpose is not registered for the selected record and context.",
    )
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
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

pub fn project_attachment_intake_router(
    targets: Vec<ProjectAttachmentIntakeHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PROJECTS_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    let base = "/api/v1/holdings/{holding_slug}/project-attachments";
    Ok(Router::new()
        .route(
            &format!("{base}/attachment-sets"),
            post(create_set),
        )
        .route(
            &format!("{base}/attachment-sets/{{set_id}}/files"),
            post(reserve),
        )
        .route(&format!("{base}/uploads/{{upload_id}}/content"), put(complete))
        .route(&format!("{base}/uploads/{{upload_id}}/status"), get(status))
        .with_state(Arc::new(ProjectAttachmentIntakeHttpState {
            targets: configured,
        })))
}

fn status_code(kind: FailureKind) -> StatusCode {
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
        status_code(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn private_headers(response: &mut Response) {
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
}

fn bounded(status: StatusCode, value: serde_json::Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    private_headers(&mut response);
    response
}

fn target<'a>(
    state: &'a ProjectAttachmentIntakeHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a ProjectAttachmentIntakeHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn reject_query(request: &Request) -> Result<(), BusinessError> {
    if request.uri().query().is_some() {
        return Err(request_invalid());
    }
    Ok(())
}

fn command_facts<'a>(
    target: &ProjectAttachmentIntakeHttpTarget,
    headers: &'a HeaderMap,
    key: Option<&'a str>,
    content_type: Option<&'a str>,
    expected_content_type: &str,
) -> Result<&'a str, BusinessError> {
    let key = key.ok_or_else(|| {
        BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
    })?;
    CommandKey::new(key)?;
    let origin = unique_header(headers, "origin")?.ok_or_else(origin_forbidden)?;
    if origin != target.origin {
        return Err(origin_forbidden());
    }
    let actual = content_type
        .ok_or_else(request_invalid)?
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if actual != expected_content_type {
        return Err(request_invalid());
    }
    Ok(key)
}

async fn json_body<const LIMIT: usize, T: serde::de::DeserializeOwned>(
    request: Request,
) -> Result<T, BusinessError> {
    let bytes = to_bytes(request.into_body(), LIMIT)
        .await
        .map_err(|_| request_invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| request_invalid())
}

fn valid_purpose(value: &str) -> bool {
    PROJECT_ATTACHMENT_PURPOSES.contains(&value)
}

async fn create_set(
    State(state): State<Arc<ProjectAttachmentIntakeHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        reject_query(&request)?;
        let target = target(&state, &headers, &slug)?;
        let key = unique_header(&headers, "idempotency-key")?;
        let key = command_facts(
            target,
            &headers,
            key,
            unique_header(&headers, "content-type")?,
            "application/json",
        )?
        .to_owned();
        let input: ProjectAttachmentSetBody = json_body::<CREATE_LIMIT, _>(request).await?;
        if input.target_type != "project"
            || input.expected_target_version < 1
            || input.target_id.len() > 180
            || RecordId::new(input.target_id.as_str()).is_err()
            || input
                .component_ref
                .as_deref()
                .is_some_and(|value| value.trim().encode_utf16().count() > 180)
        {
            return Err(request_invalid());
        }
        if input.component_kind != "record" || !valid_purpose(&input.purpose_key) {
            return Err(policy_unknown());
        }
        target
            .backend
            .create(ProjectAttachmentSetCreateHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                key: &key,
                input: &input,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(StatusCode::CREATED, reply.value()),
        Err(error) => error_response(error),
    }
}

async fn reserve(
    State(state): State<Arc<ProjectAttachmentIntakeHttpState>>,
    Path((slug, set_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        reject_query(&request)?;
        let target = target(&state, &headers, &slug)?;
        if set_id.len() > 180 {
            return Err(request_invalid());
        }
        RecordId::new(set_id.as_str()).map_err(|_| request_invalid())?;
        let key = unique_header(&headers, "idempotency-key")?;
        let key = command_facts(
            target,
            &headers,
            key,
            unique_header(&headers, "content-type")?,
            "application/json",
        )?
        .to_owned();
        let input: ProjectAttachmentFileBody = json_body::<RESERVE_LIMIT, _>(request).await?;
        if input.filename.trim().is_empty()
            || input.filename.trim().encode_utf16().count() > 255
            || input.media_type.is_empty()
            || input.media_type.len() > MAX_MEDIA_TYPE
            || !(1..=10 * 1024 * 1024).contains(&input.bytes)
            || input
                .caption
                .as_deref()
                .is_some_and(|value| value.trim().encode_utf16().count() > 1000)
            || input
                .supersedes_link_id
                .as_deref()
                .is_some_and(|value| RecordId::new(value).is_err())
        {
            return Err(request_invalid());
        }
        target
            .backend
            .reserve(ProjectAttachmentReserveHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                key: &key,
                set_id: set_id.as_str(),
                input: &input,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(StatusCode::CREATED, reply.value()),
        Err(error) => error_response(error),
    }
}

async fn complete(
    State(state): State<Arc<ProjectAttachmentIntakeHttpState>>,
    Path((slug, upload_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        reject_query(&request)?;
        let target = target(&state, &headers, &slug)?;
        if upload_id.len() > 180 {
            return Err(request_invalid());
        }
        RecordId::new(upload_id.as_str()).map_err(|_| request_invalid())?;
        let origin = unique_header(&headers, "origin")?.ok_or_else(origin_forbidden)?;
        if origin != target.origin {
            return Err(origin_forbidden());
        }
        let content_type = unique_header(&headers, "content-type")?
            .ok_or_else(request_invalid)?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if content_type.is_empty() || content_type.len() > MAX_MEDIA_TYPE {
            return Err(request_invalid());
        }
        let bytes = to_bytes(request.into_body(), UPLOAD_LIMIT)
            .await
            .map_err(|_| BusinessError::invalid(
                "UPLOAD_SIZE",
                "The uploaded bytes must exactly match the reserved size.",
            ))?;
        if bytes.is_empty() {
            return Err(BusinessError::invalid(
                "UPLOAD_SIZE",
                "The uploaded bytes must exactly match the reserved size.",
            ));
        }
        target
            .backend
            .complete(ProjectAttachmentCompleteHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                upload_id: upload_id.as_str(),
                content_type: &content_type,
                bytes: &bytes,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(StatusCode::ACCEPTED, reply.value()),
        Err(error) => error_response(error),
    }
}

async fn status(
    State(state): State<Arc<ProjectAttachmentIntakeHttpState>>,
    Path((slug, upload_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        reject_query(&request)?;
        let target = target(&state, &headers, &slug)?;
        if upload_id.len() > 180 {
            return Err(request_invalid());
        }
        RecordId::new(upload_id.as_str()).map_err(|_| request_invalid())?;
        let body = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| request_invalid())?;
        if !body.is_empty() {
            return Err(request_invalid());
        }
        target
            .backend
            .status(ProjectAttachmentUploadStatusHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                upload_id: upload_id.as_str(),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(StatusCode::OK, reply.value()),
        Err(error) => error_response(error),
    }
}
