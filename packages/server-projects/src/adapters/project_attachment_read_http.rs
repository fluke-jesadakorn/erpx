//! Owner-local HTTP ingress for native Projects attachment reads.
//!
//! The router validates exact registered Host/Holding targets, bounded ids and
//! query, empty GET bodies, then forwards only transport facts. Account/session
//! resolution, current Company scope, Projects read authority, the retained
//! transaction and Files bytes remain composition responsibilities behind
//! [`ProjectAttachmentReadHttpBackend`].
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode,
    body::{Body, to_bytes},
    header,
    routing::get,
    serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 0;

pub type ProjectAttachmentReadHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ProjectAttachmentReadSetsHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub project_id: &'a str,
}

pub struct ProjectAttachmentReadLinksHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub project_id: &'a str,
    pub set_id: &'a str,
}

pub struct ProjectAttachmentReadStatusHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub project_id: &'a str,
    pub link_id: &'a str,
}

pub struct ProjectAttachmentReadContentHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub project_id: &'a str,
    pub link_id: &'a str,
    pub variant: &'a str,
}

#[derive(Clone, Debug)]
pub struct ProjectAttachmentReadHttpResponse {
    pub data: Value,
    pub context: Value,
}

impl ProjectAttachmentReadHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

#[derive(Clone, Debug)]
pub struct ProjectAttachmentContentHttpResponse {
    pub media_type: String,
    pub filename: String,
    pub inline: bool,
    pub body: Vec<u8>,
}

/// Composition-owned Account session, current scope, Projects authority and
/// retained unit adapter. The router never mints an actor, Company or version.
pub trait ProjectAttachmentReadHttpBackend: Send + Sync {
    fn sets<'a>(
        &'a self,
        request: ProjectAttachmentReadSetsHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse>;
    fn links<'a>(
        &'a self,
        request: ProjectAttachmentReadLinksHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse>;
    fn status<'a>(
        &'a self,
        request: ProjectAttachmentReadStatusHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse>;
    fn content<'a>(
        &'a self,
        request: ProjectAttachmentReadContentHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentContentHttpResponse>;
}

pub struct ProjectAttachmentReadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn ProjectAttachmentReadHttpBackend>,
}

struct ProjectAttachmentReadHttpState {
    targets: BTreeMap<(String, String), ProjectAttachmentReadHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECTS_REQUEST_INVALID",
        "Use one unambiguous Project file read request.",
    )
}

fn set_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_SET_NOT_FOUND",
        "Attachment set not found.",
    )
}

fn link_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
}

fn variant_invalid() -> BusinessError {
    BusinessError::invalid("ATTACHMENT_VARIANT", "Choose original or preview.")
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

fn record_id(value: &str, missing: fn() -> BusinessError) -> Result<(), BusinessError> {
    RecordId::new(value).map(|_| ()).map_err(|_| missing())
}

pub fn project_attachment_read_router(
    targets: Vec<ProjectAttachmentReadHttpTarget>,
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
            return Err(BusinessError::invalid(
                "PROJECTS_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/attachment-sets",
            get(sets),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/attachment-sets/{set_id}/links",
            get(links),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/attachment-links/{link_id}/status",
            get(status),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/attachment-links/{link_id}/content",
            get(content),
        )
        .with_state(Arc::new(ProjectAttachmentReadHttpState {
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

fn bounded(value: Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    private_headers(&mut response);
    response
}

fn encode_filename(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn file_response(content: ProjectAttachmentContentHttpResponse) -> Response {
    let media_type = HeaderValue::from_str(&content.media_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let disposition = HeaderValue::from_str(&format!(
        "{}; filename*=UTF-8''{}",
        if content.inline {
            "inline"
        } else {
            "attachment"
        },
        encode_filename(&content.filename)
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment"));
    let mut response = Response::new(Body::from(content.body));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, media_type);
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    headers.insert(
        HeaderName::from_static("accept-ranges"),
        HeaderValue::from_static("none"),
    );
    if content.inline {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox; default-src 'none'"),
        );
    }
    private_headers(&mut response);
    response
}

fn target<'a>(
    state: &'a ProjectAttachmentReadHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a ProjectAttachmentReadHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn ensure_empty_body(request: Request) -> Result<(), BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| request_invalid())?;
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(request_invalid())
    }
}

fn reject_query(request: &Request) -> Result<(), BusinessError> {
    if request.uri().query().is_some_and(|value| !value.is_empty()) {
        return Err(request_invalid());
    }
    Ok(())
}

fn content_variant(query: Option<&str>) -> Result<&'static str, BusinessError> {
    let mut variant = None;
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
    {
        let (key, value) = pair.split_once('=').ok_or_else(request_invalid)?;
        if key != "variant" || variant.is_some() {
            return Err(request_invalid());
        }
        variant = Some(value);
    }
    match variant {
        None | Some("original") => Ok("original"),
        Some("preview") => Ok("preview"),
        Some(_) => Err(variant_invalid()),
    }
}

async fn sets(
    State(state): State<Arc<ProjectAttachmentReadHttpState>>,
    Path((slug, project_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        reject_query(&request)?;
        let target = target(&state, &headers, &slug)?;
        record_id(&project_id, missing)?;
        ensure_empty_body(request).await?;
        target
            .backend
            .sets(ProjectAttachmentReadSetsHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                project_id: &project_id,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn links(
    State(state): State<Arc<ProjectAttachmentReadHttpState>>,
    Path((slug, project_id, set_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        reject_query(&request)?;
        let target = target(&state, &headers, &slug)?;
        record_id(&project_id, missing)?;
        record_id(&set_id, set_missing)?;
        ensure_empty_body(request).await?;
        target
            .backend
            .links(ProjectAttachmentReadLinksHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                project_id: &project_id,
                set_id: &set_id,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn status(
    State(state): State<Arc<ProjectAttachmentReadHttpState>>,
    Path((slug, project_id, link_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        reject_query(&request)?;
        let target = target(&state, &headers, &slug)?;
        record_id(&project_id, missing)?;
        record_id(&link_id, link_missing)?;
        ensure_empty_body(request).await?;
        target
            .backend
            .status(ProjectAttachmentReadStatusHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                project_id: &project_id,
                link_id: &link_id,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn content(
    State(state): State<Arc<ProjectAttachmentReadHttpState>>,
    Path((slug, project_id, link_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let variant = content_variant(request.uri().query())?;
        let target = target(&state, &headers, &slug)?;
        record_id(&project_id, missing)?;
        record_id(&link_id, link_missing)?;
        ensure_empty_body(request).await?;
        target
            .backend
            .content(ProjectAttachmentReadContentHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                project_id: &project_id,
                link_id: &link_id,
                variant,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => file_response(reply),
        Err(error) => error_response(error),
    }
}
