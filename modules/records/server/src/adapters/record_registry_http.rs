//! Native Records registry HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded module
//! path and strict list/page query. Normal Account session resolution, selected
//! Company/branch/department scope, current IAM authority, cross-owner row
//! admission and the serializable read-only owner unit remain composition
//! responsibilities behind [`RecordsRegistryHttpBackend`].
use crate::domain::record_context::valid_module;
use crate::domain::record_registry::RegistryQuery;
use crate::domain::record_social::{
    AddPrincipalMentionInput, SuggestionDecisionInput, SuggestionInput, decision_input_invalid,
    mention_input_invalid, suggestion_input_invalid, validate_decision_input,
    validate_mention_input, validate_suggestion_input,
};
use crate::domain::record_write::{
    ArchiveRecordInput, CreateRecordInput, MAX_SAFE, ReferenceRecordInput, TransitionRecordInput,
    UpdateRecordInput,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode,
    body::to_bytes,
    header,
    routing::{any, get, patch, post},
    serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
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
    Update {
        module_id: String,
        record_id: String,
        input: UpdateRecordInput,
        key: String,
    },
    Archive {
        module_id: String,
        record_id: String,
        input: ArchiveRecordInput,
        key: String,
    },
    Reference {
        module_id: String,
        record_id: String,
        input: ReferenceRecordInput,
        key: String,
    },
    Mention {
        module_id: String,
        record_id: String,
        input: AddPrincipalMentionInput,
        key: Option<CommandKey>,
    },
    RequestSuggestion {
        module_id: String,
        record_id: String,
        input: SuggestionInput,
        key: Option<CommandKey>,
    },
    DecideSuggestion {
        module_id: String,
        record_id: String,
        suggestion_id: String,
        input: SuggestionDecisionInput,
        key: Option<CommandKey>,
    },
    Transition {
        module_id: String,
        record_id: String,
        input: TransitionRecordInput,
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
    let status = if error.code == "REQUEST_TOO_LARGE" {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        status(error.kind)
    };
    (
        status,
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

fn query_values(
    query: Option<&str>,
    allow_expected_version: bool,
) -> Result<BTreeMap<String, String>, BusinessError> {
    let raw = query.unwrap_or_default();
    if raw.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    let mut values = BTreeMap::new();
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').ok_or_else(malformed)?;
        let key = percent_decode(key)?;
        let value = percent_decode(value)?;
        let allowed = matches!(key.as_str(), "paged" | "section" | "q" | "cursor")
            || (allow_expected_version && key == "expectedVersion");
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

/// Next derived OPTIONS as 204 + `Allow` for the retired collection route.
/// Keep that contract on the native entry so browser preflights do not change.
/// Axum's `get` already serves HEAD with the body removed.
async fn collection_options() -> Response {
    options_response("GET, HEAD, OPTIONS, POST")
}

async fn record_options() -> Response {
    options_response("DELETE, GET, HEAD, OPTIONS, PATCH")
}

async fn command_options() -> Response {
    options_response("OPTIONS, POST")
}

async fn read_action_body<T: serde::de::DeserializeOwned>(
    request: Request,
) -> Result<T, BusinessError> {
    let public = request
        .headers()
        .get("x-erpx-api-transport")
        .is_some_and(|value| value == "public");
    let bytes = to_bytes(request.into_body(), 64 * 1024)
        .await
        .map_err(|_| {
            if public {
                BusinessError::invalid("REQUEST_TOO_LARGE", "This record is too large.")
            } else {
                malformed()
            }
        })?;
    serde_json::from_slice(&bytes).map_err(|_| malformed())
}

fn options_response(allow: &'static str) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(allow));
    response
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
            get(read_registry)
                .post(create_record)
                .options(collection_options),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}",
            patch(update_record)
                .delete(delete_record)
                .options(record_options),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/references",
            post(add_reference).options(command_options),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/mentions",
            any(|| async { StatusCode::METHOD_NOT_ALLOWED })
                .post(add_mention)
                .options(command_options),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/suggestions",
            any(|| async { StatusCode::METHOD_NOT_ALLOWED })
                .post(add_suggestion)
                .options(command_options),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/suggestions/{suggestion_id}",
            any(|| async { StatusCode::METHOD_NOT_ALLOWED })
                .post(decide_suggestion)
                .options(command_options),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/transition",
            post(transition_record).options(command_options),
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
        let values = query_values(request.uri().query(), false)?;
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

async fn delete_record(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id, record_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
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
                    "Provide a command key to archive this record.",
                )
            })?;
        CommandKey::new(key.to_owned()).map_err(|_| {
            BusinessError::invalid(
                "IDEMPOTENCY_KEY_REQUIRED",
                "Provide a command key to archive this record.",
            )
        })?;
        if !valid_module(&module_id) || !crate::domain::record_context::valid_id(&record_id) {
            return Err(missing());
        }
        let values = query_values(query.as_deref(), true)?;
        if values.len() != 1 {
            return Err(malformed());
        }
        let expected_version = values
            .get("expectedVersion")
            .ok_or_else(malformed)?
            .parse::<i64>()
            .map_err(|_| malformed())?;
        if !(1..=MAX_SAFE).contains(&expected_version) {
            return Err(malformed());
        }
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(RecordsRegistryHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: RecordsRegistryHttpCommand::Archive {
                    module_id,
                    record_id,
                    input: ArchiveRecordInput { expected_version },
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

async fn add_reference(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id, record_id)): Path<(String, String, String)>,
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
                    "Provide a command key to add this reference.",
                )
            })?;
        CommandKey::new(key.to_owned()).map_err(|_| {
            BusinessError::invalid(
                "IDEMPOTENCY_KEY_REQUIRED",
                "Provide a command key to add this reference.",
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
                "Submit the reference as JSON.",
            ));
        }
        if !valid_module(&module_id) || !crate::domain::record_context::valid_id(&record_id) {
            return Err(missing());
        }
        let input: ReferenceRecordInput = read_action_body(request).await?;
        target
            .backend
            .execute(RecordsRegistryHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: RecordsRegistryHttpCommand::Reference {
                    module_id,
                    record_id,
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

async fn add_mention(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id, record_id)): Path<(String, String, String)>,
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
        if !valid_module(&module_id) || !crate::domain::record_context::valid_id(&record_id) {
            return Err(missing());
        }
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?
            .map(CommandKey::new)
            .transpose()?;
        let input = read_action_body(request).await.map_err(|error| {
            if error.code == "REQUEST_TOO_LARGE" {
                error
            } else {
                mention_input_invalid()
            }
        })?;
        validate_mention_input(&input)?;
        target
            .backend
            .execute(RecordsRegistryHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: RecordsRegistryHttpCommand::Mention {
                    module_id,
                    record_id,
                    input,
                    key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => {
            let mut response = bounded(serde_json::json!({"data": reply.data}));
            *response.status_mut() = StatusCode::CREATED;
            response
        }
        Err(error) => error_response(error),
    }
}

async fn add_suggestion(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id, record_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    write_suggestion(state, slug, module_id, record_id, None, request).await
}

async fn decide_suggestion(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id, record_id, suggestion_id)): Path<(String, String, String, String)>,
    request: Request,
) -> Response {
    write_suggestion(
        state,
        slug,
        module_id,
        record_id,
        Some(suggestion_id),
        request,
    )
    .await
}

async fn write_suggestion(
    state: Arc<RecordsRegistryHttpState>,
    slug: String,
    module_id: String,
    record_id: String,
    suggestion_id: Option<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let status = if suggestion_id.is_some() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        if unique_header(&headers, header::ORIGIN)?.is_none_or(|origin| origin != target.origin) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let valid_id = crate::domain::record_context::valid_id;
        if !valid_module(&module_id)
            || !valid_id(&record_id)
            || suggestion_id.as_deref().is_some_and(|id| !valid_id(id))
        {
            return Err(missing());
        }
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?
            .map(CommandKey::new)
            .transpose()?;
        let invalid = if suggestion_id.is_some() {
            decision_input_invalid
        } else {
            suggestion_input_invalid
        };
        let input_error = |error: BusinessError| {
            if error.code == "REQUEST_TOO_LARGE" {
                error
            } else {
                invalid()
            }
        };
        let command = match suggestion_id {
            Some(suggestion_id) => {
                let input = read_action_body(request).await.map_err(input_error)?;
                validate_decision_input(&input)?;
                RecordsRegistryHttpCommand::DecideSuggestion {
                    module_id,
                    record_id,
                    suggestion_id,
                    input,
                    key,
                }
            }
            None => {
                let input = read_action_body(request).await.map_err(input_error)?;
                validate_suggestion_input(&input)?;
                RecordsRegistryHttpCommand::RequestSuggestion {
                    module_id,
                    record_id,
                    input,
                    key,
                }
            }
        };
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
        Ok(reply) => {
            let mut response = bounded(serde_json::json!({"data": reply.data}));
            *response.status_mut() = status;
            response
        }
        Err(error) => error_response(error),
    }
}

async fn transition_record(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id, record_id)): Path<(String, String, String)>,
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
                    "Provide a command key to transition this record.",
                )
            })?;
        CommandKey::new(key.to_owned()).map_err(|_| {
            BusinessError::invalid(
                "IDEMPOTENCY_KEY_REQUIRED",
                "Provide a command key to transition this record.",
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
                "Submit the transition as JSON.",
            ));
        }
        if !valid_module(&module_id) || !crate::domain::record_context::valid_id(&record_id) {
            return Err(missing());
        }
        let input: TransitionRecordInput = read_action_body(request).await?;
        target
            .backend
            .execute(RecordsRegistryHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: RecordsRegistryHttpCommand::Transition {
                    module_id,
                    record_id,
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

async fn update_record(
    State(state): State<Arc<RecordsRegistryHttpState>>,
    Path((slug, module_id, record_id)): Path<(String, String, String)>,
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
                    "Provide a command key to update this record.",
                )
            })?;
        CommandKey::new(key.to_owned()).map_err(|_| {
            BusinessError::invalid(
                "IDEMPOTENCY_KEY_REQUIRED",
                "Provide a command key to update this record.",
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
        if !valid_module(&module_id) || !crate::domain::record_context::valid_id(&record_id) {
            return Err(missing());
        }
        let input: UpdateRecordInput = read_action_body(request).await?;
        target
            .backend
            .execute(RecordsRegistryHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: RecordsRegistryHttpCommand::Update {
                    module_id,
                    record_id,
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

#[cfg(test)]
mod wire_parity_tests {
    use super::*;
    use crate::domain::record_context::RecordView;
    use crate::domain::record_registry::RegistryPage;

    fn keys(value: &Value) -> Vec<&str> {
        let mut names = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }

    #[test]
    fn reply_is_the_exact_owner_data_context_envelope() {
        let reply = RecordsRegistryHttpResponse {
            data: serde_json::json!({"id": "rec-1", "version": 1}),
            context: serde_json::json!({"userId": "actor", "revision": 9}),
        }
        .value();
        assert_eq!(keys(&reply), vec!["context", "data"]);
        assert_eq!(reply["data"]["id"], "rec-1");
        assert_eq!(reply["data"]["version"], 1);
        assert_eq!(reply["context"]["userId"], "actor");
        assert_eq!(reply["context"]["revision"], 9);
    }

    #[test]
    fn wire_shapes_have_no_link_fields_for_typescript_to_reshape() {
        let view = RecordView {
            module_id: "operations".into(),
            id: "rec-1".into(),
            section_id: "overview".into(),
            title: "Title".into(),
            description: "Description".into(),
            status: "draft".into(),
            fields: serde_json::json!([{"key":"total","label":"Total","type":"number","value":3}]),
            version: 1,
            created_by: "actor".into(),
            updated_at: 7,
        }
        .to_wire();
        assert_eq!(
            keys(&view),
            vec![
                "createdBy",
                "description",
                "fields",
                "id",
                "moduleId",
                "sectionId",
                "status",
                "title",
                "updatedAt",
                "version"
            ]
        );
        assert!(
            keys(&view)
                .iter()
                .all(|key| *key != "href" && !key.ends_with("Href") && !key.ends_with("_href"))
        );
        let page = RegistryPage {
            items: Vec::new(),
            next_cursor: None,
            previous_cursor: None,
            has_more: false,
        }
        .to_wire();
        assert_eq!(
            keys(&page),
            vec!["hasMore", "items", "nextCursor", "previousCursor"]
        );
    }

    #[tokio::test]
    async fn collection_options_preserves_the_retired_next_preflight() {
        let response = collection_options().await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers().get(header::ALLOW).unwrap(),
            "GET, HEAD, OPTIONS, POST"
        );
    }

    #[test]
    fn owner_failure_statuses_are_the_exact_transport_contract() {
        for (kind, expected) in [
            (FailureKind::InvalidInput, StatusCode::UNPROCESSABLE_ENTITY),
            (FailureKind::Unauthenticated, StatusCode::UNAUTHORIZED),
            (FailureKind::Forbidden, StatusCode::FORBIDDEN),
            (FailureKind::NotFound, StatusCode::NOT_FOUND),
            (FailureKind::Conflict, StatusCode::CONFLICT),
            (FailureKind::Unavailable, StatusCode::SERVICE_UNAVAILABLE),
        ] {
            assert_eq!(status(kind), expected);
        }
    }
}
