//! Single axum command router for all native Work writes. Transport facts only:
//! typed allow-listed kind, caller-owned positional values, exact registered
//! Host/Holding/Origin, one Idempotency-Key and a bounded body. The backend
//! opens the transaction and the owner adapter EXECUTES the write; this router
//! never returns SQL and never falls back to TypeScript.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 192 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerWriteCommandInput {
    pub kind: String,
    pub values: Vec<Value>,
    #[serde(default)]
    pub company_id: String,
    #[serde(default)]
    pub now: i64,
}

pub type OwnerWriteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, BusinessError>> + Send + 'a>>;

pub struct OwnerWriteHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: &'a OwnerWriteCommandInput,
    pub key: &'a str,
    pub control: bool,
}

pub trait OwnerWriteHttpBackend: Send + Sync {
    fn execute<'a>(&'a self, request: OwnerWriteHttpRequest<'a>) -> OwnerWriteFuture<'a>;
}

#[derive(Clone)]
pub struct OwnerWriteHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub control: bool,
    pub backend: Arc<dyn OwnerWriteHttpBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), OwnerWriteHttpTarget>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid("WORK_OWNER_WRITE_INVALID", "Review the owner write request.")
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered application origin.",
    )
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
) -> Result<&'a OwnerWriteHttpTarget, BusinessError> {
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

pub fn owner_write_router(targets: Vec<OwnerWriteHttpTarget>) -> Result<Router, BusinessError> {
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
            "/api/v1/holdings/{holding_slug}/work/owner-write",
            post(write),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/work/owner-write-control",
            post(write),
        )
        .with_state(Arc::new(AppState { targets: configured })))
}

async fn decode(request: Request) -> Result<OwnerWriteCommandInput, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    let command: OwnerWriteCommandInput = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if command.kind.is_empty()
        || command.kind.len() > 120
        || command.values.is_empty()
        || command.values.len() > 64
        || command.company_id.len() > 200
    {
        return Err(invalid());
    }
    Ok(command)
}

async fn write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let key = unique_header(&headers, KEY_HEADER)?
            .and_then(|value| CommandKey::new(&value).ok())
            .ok_or_else(invalid)?;
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
            .execute(OwnerWriteHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: &command,
                key: key.as_str(),
                control: target.control,
            })
            .await?;
        Ok(serde_json::json!({"data":{"ok":true,"kind":command.kind,"result":result}}))
    }
    .await;
    match result {
        Ok(data) => bounded(data),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Noop;
    impl OwnerWriteHttpBackend for Noop {
        fn execute<'a>(&'a self, _: OwnerWriteHttpRequest<'a>) -> OwnerWriteFuture<'a> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn configuration_requires_distinct_exact_holdings_and_https_origins() {
        let backend = Arc::new(Noop);
        let target = || OwnerWriteHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            control: false,
            backend: backend.clone(),
        };
        assert!(owner_write_router(vec![target()]).is_ok());
        assert!(owner_write_router(vec![target(), target()]).is_err());
        assert!(owner_write_router(vec![OwnerWriteHttpTarget {
            origin: "http://forged.example.test".into(),
            ..target()
        }])
        .is_err());
    }

    #[test]
    fn decode_rejects_unknown_fields() {
        let raw = serde_json::json!({"kind":"k","values":[1],"companyId":"c","now":1,"extra":1});
        assert!(serde_json::from_value::<OwnerWriteCommandInput>(raw).is_err());
    }
}
