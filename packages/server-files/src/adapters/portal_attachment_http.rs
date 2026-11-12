//! Owner-local portal HTTP ingress for the native Customer Files withdraw.
//!
//! Transport facts only: one exact registered portal Host/Holding target, the
//! browser Origin for that target, strict JSON plus an idempotency key and a
//! bounded reason. The verified portal session, its customer binding and the
//! Files serializable unit remain composition responsibilities behind
//! [`PortalAttachmentHttpBackend`]. No Account context header is accepted.
use crate::domain::attachment_command::valid_command_key;
use crate::domain::attachment_read::MAX_IDENTIFIER_BYTES;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const COMMAND_BODY_LIMIT: usize = 8 * 1024;

pub type PortalAttachmentHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// One bounded portal remove command. The caller must already hold the exact
/// portal product session; this request never carries an identity header.
pub struct PortalAttachmentHttpWithdrawRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub link_id: &'a str,
    pub reason: &'a str,
    pub command_key: &'a str,
}

/// Owner data only. The portal surface has no staff context projection.
#[derive(Clone, Debug)]
pub struct PortalAttachmentHttpReply {
    pub data: Value,
}

pub trait PortalAttachmentHttpBackend: Send + Sync {
    fn withdraw<'a>(
        &'a self,
        request: PortalAttachmentHttpWithdrawRequest<'a>,
    ) -> PortalAttachmentHttpFuture<'a, PortalAttachmentHttpReply>;
}

pub struct PortalAttachmentHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PortalAttachmentHttpBackend>,
}

struct PortalAttachmentHttpState {
    targets: BTreeMap<(String, String), PortalAttachmentHttpTarget>,
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
    state: &'a PortalAttachmentHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PortalAttachmentHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
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

pub fn portal_attachment_http_router(
    targets: Vec<PortalAttachmentHttpTarget>,
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
            "/api/customer-portal/{holding_slug}/attachments/attachment-links/{link_id}/withdraw",
            post(withdraw_link),
        )
        .with_state(Arc::new(PortalAttachmentHttpState {
            targets: configured,
        })))
}

async fn withdraw_link(
    State(state): State<Arc<PortalAttachmentHttpState>>,
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
        let reply = target
            .backend
            .withdraw(PortalAttachmentHttpWithdrawRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                link_id: &link_id,
                reason: &body.reason,
                command_key: &key,
            })
            .await?;
        Ok::<_, BusinessError>(serde_json::json!({"data": reply.data}))
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}
