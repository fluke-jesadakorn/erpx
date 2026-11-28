//! Native PMS cutover-import write HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding target, Origin,
//! Idempotency-Key, bounded JSON body and a bounded work-context selection.
//! Account session resolution, current IAM/assignment authority, the
//! serializable owner unit and the PMS actor remain composition
//! responsibilities behind [`PmsImportHttpBackend`]. No context value below is
//! trusted as authority; composition resolves and compares each selector with
//! the retained session before it builds a
//! [`crate::application::pms_import::PmsImportActor`].
use crate::domain::pms_import::{PmsImportCommandKind, invalid};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 1_048_576;

pub type PmsImportHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Selected work context supplied with the write. These are selectors for
/// composition to resolve against the retained Account session; they are not
/// proof of authority, a staff label or a Company grant.
#[derive(Clone, Debug)]
pub struct PmsImportHttpContext {
    pub company_id: String,
    pub assignment_id: String,
    pub user_id: String,
}

pub struct PmsImportHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub context: PmsImportHttpContext,
    pub key: CommandKey,
    pub kind: PmsImportCommandKind,
    pub facts: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct PmsImportHttpResponse {
    /// Owner command result. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl PmsImportHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait PmsImportHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: PmsImportHttpCommandRequest<'a>,
    ) -> PmsImportHttpFuture<'a, PmsImportHttpResponse>;
}

pub struct PmsImportHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PmsImportHttpBackend>,
}

struct PmsImportHttpState {
    targets: BTreeMap<(String, String), PmsImportHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Review the current workspace and role.",
    )
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

fn bounded(body: serde_json::Value) -> Response {
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
    state: &'a PmsImportHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PmsImportHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn command_origin(target: &PmsImportHttpTarget, headers: &HeaderMap) -> Result<(), BusinessError> {
    if unique_header(headers, header::ORIGIN)? != Some(target.origin.as_str()) {
        return Err(origin_forbidden());
    }
    Ok(())
}

fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    let key =
        unique_header(headers, HeaderName::from_static("idempotency-key"))?.ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Use a valid Idempotency-Key for this action.",
            )
        })?;
    CommandKey::new(key)
}

fn command_content_type(headers: &HeaderMap) -> Result<(), BusinessError> {
    if !unique_header(headers, header::CONTENT_TYPE)?.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return Err(invalid());
    }
    Ok(())
}

fn selector(value: &str) -> Result<String, BusinessError> {
    if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

fn context(headers: &HeaderMap) -> Result<PmsImportHttpContext, BusinessError> {
    let header = |name: &'static str| {
        unique_header(headers, HeaderName::from_static(name))?
            .ok_or_else(context_changed)
            .and_then(selector)
    };
    let company_id =
        CompanyId::new(header("x-erpx-context-company")?).map_err(|_| context_changed())?;
    let assignment_id =
        AssignmentId::new(header("x-erpx-context-assignment")?).map_err(|_| context_changed())?;
    let user_id = header("x-erpx-actor-user")?;
    Ok(PmsImportHttpContext {
        company_id: company_id.as_str().to_owned(),
        assignment_id: assignment_id.as_str().to_owned(),
        user_id,
    })
}

async fn command_body(request: Request) -> Result<serde_json::Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| {
            BusinessError::invalid(
                "BODY_TOO_LARGE",
                "Use a smaller cutover-import write request.",
            )
        })?;
    if bytes.is_empty() {
        return Err(invalid());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if !value.is_object() {
        return Err(invalid());
    }
    Ok(value)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WriteBody {
    command: PmsImportCommandKind,
    facts: serde_json::Value,
}

fn decode_command(
    body: serde_json::Value,
) -> Result<(PmsImportCommandKind, serde_json::Value), BusinessError> {
    let decoded: WriteBody = serde_json::from_value(body).map_err(|_| invalid())?;
    if !decoded.facts.is_object() {
        return Err(invalid());
    }
    Ok((decoded.command, decoded.facts))
}

/// Build the native cutover-import write router from exact registered targets.
pub fn pms_import_router(targets: Vec<PmsImportHttpTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PMS_IMPORT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/import-command",
            post(command),
        )
        .with_state(Arc::new(PmsImportHttpState {
            targets: configured,
        })))
}

async fn command(
    State(state): State<Arc<PmsImportHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        command_origin(target, request.headers())?;
        let key = command_key(request.headers())?;
        command_content_type(request.headers())?;
        let selected = context(request.headers())?;
        let headers = request.headers().clone();
        let body = command_body(request).await?;
        let (kind, facts) = decode_command(body)?;
        target
            .backend
            .command(PmsImportHttpCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                context: selected,
                key,
                kind,
                facts,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
