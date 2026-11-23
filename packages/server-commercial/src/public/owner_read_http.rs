//! Native Commercial owner-read axum ingress.
//!
//! Transport facts only: an exact registered Host/Holding target, browser
//! Origin, a closed read kind and bounded JSON. The registered kind selects a
//! static owner SELECT; no caller SQL is accepted and no TypeScript fallback
//! exists. The returned SELECT is executed by the caller on its own unit, so
//! caller filters and `FOR UPDATE` stay inside the caller's transaction.
use crate::domain::owner_read::{invalid, is_owner_read};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");

pub type OwnerReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct OwnerReadResponse {
    pub data: serde_json::Value,
}
impl OwnerReadResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

pub trait OwnerReadBackend: Send + Sync {
    fn read<'a>(&'a self, kind: &'a str) -> OwnerReadFuture<'a, OwnerReadResponse>;
}

pub struct OwnerReadTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn OwnerReadBackend>,
}
struct AppState {
    targets: BTreeMap<(String, String), OwnerReadTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    invalid()
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
        "COMMERCIAL_OWNER_READ_CONFIG_INVALID",
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
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|b| matches!(b, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|p| {
                (1..=5).contains(&p.len())
                    && p.bytes().all(|b| b.is_ascii_digit())
                    && p.parse::<u16>().is_ok_and(|v| v > 0)
            })
    })
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
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

pub fn commercial_owner_read_router(targets: Vec<OwnerReadTarget>) -> Result<Router, BusinessError> {
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
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commercial/owner-read",
            post(read),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
        })))
}

async fn read(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|v| !v.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let origin = unique_header(request.headers(), ORIGIN_HEADER)?.ok_or_else(forbidden)?;
        // fetch() forbids overriding Host, so native loopback POSTs arrive with a
        // loopback Host. Fall back to the validated Origin's host before giving
        // up; the exact Origin check below still applies to the selected target.
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .or_else(|| {
                origin
                    .strip_prefix("https://")
                    .and_then(|rest| match rest.rsplit_once(':') {
                        Some((h, _)) => Some(h),
                        None => Some(rest),
                    })
                    .and_then(|h| state.targets.get(&(h.to_owned(), slug.clone())))
            })
            .ok_or_else(missing)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        let kind = body
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(malformed)?;
        if !is_owner_read(kind) {
            return Err(malformed());
        }
        target.backend.read(kind).await
    }
    .await;
    match result {
        Ok(reply) => bounded(serde_json::json!({"data": reply.value()})),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::owner_read::{USAGE_METER, statement, unavailable};

    #[test]
    fn registry_and_configuration_guards() {
        struct Noop;
        impl OwnerReadBackend for Noop {
            fn read<'a>(
                &'a self,
                kind: &'a str,
            ) -> OwnerReadFuture<'a, OwnerReadResponse> {
                let sql = statement(kind).unwrap_or_default().to_owned();
                Box::pin(async move {
                    Ok(OwnerReadResponse {
                        data: serde_json::json!({"sql": sql}),
                    })
                })
            }
        }
        let backend = Arc::new(Noop);
        let target = || OwnerReadTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(commercial_owner_read_router(vec![target()]).is_ok());
        assert!(commercial_owner_read_router(vec![target(), target()]).is_err());
        assert!(is_owner_read(USAGE_METER));
        assert!(!is_owner_read("bogus"));
        assert!(!unavailable().code.is_empty());
    }
}
