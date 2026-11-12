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
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 0;
const COMMAND_BODY_LIMIT: usize = 8 * 1024;

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
