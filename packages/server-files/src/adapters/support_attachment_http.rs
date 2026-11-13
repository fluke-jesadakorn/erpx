//! Owner-local HTTP ingress for the native Support evidence attachment intake.
//!
//! The router validates exact registered Host/Holding targets, Origin,
//! Idempotency-Key, content type, ids and body bounds, then forwards only
//! transport facts. Account/session resolution, current Company scope, the
//! Support ticket authority, the purchased `support.evidence` capability, the
//! SERIALIZABLE unit, the private quarantine writer and the native scan remain
//! composition responsibilities behind [`SupportAttachmentHttpBackend`].
use crate::adapters::support_attachment_intake::{
    SupportAttachmentFileRequest, SupportAttachmentSetRequest,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, routing::put, serde_json,
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

pub type SupportAttachmentHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct SupportAttachmentCreateHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub input: &'a SupportAttachmentSetRequest,
}

pub struct SupportAttachmentReserveHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub set_id: &'a str,
    pub input: &'a SupportAttachmentFileRequest,
}

pub struct SupportAttachmentCompleteHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub upload_id: &'a str,
    pub content_type: &'a str,
    pub bytes: &'a [u8],
}

pub struct SupportAttachmentScanHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub upload_id: &'a str,
}

#[derive(Clone, Debug)]
pub struct SupportAttachmentHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl SupportAttachmentHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, purchase, owner-unit and scan
/// adapter. The router never mints a session, actor, Company, version or key.
pub trait SupportAttachmentHttpBackend: Send + Sync {
    fn create<'a>(
        &'a self,
        request: SupportAttachmentCreateHttpRequest<'a>,
    ) -> SupportAttachmentHttpFuture<'a, SupportAttachmentHttpResponse>;
    fn reserve<'a>(
        &'a self,
        request: SupportAttachmentReserveHttpRequest<'a>,
    ) -> SupportAttachmentHttpFuture<'a, SupportAttachmentHttpResponse>;
    fn complete<'a>(
        &'a self,
        request: SupportAttachmentCompleteHttpRequest<'a>,
    ) -> SupportAttachmentHttpFuture<'a, SupportAttachmentHttpResponse>;
    /// Run the native malware scan for the exact completed source and return
    /// its bounded state. A configured failure never falls back to TS.
    fn scan<'a>(
        &'a self,
        request: SupportAttachmentScanHttpRequest<'a>,
    ) -> SupportAttachmentHttpFuture<'a, SupportAttachmentHttpResponse>;
}

pub struct SupportAttachmentHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SupportAttachmentHttpBackend>,
}

struct SupportAttachmentHttpState {
    targets: BTreeMap<(String, String), SupportAttachmentHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn request_invalid() -> BusinessError {
    BusinessError::invalid("ATTACHMENT_REQUEST_INVALID", "Review the file request.")
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

pub fn support_attachment_router(
    targets: Vec<SupportAttachmentHttpTarget>,
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
                "FILES_SUPPORT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    let base = "/api/v1/holdings/{holding_slug}/support-attachments";
    Ok(Router::new()
        .route(&format!("{base}/attachment-sets"), post(create_set))
        .route(
            &format!("{base}/attachment-sets/{{set_id}}/files"),
            post(reserve),
        )
        .route(
            &format!("{base}/uploads/{{upload_id}}/content"),
            put(complete),
        )
        .route(&format!("{base}/uploads/{{upload_id}}/scan"), post(scan))
        .with_state(Arc::new(SupportAttachmentHttpState {
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
    state: &'a SupportAttachmentHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SupportAttachmentHttpTarget, BusinessError> {
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
    target: &SupportAttachmentHttpTarget,
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

fn key_only<'a>(
    target: &SupportAttachmentHttpTarget,
    headers: &'a HeaderMap,
) -> Result<(), BusinessError> {
    let key = unique_header(headers, "idempotency-key")?.ok_or_else(|| {
        BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
    })?;
    CommandKey::new(key)?;
    let origin = unique_header(headers, "origin")?.ok_or_else(origin_forbidden)?;
    if origin != target.origin {
        return Err(origin_forbidden());
    }
    Ok(())
}

async fn json_body<const LIMIT: usize, T: serde::de::DeserializeOwned>(
    request: Request,
) -> Result<T, BusinessError> {
    let bytes = to_bytes(request.into_body(), LIMIT)
        .await
        .map_err(|_| request_invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| request_invalid())
}

fn valid_media_type(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_MEDIA_TYPE && !value.contains('\0')
}

async fn create_set(
    State(state): State<Arc<SupportAttachmentHttpState>>,
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
        let input: SupportAttachmentSetRequest = json_body::<CREATE_LIMIT, _>(request).await?;
        if input.target_type != "support_ticket"
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
        if input.component_kind != "action" {
            return Err(BusinessError::invalid(
                "ATTACHMENT_POLICY_UNKNOWN",
                "This file purpose is not registered for the selected record and context.",
            ));
        }
        target
            .backend
            .create(SupportAttachmentCreateHttpRequest {
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
    State(state): State<Arc<SupportAttachmentHttpState>>,
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
        let input: SupportAttachmentFileRequest = json_body::<RESERVE_LIMIT, _>(request).await?;
        if input.filename.trim().is_empty()
            || input.filename.trim().encode_utf16().count() > 255
            || !valid_media_type(&input.media_type)
            || !(1..=UPLOAD_LIMIT as i64).contains(&input.bytes)
            || input
                .caption
                .as_deref()
                .is_some_and(|value| value.trim().encode_utf16().count() > 1000)
            || input.supersedes_link_id.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 180 || value.contains('/') || value.contains('\\')
            })
        {
            return Err(request_invalid());
        }
        target
            .backend
            .reserve(SupportAttachmentReserveHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                key: &key,
                set_id: &set_id,
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
    State(state): State<Arc<SupportAttachmentHttpState>>,
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
        let content_type = unique_header(&headers, "content-type")?
            .ok_or_else(request_invalid)?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if !valid_media_type(&content_type) {
            return Err(request_invalid());
        }
        let bytes = to_bytes(request.into_body(), UPLOAD_LIMIT)
            .await
            .map_err(|_| request_invalid())?;
        if bytes.is_empty() {
            return Err(request_invalid());
        }
        target
            .backend
            .complete(SupportAttachmentCompleteHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                upload_id: &upload_id,
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

async fn scan(
    State(state): State<Arc<SupportAttachmentHttpState>>,
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
        key_only(target, &headers)?;
        let bytes = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| request_invalid())?;
        if !bytes.is_empty() {
            return Err(request_invalid());
        }
        target
            .backend
            .scan(SupportAttachmentScanHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                upload_id: &upload_id,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(StatusCode::OK, reply.value()),
        Err(error) => error_response(error),
    }
}
