//! Single native axum command router for every PMS owner write (wf1a2 slice).
//!
//! Transport facts only: exact registered Host/Holding target and Origin
//! (browser route), one bounded Idempotency-Key, an allow-listed typed `kind`
//! and an ordered JSON values array. The router never accepts SQL text and
//! never mints authority; the composition backend resolves the current
//! account/authority and executes the owner-selected statement on a
//! serializable unit with receipt/audit/outbox effects.
use crate::domain::pms_owner_write::supported;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 1_048_576;

pub type PmsOwnerWriteFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct PmsOwnerWriteRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub company_id: &'a str,
    pub user_id: &'a str,
    pub assignment_id: &'a str,
    pub authority_json: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub kind: &'a str,
    pub values: &'a [Value],
    pub now: i64,
}

pub trait PmsOwnerWriteBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PmsOwnerWriteRequest<'a>,
    ) -> PmsOwnerWriteFuture<'a, Value>;
}

pub struct PmsOwnerWriteTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn PmsOwnerWriteBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), PmsOwnerWriteTarget>,
    company_targets: BTreeMap<String, Arc<dyn PmsOwnerWriteBackend>>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PMS_OWNER_WRITE_INVALID",
        "Review the hotel write request.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host.bytes().any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
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

fn bounded(data: Value, context: Value) -> Response {
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({"data": data, "context": context})),
    )
        .into_response();
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

fn selector(headers: &HeaderMap, name: &'static str, max: usize) -> Result<String, BusinessError> {
    let value = unique_header(headers, HeaderName::from_static(name))?
        .ok_or_else(invalid)?
        .to_owned();
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(value)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WriteCommand {
    kind: String,
    values: Vec<Value>,
    #[serde(default)]
    now: i64,
}

fn decode(raw: Value) -> Result<WriteCommand, BusinessError> {
    let command: WriteCommand = serde_json::from_value(raw).map_err(|_| invalid())?;
    if !supported(&command.kind)
        || command.values.is_empty()
        || command.values.len() > 64
        || command.values.iter().any(|value| !value.is_string()
            && !value.is_number()
            && !value.is_boolean()
            && !value.is_null())
    {
        return Err(invalid());
    }
    Ok(command)
}

async fn body(request: Request) -> Result<Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller hotel write request."))?;
    if bytes.is_empty() {
        return Err(invalid());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if !value.is_object() {
        return Err(invalid());
    }
    Ok(value)
}

fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PmsOwnerWriteTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let target = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    if unique_header(headers, header::ORIGIN)? != Some(target.origin.as_str()) {
        return Err(origin_forbidden());
    }
    Ok(target)
}

fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    let key = unique_header(headers, HeaderName::from_static("idempotency-key"))?
        .ok_or_else(|| BusinessError::invalid("COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key."))?;
    CommandKey::new(key)
}

fn context_value(headers: &HeaderMap, company: &str, user: &str, assignment: &str) -> Value {
    let branch = unique_header(headers, HeaderName::from_static("x-erpx-workspace-branch"))
        .ok()
        .flatten()
        .map(str::to_owned);
    let department = unique_header(headers, HeaderName::from_static("x-erpx-workspace-department"))
        .ok()
        .flatten()
        .map(str::to_owned);
    serde_json::json!({
        "companyId": company,
        "userId": user,
        "assignmentId": assignment,
        "branchSlug": branch,
        "departmentSlug": department,
    })
}

async fn write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let key = command_key(&headers)?;
        let command = decode(body(request).await?)?;
        let company = selector(&headers, "x-erpx-context-company", 200)?;
        CompanyId::new(&company).map_err(|_| invalid())?;
        let user = selector(&headers, "x-erpx-actor-user", 200)?;
        let assignment = selector(&headers, "x-erpx-context-assignment", 200)?;
        let authority = unique_header(&headers, HeaderName::from_static("x-erpx-authority-json"))?
            .unwrap_or("{}")
            .to_owned();
        let context = context_value(&headers, &company, &user, &assignment);
        let data = target
            .backend
            .execute(PmsOwnerWriteRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                company_id: &company,
                user_id: &user,
                assignment_id: &assignment,
                authority_json: &authority,
                headers: &headers,
                key: key.as_str(),
                kind: &command.kind,
                values: &command.values,
                now: command.now,
            })
            .await?;
        Ok::<_, BusinessError>(bounded(data, context))
    }
    .await;
    match result {
        Ok(response) => response,
        Err(error) => error_response(error),
    }
}

async fn company_write(
    State(state): State<Arc<AppState>>,
    Path(company): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state.company_targets.get(&company).ok_or_else(missing)?;
        let key = command_key(&headers)?;
        let command = decode(body(request).await?)?;
        CompanyId::new(&company).map_err(|_| invalid())?;
        let user = unique_header(&headers, HeaderName::from_static("x-erpx-actor-user"))
            .ok()
            .flatten()
            .unwrap_or("system:pms")
            .to_owned();
        let context = serde_json::json!({"companyId": company, "userId": user, "system": true});
        let data = backend
            .execute(PmsOwnerWriteRequest {
                host: "",
                holding_slug: "",
                company_id: &company,
                user_id: &user,
                assignment_id: "system:pms",
                authority_json: "{}",
                headers: &headers,
                key: key.as_str(),
                kind: &command.kind,
                values: &command.values,
                now: command.now,
            })
            .await?;
        Ok::<_, BusinessError>(bounded(data, context))
    }
    .await;
    match result {
        Ok(response) => response,
        Err(error) => error_response(error),
    }
}

/// Build the browser-facing owner-write router from exact registered targets.
pub fn pms_owner_write_router(
    targets: Vec<PmsOwnerWriteTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PMS_OWNER_WRITE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/owner-write",
            post(write),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            company_targets: BTreeMap::new(),
        })))
}

/// Build the Company-keyed system router for workers without a browser context.
pub fn pms_owner_write_company_router(
    pairs: Vec<(String, Arc<dyn PmsOwnerWriteBackend>)>,
) -> Result<Router, BusinessError> {
    let mut company_targets = BTreeMap::new();
    for (company, backend) in pairs {
        if company.is_empty() || CompanyId::new(&company).is_err() {
            return Err(invalid());
        }
        company_targets.insert(company, backend);
    }
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/pms/owner-write",
            post(company_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            company_targets,
        })))
}
