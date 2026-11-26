//! Owner-local HTTP ingress for native Files attachment reads and commands.
//!
//! Transport facts only: exact registered Host/Holding targets, one bounded
//! list query, one bounded status path, one exact-origin remove command, strict
//! JSON, and BusinessError mapping. Normal Account session resolution, current
//! Company/branch/department scope, the owning-record authority and the
//! serializable owner unit remain composition responsibilities behind
//! [`AttachmentReadHttpBackend`].
use crate::domain::attachment_command::valid_command_key;
use crate::domain::attachment_read::{
    AttachmentListFilter, MAX_IDENTIFIER_BYTES, MAX_PURPOSE_BYTES, MAX_TARGET_ID_BYTES,
    MAX_TARGET_TYPE_BYTES,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 0;
const COMMAND_BODY_LIMIT: usize = 8 * 1024;
const CONTENT_BODY_LIMIT: usize = 10 * 1024 * 1024;

pub type AttachmentReadHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct AttachmentReadHttpListRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub filter: &'a AttachmentListFilter,
    pub read_only: bool,
}
pub struct AttachmentReadHttpStatusRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub link_id: &'a str,
}
/// One bounded remove command. `reason` and `command_key` are normalized by
/// the owner service; the router only proves exact Host/Origin/JSON/key shape.
pub struct AttachmentCommandHttpWithdrawRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub link_id: &'a str,
    pub reason: &'a str,
    pub command_key: &'a str,
}

/// One bounded content read. The router owns Host/Origin, id and query shape,
/// the empty GET body and the wire headers; the composition backend owns the
/// current actor, record authority and the exact stored bytes.
pub struct AttachmentContentHttpByLinkRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub link_id: &'a str,
    pub variant: &'a str,
    pub page: Option<u32>,
}

pub struct AttachmentContentHttpByAttachmentRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub attachment_id: &'a str,
    pub variant: &'a str,
    pub page: Option<u32>,
}

/// Verified private bytes plus their exact disposition facts. The checksum is
/// computed over `body` by the owner and re-verified at the router boundary.
#[derive(Clone, Debug)]
pub struct AttachmentContentHttpReply {
    pub media_type: String,
    pub filename: String,
    pub inline: bool,
    pub body: Vec<u8>,
    pub checksum_sha256: String,
}

/// Map the owner use case's verified read onto the wire reply. The router still
/// rechecks checksum, media type and size before it writes any response header.
pub fn content_http_reply(
    read: crate::adapters::attachment_content_use_cases::AttachmentContentRead,
) -> AttachmentContentHttpReply {
    AttachmentContentHttpReply {
        media_type: read.media_type,
        filename: read.filename,
        inline: read.inline,
        body: read.body,
        checksum_sha256: read.checksum_sha256,
    }
}

/// Owner data plus the composition-validated current context. The router never
/// mints a session, actor, Company, permission, scope or record version.
#[derive(Clone, Debug)]
pub struct AttachmentReadHttpReply {
    pub data: Value,
    pub context: Value,
}
impl AttachmentReadHttpReply {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait AttachmentReadHttpBackend: Send + Sync {
    fn list<'a>(
        &'a self,
        request: AttachmentReadHttpListRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply>;
    fn status<'a>(
        &'a self,
        request: AttachmentReadHttpStatusRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply>;
    fn withdraw<'a>(
        &'a self,
        request: AttachmentCommandHttpWithdrawRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply>;
    /// Content delivery stays fail-closed until the owning composition backend
    /// mounts a byte store. The router still executes transport, validation and
    /// response shaping for every configured Holding.
    fn content_by_link<'a>(
        &'a self,
        _: AttachmentContentHttpByLinkRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentContentHttpReply> {
        Box::pin(async { Err(content_unavailable()) })
    }
    fn content_by_attachment<'a>(
        &'a self,
        _: AttachmentContentHttpByAttachmentRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentContentHttpReply> {
        Box::pin(async { Err(content_unavailable()) })
    }
}

pub struct AttachmentReadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn AttachmentReadHttpBackend>,
}

struct AttachmentReadHttpState {
    targets: BTreeMap<(String, String), AttachmentReadHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid("ATTACHMENT_REQUEST_INVALID", "Review the file request.")
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ATTACHMENT_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn content_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_CONTENT_UNAVAILABLE",
        "The native file reader is unavailable.",
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
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}
fn valid_link_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || matches!(byte, b'/' | b'\\' | b'?' | b'#'))
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
    state: &'a AttachmentReadHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a AttachmentReadHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn ensure_empty_body(request: Request) -> AttachmentReadHttpFuture<'static, ()> {
    Box::pin(async move {
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        if bytes.is_empty() {
            Ok(())
        } else {
            Err(malformed())
        }
    })
}
fn percent_decode(raw: &str) -> Result<String, BusinessError> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes
                    .get(index + 1)
                    .and_then(|value| (*value as char).to_digit(16))
                    .ok_or_else(malformed)?;
                let low = bytes
                    .get(index + 2)
                    .and_then(|value| (*value as char).to_digit(16))
                    .ok_or_else(malformed)?;
                out.push((high * 16 + low) as u8);
                index += 3;
            }
            value => {
                out.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}
fn query_values(query: Option<&str>) -> Result<BTreeMap<String, String>, BusinessError> {
    let mut values = BTreeMap::new();
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
    {
        let (key, value) = pair.split_once('=').ok_or_else(malformed)?;
        let key = percent_decode(key)?;
        let value = percent_decode(value)?;
        let allowed = matches!(
            key.as_str(),
            "targetType"
                | "targetId"
                | "purposeKey"
                | "componentKind"
                | "expectedTargetVersion"
                | "readOnly"
        );
        if !allowed || values.insert(key, value).is_some() {
            return Err(malformed());
        }
    }
    Ok(values)
}
fn list_filter(
    values: &BTreeMap<String, String>,
) -> Result<(AttachmentListFilter, bool), BusinessError> {
    let target_type = values
        .get("targetType")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BusinessError::invalid(
                "ATTACHMENT_TARGET_REQUIRED",
                "Choose a record before listing files.",
            )
        })?;
    let target_id = values
        .get("targetId")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BusinessError::invalid(
                "ATTACHMENT_TARGET_REQUIRED",
                "Choose a record before listing files.",
            )
        })?;
    let expected = values
        .get("expectedTargetVersion")
        .map(|value| {
            value
                .parse::<i64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(malformed)
        })
        .transpose()?;
    let read_only = match values.get("readOnly").map(String::as_str) {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return Err(malformed()),
    };
    if target_type.len() > MAX_TARGET_TYPE_BYTES
        || target_id.len() > MAX_TARGET_ID_BYTES
        || values
            .get("purposeKey")
            .is_some_and(|value| value.len() > MAX_PURPOSE_BYTES)
    {
        return Err(malformed());
    }
    Ok((
        AttachmentListFilter::new(
            target_type.clone(),
            target_id.clone(),
            values.get("purposeKey").cloned(),
            values.get("componentKind").cloned(),
            expected,
        )?,
        read_only,
    ))
}

/// The two content paths accept only `variant`, `page` and the front-office
/// placement hint. `page` always selects the sanitized preview, matching the
/// retained Next route, and any unknown or repeated key fails closed.
fn content_query(query: Option<&str>) -> Result<(&'static str, Option<u32>), BusinessError> {
    let mut variant = None;
    let mut page = None;
    let mut front = false;
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.is_empty())
    {
        let (key, raw) = pair.split_once('=').ok_or_else(malformed)?;
        let value = percent_decode(raw)?;
        match key {
            "variant" => {
                if variant.is_some() {
                    return Err(malformed());
                }
                variant = Some(match value.as_str() {
                    "original" => "original",
                    "preview" => "preview",
                    _ => {
                        return Err(BusinessError::invalid(
                            "ATTACHMENT_VARIANT",
                            "Choose original or preview.",
                        ));
                    }
                });
            }
            "page" => {
                if page.is_some() {
                    return Err(malformed());
                }
                let number = value.parse::<u32>().map_err(|_| malformed())?;
                if number == 0 {
                    return Err(malformed());
                }
                page = Some(number);
            }
            "frontLocation" => {
                if front
                    || value.is_empty()
                    || value.len() > 200
                    || value.bytes().any(|byte| byte.is_ascii_control())
                {
                    return Err(malformed());
                }
                front = true;
            }
            _ => return Err(malformed()),
        }
    }
    let variant = match variant {
        Some("preview") => "preview",
        _ if page.is_some() => "preview",
        _ => "original",
    };
    Ok((variant, page))
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

fn content_response(
    reply: AttachmentContentHttpReply,
    variant: &str,
) -> Result<Response, BusinessError> {
    if reply.body.is_empty()
        || reply.body.len() > CONTENT_BODY_LIMIT
        || reply.checksum_sha256.len() != 64
        || !reply
            .checksum_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || hex::encode(Sha256::digest(&reply.body)) != reply.checksum_sha256
    {
        return Err(content_unavailable());
    }
    const PREVIEW_TYPES: &[&str] = &["image/jpeg", "image/webp"];
    const ORIGINAL_TYPES: &[&str] = &[
        "image/jpeg",
        "image/png",
        "image/webp",
        "image/heic",
        "image/heif",
        "application/pdf",
    ];
    let allowed = if variant == "preview" {
        PREVIEW_TYPES
    } else {
        ORIGINAL_TYPES
    };
    if !allowed.contains(&reply.media_type.as_str()) || reply.filename.is_empty() {
        return Err(content_unavailable());
    }
    let Ok(media) = HeaderValue::from_str(&reply.media_type) else {
        return Err(content_unavailable());
    };
    let disposition = format!(
        "{}; filename*=UTF-8''{}",
        if reply.inline { "inline" } else { "attachment" },
        encode_filename(&reply.filename)
    );
    let Ok(disposition) = HeaderValue::from_str(&disposition) else {
        return Err(content_unavailable());
    };
    let Ok(checksum) = HeaderValue::from_str(&reply.checksum_sha256) else {
        return Err(content_unavailable());
    };
    let mut response = Response::new(erpx_server_runtime::public::http_support::body::Body::from(
        reply.body,
    ));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, media);
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    headers.insert(HeaderName::from_static("x-erpx-checksum-sha256"), checksum);
    // Byte ranges are deliberately closed on both the retained Next route and
    // the native Field/Project content routes. One bounded 200 body is served.
    headers.insert(
        HeaderName::from_static("accept-ranges"),
        HeaderValue::from_static("none"),
    );
    if reply.inline {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox; default-src 'none'"),
        );
    }
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    Ok(response)
}

pub fn attachment_read_router(
    targets: Vec<AttachmentReadHttpTarget>,
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
        .route("/api/v1/holdings/{holding_slug}/attachment-sets", get(list))
        .route(
            "/api/v1/holdings/{holding_slug}/attachment-links/{link_id}/status",
            get(status_of_link),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/attachment-links/{link_id}/withdraw",
            post(withdraw_link),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/attachment-links/{link_id}/content",
            get(content_by_link),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/attachments/{attachment_id}/content",
            get(content_by_attachment),
        )
        .with_state(Arc::new(AttachmentReadHttpState {
            targets: configured,
        })))
}

async fn list(
    State(state): State<Arc<AttachmentReadHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        let values = query_values(request.uri().query())?;
        let (filter, read_only) = list_filter(&values)?;
        ensure_empty_body(request).await?;
        target
            .backend
            .list(AttachmentReadHttpListRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                filter: &filter,
                read_only,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn status_of_link(
    State(state): State<Arc<AttachmentReadHttpState>>,
    Path((slug, link_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        if request.uri().query().is_some_and(|value| !value.is_empty()) {
            return Err(malformed());
        }
        if !valid_link_id(&link_id) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "ATTACHMENT_LINK_NOT_FOUND",
                "File link not found.",
            ));
        }
        ensure_empty_body(request).await?;
        target
            .backend
            .status(AttachmentReadHttpStatusRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WithdrawBody {
    reason: String,
}

fn command_key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let key =
        unique_header(headers, HeaderName::from_static("idempotency-key"))?.ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "A valid Idempotency-Key is required.",
            )
        })?;
    if !valid_command_key(key) {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "A valid Idempotency-Key is required.",
        ));
    }
    Ok(key.to_owned())
}

fn json_content_type(headers: &HeaderMap) -> Result<(), BusinessError> {
    let value = unique_header(headers, header::CONTENT_TYPE)?.ok_or_else(|| {
        BusinessError::invalid("ATTACHMENT_REQUEST_INVALID", "Review the file request.")
    })?;
    let mut parts = value.split(';');
    let media_type = parts.next().unwrap_or_default().trim();
    if !media_type.eq_ignore_ascii_case("application/json")
        || parts.any(|part| !part.trim().eq_ignore_ascii_case("charset=utf-8"))
    {
        return Err(BusinessError::invalid(
            "ATTACHMENT_REQUEST_INVALID",
            "Review the file request.",
        ));
    }
    Ok(())
}

async fn withdraw_link(
    State(state): State<Arc<AttachmentReadHttpState>>,
    Path((slug, link_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        if request.uri().query().is_some_and(|value| !value.is_empty()) {
            return Err(malformed());
        }
        if !valid_link_id(&link_id) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "ATTACHMENT_LINK_NOT_FOUND",
                "File link not found.",
            ));
        }
        let origin = unique_header(&headers, header::ORIGIN)?.ok_or_else(|| {
            BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
        })?;
        if origin != target.origin {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        json_content_type(&headers)?;
        let key = command_key(&headers)?;
        let bytes = to_bytes(request.into_body(), COMMAND_BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        if bytes.is_empty() {
            return Err(malformed());
        }
        let body: WithdrawBody = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        target
            .backend
            .withdraw(AttachmentCommandHttpWithdrawRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                link_id: &link_id,
                reason: &body.reason,
                command_key: &key,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn content_by_link(
    State(state): State<Arc<AttachmentReadHttpState>>,
    Path((slug, link_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let (variant, page) = match content_query(request.uri().query()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        if !valid_link_id(&link_id) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "ATTACHMENT_LINK_NOT_FOUND",
                "File link not found.",
            ));
        }
        ensure_empty_body(request).await?;
        target
            .backend
            .content_by_link(AttachmentContentHttpByLinkRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                link_id: &link_id,
                variant,
                page,
            })
            .await
    }
    .await;
    match result.and_then(|reply| content_response(reply, variant)) {
        Ok(response) => response,
        Err(error) => error_response(error),
    }
}

async fn content_by_attachment(
    State(state): State<Arc<AttachmentReadHttpState>>,
    Path((slug, attachment_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let (variant, page) = match content_query(request.uri().query()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        if !valid_link_id(&attachment_id) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "ATTACHMENT_NOT_FOUND",
                "Attachment not found.",
            ));
        }
        ensure_empty_body(request).await?;
        target
            .backend
            .content_by_attachment(AttachmentContentHttpByAttachmentRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                attachment_id: &attachment_id,
                variant,
                page,
            })
            .await
    }
    .await;
    match result.and_then(|reply| content_response(reply, variant)) {
        Ok(response) => response,
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serve;
    use std::sync::Mutex;

    const ORIGINAL: &[u8] = b"%PDF-1.4 native attachment";
    const PREVIEW: &[u8] = b"\x89WEBP native preview";

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
        broken: bool,
    }

    fn denied() -> BusinessError {
        BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
    }

    impl AttachmentReadHttpBackend for Probe {
        fn list<'a>(
            &'a self,
            _: AttachmentReadHttpListRequest<'a>,
        ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
            Box::pin(async { Err(denied()) })
        }
        fn status<'a>(
            &'a self,
            _: AttachmentReadHttpStatusRequest<'a>,
        ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
            Box::pin(async { Err(denied()) })
        }
        fn withdraw<'a>(
            &'a self,
            _: AttachmentCommandHttpWithdrawRequest<'a>,
        ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
            Box::pin(async { Err(denied()) })
        }
        fn content_by_link<'a>(
            &'a self,
            request: AttachmentContentHttpByLinkRequest<'a>,
        ) -> AttachmentReadHttpFuture<'a, AttachmentContentHttpReply> {
            let cookie = request
                .headers
                .get("cookie")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_owned();
            self.seen.lock().unwrap().push(format!(
                "link|{}|{}|{}|{}|{}|{cookie}",
                request.host,
                request.holding_slug,
                request.link_id,
                request.variant,
                request.page.unwrap_or(0)
            ));
            let preview = request.variant == "preview";
            Box::pin(async move {
                Ok(AttachmentContentHttpReply {
                    media_type: if preview {
                        "image/webp"
                    } else {
                        "application/pdf"
                    }
                    .into(),
                    filename: if preview {
                        "preview.png"
                    } else {
                        "evidence.pdf"
                    }
                    .into(),
                    inline: preview,
                    body: if preview { PREVIEW } else { ORIGINAL }.to_vec(),
                    checksum_sha256: if self.broken {
                        "0".repeat(64)
                    } else {
                        hex::encode(Sha256::digest(if preview { PREVIEW } else { ORIGINAL }))
                    },
                })
            })
        }
        fn content_by_attachment<'a>(
            &'a self,
            request: AttachmentContentHttpByAttachmentRequest<'a>,
        ) -> AttachmentReadHttpFuture<'a, AttachmentContentHttpReply> {
            self.seen.lock().unwrap().push(format!(
                "attachment|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.attachment_id,
                request.variant,
                request.page.unwrap_or(0)
            ));
            Box::pin(async move {
                Ok(AttachmentContentHttpReply {
                    media_type: "image/webp".into(),
                    filename: "preview.png".into(),
                    inline: true,
                    body: PREVIEW.to_vec(),
                    checksum_sha256: hex::encode(Sha256::digest(PREVIEW)),
                })
            })
        }
    }

    struct DefaultBackend;
    impl AttachmentReadHttpBackend for DefaultBackend {
        fn list<'a>(
            &'a self,
            _: AttachmentReadHttpListRequest<'a>,
        ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
            Box::pin(async { Err(denied()) })
        }
        fn status<'a>(
            &'a self,
            _: AttachmentReadHttpStatusRequest<'a>,
        ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
            Box::pin(async { Err(denied()) })
        }
        fn withdraw<'a>(
            &'a self,
            _: AttachmentCommandHttpWithdrawRequest<'a>,
        ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
            Box::pin(async { Err(denied()) })
        }
    }

    fn target(host: &str, backend: Arc<dyn AttachmentReadHttpBackend>) -> AttachmentReadHttpTarget {
        AttachmentReadHttpTarget {
            host: host.into(),
            holding_slug: "acme".into(),
            origin: format!("https://{host}"),
            backend,
        }
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, Vec<u8>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let split = bytes
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .unwrap()
            + 4;
        let text = String::from_utf8_lossy(&bytes[..split]).to_string();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        (status, text.to_ascii_lowercase(), bytes[split..].to_vec())
    }

    fn get(path: &str, host: &str) -> String {
        format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\nConnection: close\r\n\r\n"
        )
    }

    #[test]
    fn content_query_normalizes_page_and_rejects_unknown_keys() {
        assert_eq!(content_query(None).unwrap(), ("original", None));
        assert_eq!(
            content_query(Some("variant=original")).unwrap(),
            ("original", None)
        );
        assert_eq!(
            content_query(Some("variant=original&page=2&frontLocation=front")).unwrap(),
            ("preview", Some(2))
        );
        assert_eq!(content_query(Some("page=1")).unwrap(), ("preview", Some(1)));
        assert!(content_query(Some("variant=thumb")).is_err());
        assert!(content_query(Some("page=0")).is_err());
        assert!(content_query(Some("variant=a&variant=b")).is_err());
        assert!(content_query(Some("extra=1")).is_err());
    }

    #[tokio::test]
    async fn content_routes_serve_exact_status_type_range_and_bytes() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = attachment_read_router(vec![target(
            "acme.example.test",
            Arc::new(Probe {
                seen: seen.clone(),
                broken: false,
            }),
        )])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });

        let (status, headers, body) = http(
            addr,
            get(
                "/api/v1/holdings/acme/attachment-links/link-1/content?variant=original",
                "acme.example.test",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert!(headers.contains("content-type: application/pdf"));
        assert!(headers.contains("content-disposition: attachment; filename*=utf-8''evidence.pdf"));
        assert!(headers.contains("accept-ranges: none"));
        assert!(headers.contains("cache-control: private, no-store"));
        assert!(headers.contains("referrer-policy: no-referrer"));
        assert!(headers.contains("x-content-type-options: nosniff"));
        assert!(headers.contains(&format!(
            "x-erpx-checksum-sha256: {}",
            hex::encode(Sha256::digest(ORIGINAL))
        )));
        assert!(!headers.contains("content-security-policy"));
        assert_eq!(body, ORIGINAL);

        let (status, headers, body) = http(
            addr,
            get(
                "/api/v1/holdings/acme/attachments/attachment-1/content?page=2",
                "acme.example.test",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert!(headers.contains("content-type: image/webp"));
        assert!(headers.contains("content-disposition: inline; filename*=utf-8''preview.png"));
        assert!(headers.contains("content-security-policy: sandbox; default-src 'none'"));
        assert!(headers.contains("accept-ranges: none"));
        assert_eq!(body, PREVIEW);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                "link|acme.example.test|acme|link-1|original|0|erpx=session",
                "attachment|acme.example.test|acme|attachment-1|preview|2",
            ]
        );
    }

    #[tokio::test]
    async fn content_routes_fail_closed_before_the_backend_and_on_bad_bytes() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let router = attachment_read_router(vec![
            target(
                "acme.example.test",
                Arc::new(Probe {
                    seen: seen.clone(),
                    broken: false,
                }),
            ),
            target(
                "broken.example.test",
                Arc::new(Probe {
                    seen: seen.clone(),
                    broken: true,
                }),
            ),
            target("default.example.test", Arc::new(DefaultBackend)),
        ])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });

        for path in [
            "/api/v1/holdings/acme/attachment-links/link-1/content?variant=thumb",
            "/api/v1/holdings/acme/attachment-links/link-1/content?page=0",
            "/api/v1/holdings/acme/attachment-links/link-1/content?extra=1",
            "/api/v1/holdings/other/attachment-links/link-1/content",
            "/api/v1/holdings/acme/attachment-links/link%2Fother/content",
        ] {
            let (status, _, _) = http(addr, get(path, "acme.example.test")).await;
            assert_eq!(
                status,
                if path.contains("/holdings/other/") || path.contains("%2F") {
                    404
                } else {
                    422
                },
                "{path}"
            );
        }
        let (status, _, _) = http(
            addr,
            get(
                "/api/v1/holdings/acme/attachment-links/link-1/content",
                "other.example.test",
            ),
        )
        .await;
        assert_eq!(status, 404);
        let (status, _, _) = http(
            addr,
            get(
                "/api/v1/holdings/acme/attachment-links/link-1/content",
                "broken.example.test",
            ),
        )
        .await;
        assert_eq!(status, 503);
        let (status, _, body) = http(
            addr,
            get(
                "/api/v1/holdings/acme/attachment-links/link-1/content",
                "default.example.test",
            ),
        )
        .await;
        assert_eq!(status, 503);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"]["code"],
            "ATTACHMENT_CONTENT_UNAVAILABLE"
        );
    }
}
