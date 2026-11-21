//! Owner-local HTTP ingress for the native People workforce directory command.
//!
//! Transport facts only: exact registered Host/Holding target, exact browser
//! origin, one Idempotency-Key and bounded JSON. Account/IAM authority, package
//! admission, the serializable owner transaction and the Organization/Identity
//! ports remain composition responsibilities behind [`WorkforceHttpBackend`].
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type WorkforceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkforceHttpKind {
    Create,
    Update { person_id: String },
}

pub struct WorkforceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: WorkforceHttpKind,
    pub body: &'a serde_json::Value,
    pub key: &'a CommandKey,
}

#[derive(Clone, Debug)]
pub struct WorkforceHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl WorkforceHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait WorkforceHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: WorkforceHttpRequest<'a>,
    ) -> WorkforceHttpFuture<'a, WorkforceHttpResponse>;
}

pub struct WorkforceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn WorkforceHttpBackend>,
}

struct WorkforceHttpState {
    targets: BTreeMap<(String, String), WorkforceHttpTarget>,
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_WORKFORCE_REQUEST_INVALID",
        "Use one bounded People directory command.",
    )
}
fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered application origin.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_WORKFORCE_CONFIGURATION_INVALID",
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

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_origin(value: &str) -> bool {
    value.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host.bytes().any(|b| matches!(b, b'/' | b'?' | b'#' | b'@'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len()) && port.bytes().all(|b| b.is_ascii_digit())
            })
    })
}

async fn directory(
    State(state): State<Arc<WorkforceHttpState>>,
    Path((holding_slug, person_id)): Path<(String, Option<String>)>,
    request: Request,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let host = headers
            .get("host")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(not_found)?;
        let target = state
            .targets
            .get(&(host.to_owned(), holding_slug.clone()))
            .ok_or_else(not_found)?;
        if let Some(origin) = unique_header(&headers, HeaderName::from_static("origin"))?
            && origin != target.origin
        {
            return Err(forbidden());
        }
        let value: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
        if !value.is_object() {
            return Err(malformed());
        }
        let key = unique_header(&headers, KEY_HEADER)?.ok_or_else(malformed)?;
        let key = CommandKey::new(key.to_owned()).map_err(|_| malformed())?;
        let kind = match person_id {
            None => WorkforceHttpKind::Create,
            Some(person_id) if !person_id.is_empty() && person_id.len() <= 200 => {
                WorkforceHttpKind::Update { person_id }
            }
            Some(_) => return Err(malformed()),
        };
        target
            .backend
            .command(WorkforceHttpRequest {
                host,
                holding_slug: &holding_slug,
                headers: &headers,
                kind,
                body: &value,
                key: &key,
            })
            .await
            .map(|reply| reply.value())
    }
    .await;
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => {
            let status = match error.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (
                status,
                Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
            )
                .into_response()
        }
    }
}

/// Router exposing native People directory create/update commands.
pub fn workforce_router(targets: Vec<WorkforceHttpTarget>) -> Result<Router, BusinessError> {
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
            "/api/v1/holdings/{holding_slug}/people/directory",
            post(directory),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/directory/{person_id}",
            post(directory),
        )
        .with_state(Arc::new(WorkforceHttpState {
            targets: configured,
        })))
}
