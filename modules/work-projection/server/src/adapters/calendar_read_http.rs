//! Private axum ingress for native Work calendar reads. Transport facts only:
//! an allow-listed read kind, bounded positional values, exact registered
//! Host/Holding/Origin and a bounded body. The backend opens the Holding
//! transaction and the owner read port executes the calendar SELECT; this
//! router never returns SQL and never falls back to TypeScript.
use crate::adapters::calendar_read::{
    CalendarReadCommand, execute_calendar_read, supported, validate,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 128 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalendarReadCommandInput {
    pub kind: String,
    pub values: Vec<Value>,
    #[serde(default)]
    pub company_id: String,
}

pub type CalendarReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, BusinessError>> + Send + 'a>>;

pub struct CalendarReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: &'a CalendarReadCommandInput,
}

pub trait CalendarReadHttpBackend: Send + Sync {
    fn read<'a>(&'a self, request: CalendarReadHttpRequest<'a>) -> CalendarReadFuture<'a>;
}

#[derive(Clone)]
pub struct CalendarReadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CalendarReadHttpBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), CalendarReadHttpTarget>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid("WORK_CALENDAR_READ_INVALID", "Review the calendar read request.")
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_WORK_UNAVAILABLE",
        "The native Work service is unavailable.",
    )
}
fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered application origin.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<String>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values
        .next()
        .map(|value| value.to_str().map(str::to_owned))
        .transpose()
        .map_err(|_| invalid())?;
    if values.next().is_some() {
        return Err(invalid());
    }
    Ok(first)
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
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
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
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CalendarReadHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let target = state
        .targets
        .get(&(host, slug.to_owned()))
        .ok_or_else(missing)?;
    match unique_header(headers, ORIGIN_HEADER)? {
        Some(origin) if origin == target.origin => Ok(()),
        _ => Err(forbidden()),
    }
    .map(|()| target)
}

async fn decode(request: Request) -> Result<CalendarReadCommandInput, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    let command: CalendarReadCommandInput = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if !supported(&command.kind)
        || validate(&command.kind, command.values.len()).is_err()
        || command.company_id.len() > 200
    {
        return Err(invalid());
    }
    Ok(command)
}

async fn read(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let command = decode(request).await?;
        if let Some(company) = unique_header(&headers, CONTEXT_COMPANY)?
            && company != command.company_id
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "WORK_CONTEXT_CHANGED",
                "The Company scope changed.",
            ));
        }
        let result = target
            .backend
            .read(CalendarReadHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: &command,
            })
            .await?;
        Ok(serde_json::json!({"data":{"kind":command.kind,"result":result}}))
    }
    .await;
    match result {
        Ok(data) => bounded(data),
        Err(error) => error_response(error),
    }
}

/// Ready-made composition backend: one exact Holding pool per route target,
/// one transaction per read, no SQL or authority decisions outside the owner.
pub struct PgCalendarReadBackend {
    holdings: BTreeMap<String, DatabasePool>,
}

impl PgCalendarReadBackend {
    pub fn new(holdings: impl IntoIterator<Item = (String, DatabasePool)>) -> Self {
        Self {
            holdings: holdings.into_iter().collect(),
        }
    }
}

impl CalendarReadHttpBackend for PgCalendarReadBackend {
    fn read<'a>(&'a self, request: CalendarReadHttpRequest<'a>) -> CalendarReadFuture<'a> {
        Box::pin(async move {
            let pool = self
                .holdings
                .get(request.holding_slug)
                .ok_or_else(unavailable)?
                .clone();
            let kind = request.command.kind.clone();
            let values = request.command.values.clone();
            let company = request.command.company_id.clone();
            PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let kind = kind.clone();
                    let values = values.clone();
                    let company = company.clone();
                    Box::pin(async move {
                        let command = CalendarReadCommand {
                            kind: &kind,
                            values: &values,
                            company_id: &company,
                        };
                        execute_calendar_read(&mut *connection, &command).await
                    })
                })
                .await
                .map_err(business)
        })
    }
}

pub fn calendar_read_router(
    targets: Vec<CalendarReadHttpTarget>,
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
            return Err(invalid());
        }
    }
    if configured.is_empty() {
        return Err(invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/work/owner-read",
            post(read),
        )
        .with_state(Arc::new(AppState { targets: configured })))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Noop;
    impl CalendarReadHttpBackend for Noop {
        fn read<'a>(&'a self, _: CalendarReadHttpRequest<'a>) -> CalendarReadFuture<'a> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn configuration_requires_distinct_exact_holdings_and_https_origins() {
        let backend = Arc::new(Noop) as Arc<dyn CalendarReadHttpBackend>;
        let target = |host: &str, slug: &str, origin: &str| CalendarReadHttpTarget {
            host: host.into(),
            holding_slug: slug.into(),
            origin: origin.into(),
            backend: backend.clone(),
        };
        assert!(calendar_read_router(vec![target(
            "acme.example.test",
            "acme",
            "https://acme.example.test"
        )])
        .is_ok());
        assert!(calendar_read_router(vec![target(
            "acme.example.test",
            "acme",
            "http://acme.example.test"
        )])
        .is_err());
        assert!(calendar_read_router(vec![
            target("a.example.test", "acme", "https://a.example.test"),
            target("a.example.test", "acme", "https://a.example.test"),
        ])
        .is_err());
        assert!(calendar_read_router(vec![]).is_err());
    }
}
