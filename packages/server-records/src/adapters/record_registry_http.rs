//! Native Records registry HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded module
//! path and strict list/page query. Normal Account session resolution, selected
//! Company/branch/department scope, current IAM authority, cross-owner row
//! admission and the serializable read-only owner unit remain composition
//! responsibilities behind [`RecordsRegistryHttpBackend`].
use crate::domain::record_context::valid_module;
use crate::domain::record_registry::RegistryQuery;
use crate::domain::record_write::CreateRecordInput;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

pub type RecordsRegistryHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum RecordsRegistryHttpCommand {
    List {
        module_id: String,
        section_id: String,
    },
    Page {
        module_id: String,
        section_id: String,
        q: Option<String>,
        cursor: Option<String>,
    },
    Create {
        module_id: String,
        input: CreateRecordInput,
        key: String,
    },
}

pub struct RecordsRegistryHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: RecordsRegistryHttpCommand,
}

#[derive(Clone, Debug)]
pub struct RecordsRegistryHttpResponse {
    /// Owner list/page data. The backend adds the validated current context.
    pub data: Value,
    pub context: Value,
}

impl RecordsRegistryHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, module or permission.
pub trait RecordsRegistryHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: RecordsRegistryHttpRequest<'a>,
    ) -> RecordsRegistryHttpFuture<'a, RecordsRegistryHttpResponse>;
}

pub struct RecordsRegistryHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn RecordsRegistryHttpBackend>,
}

struct RecordsRegistryHttpState {
    targets: BTreeMap<(String, String), RecordsRegistryHttpTarget>,
}

const QUERY_LIMIT: usize = 4_096;

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    crate::domain::record_registry::request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "RECORDS_REGISTRY_CONFIGURATION_INVALID",
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
    state: &'a RecordsRegistryHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a RecordsRegistryHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
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
    let raw = query.unwrap_or_default();
    if raw.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    let mut values = BTreeMap::new();
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').ok_or_else(malformed)?;
        let key = percent_decode(key)?;
        let value = percent_decode(value)?;
        let allowed = matches!(key.as_str(), "paged" | "section" | "q" | "cursor");
        if !allowed || values.insert(key, value).is_some() {
            return Err(malformed());
        }
    }
    Ok(values)
}

fn command(
    module_id: &str,
    values: &BTreeMap<String, String>,
) -> Result<RecordsRegistryHttpCommand, BusinessError> {
    if !valid_module(module_id) {
        return Err(missing());
    }
    let section_id = values
        .get("section")
        .filter(|value| !value.is_empty())
        .ok_or_else(malformed)?;
    match values.get("paged").map(String::as_str) {
        None => {
            if values.contains_key("q") || values.contains_key("cursor") {
                return Err(malformed());
            }
            RegistryQuery::list(module_id, section_id)?;
            Ok(RecordsRegistryHttpCommand::List {
                module_id: module_id.to_owned(),
                section_id: section_id.clone(),
            })
        }
        Some("1") => {
            let query = RegistryQuery::page(
                module_id,
                section_id,
                values.get("q").map(String::as_str),
                values.get("cursor").map(String::as_str),
            )?;
            Ok(RecordsRegistryHttpCommand::Page {
                module_id: query.module_id().to_owned(),
                section_id: query.section_id().to_owned(),
                q: query.q().map(str::to_owned),
                cursor: query.cursor().map(str::to_owned),
            })
        }
        Some(_) => Err(malformed()),
    }
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

pub fn records_registry_http_router(
    targets: Vec<RecordsRegistryHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/records/{module_id}",
            get(read_registry).post(create_record),
        )
        .with_state(Arc::new(RecordsRegistryHttpState {
            targets: configured,
        })))
}

async fn read_registry(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        let values = query_values(request.uri().query())?;
        let command = command(&module_id, &values)?;
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(RecordsRegistryHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn create_record(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        if unique_header(&headers, header::ORIGIN)?.is_none_or(|origin| origin != target.origin) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid(
                    "IDEMPOTENCY_KEY_REQUIRED",
                    "Provide a command key to create this record.",
                )
            })?;
        CommandKey::new(key.to_owned()).map_err(|_| {
            BusinessError::invalid(
                "IDEMPOTENCY_KEY_REQUIRED",
                "Provide a command key to create this record.",
            )
        })?;
        let content_type = unique_header(&headers, header::CONTENT_TYPE)?
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json" {
            return Err(BusinessError::invalid(
                "JSON_REQUIRED",
                "Submit the record as JSON.",
            ));
        }
        if !valid_module(&module_id) {
            return Err(missing());
        }
        let bytes = to_bytes(request.into_body(), 64 * 1024)
            .await
            .map_err(|_| malformed())?;
        let input: CreateRecordInput = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        target
            .backend
            .execute(RecordsRegistryHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: RecordsRegistryHttpCommand::Create {
                    module_id,
                    input,
                    key: key.to_owned(),
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
