//! Axum HTTP ingress for the native customer-access sources read.
//!
//! The handler validates transport facts, delegates to the application port
//! and returns the complete owner payload. It never prepares or executes SQL
//! and never falls back to a TypeScript route.
use crate::application::access_sources::SourcesFuture;
use crate::domain::access::AccessSourceRow;
use crate::domain::access_sources::AccessSourcesQuery;
use erpx_server_runtime::public::http_support::{
    HeaderMap, IntoResponse, Json, Path, Query, Request, Response, Router, State, StatusCode, get,
    header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Native route mounted by composition for the customer-access family.
pub const CUSTOMER_ACCESS_SOURCES_PATH: &str =
    "/api/v1/holdings/{holding_slug}/customer-access/sources";

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AccessSourcesParams {
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
}

pub struct AccessSourcesRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: AccessSourcesQuery,
}

/// Composition-owned session, Company scope, purchase admission and owner-unit
/// adapter. The router never mints a session or a Company and accepts no SQL.
pub trait AccessSourcesBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: AccessSourcesRequest<'a>,
    ) -> SourcesFuture<'a, Vec<AccessSourceRow>>;
}

pub struct AccessSourcesTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub company_id: String,
    pub backend: Arc<dyn AccessSourcesBackend>,
}

struct AccessSourcesState {
    targets: BTreeMap<(String, String), AccessSourcesTarget>,
}

fn invalid_input() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_ACCESS_SOURCES_REQUEST_INVALID",
        "Use one bounded customer-access source request.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_ACCESS_SOURCES_CONFIG_INVALID",
        "Use exact distinct registered targets.",
    )
}

fn valid_origin(value: &str) -> bool {
    value.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|b| matches!(b, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}

fn unique<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_input());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_input()))
        .transpose()
}

fn target<'a>(
    state: &'a AccessSourcesState,
    headers: &'a HeaderMap,
    slug: &str,
) -> Result<(&'a AccessSourcesTarget, &'a str), BusinessError> {
    let host = unique(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let found = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    if unique(headers, "origin")?.is_some_and(|value| value != found.origin) {
        return Err(forbidden());
    }
    Ok((found, host))
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

/// Pin the complete four-field source payload. A partial projection or an
/// extra field fails closed instead of masquerading as parity.
fn valid_source(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 4
            && ["id", "number", "name", "version"]
                .iter()
                .all(|key| object.contains_key(*key))
            && object["id"].as_str().is_some_and(|id| !id.is_empty())
            && object["number"].as_str().is_some_and(|n| !n.is_empty())
            && object["name"].as_str().is_some_and(|n| !n.is_empty())
            && object["version"].as_i64().is_some_and(|v| v >= 1)
    })
}

fn success(rows: Vec<AccessSourceRow>) -> Response {
    let sources: Vec<Value> = rows.iter().map(AccessSourceRow::to_json).collect();
    if !sources.iter().all(valid_source) {
        return error_response(BusinessError::new(
            FailureKind::Unavailable,
            "CUSTOMER_ACCESS_SOURCES_UNAVAILABLE",
            "The approved customer document list is not available.",
        ));
    }
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({"data":{"sources":sources}})),
    )
        .into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, header::HeaderValue::from_static(value));
    }
    response
}

async fn read_sources(
    State(state): State<Arc<AccessSourcesState>>,
    Path(slug): Path<String>,
    Query(params): Query<AccessSourcesParams>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let (found, host) = target(&state, &headers, &slug)?;
        let query = AccessSourcesQuery {
            company_id: found.company_id.clone(),
            branch_id: params.branch,
            department_id: params.department,
        };
        found
            .backend
            .execute(AccessSourcesRequest {
                host,
                holding_slug: &found.holding_slug,
                headers: &headers,
                query,
            })
            .await
    }
    .await;
    match result {
        Ok(rows) => success(rows),
        Err(error) => error_response(error),
    }
}

/// Build the native customer-access sources router from exact registered
/// targets. Duplicate or malformed targets fail closed at construction.
pub fn customer_access_sources_router(
    targets: Vec<AccessSourcesTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || target.holding_slug.is_empty()
            || target.company_id.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(CUSTOMER_ACCESS_SOURCES_PATH, get(read_sources))
        .with_state(Arc::new(AccessSourcesState {
            targets: configured,
        })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FakeBackend;
    impl AccessSourcesBackend for FakeBackend {
        fn execute<'a>(
            &'a self,
            _: AccessSourcesRequest<'a>,
        ) -> SourcesFuture<'a, Vec<AccessSourceRow>> {
            Box::pin(async { Ok(vec![]) })
        }
    }

    fn target() -> AccessSourcesTarget {
        AccessSourcesTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            company_id: "company-1".into(),
            backend: Arc::new(FakeBackend),
        }
    }

    #[test]
    fn configuration_rejects_invalid_targets() {
        assert!(customer_access_sources_router(vec![target()]).is_ok());
        let mut bad = target();
        bad.company_id.clear();
        assert!(customer_access_sources_router(vec![bad]).is_err());
        let mut bad = target();
        bad.origin = "http://acme.example.test".into();
        assert!(customer_access_sources_router(vec![bad]).is_err());
        assert!(customer_access_sources_router(vec![target(), target()]).is_err());
    }

    #[test]
    fn payload_validation_requires_all_four_fields() {
        assert!(valid_source(&json!({"id":"a","number":"SO-1","name":"Acme","version":1})));
        assert!(!valid_source(&json!({"id":"a","number":"SO-1","version":1})));
        assert!(!valid_source(&json!({"id":"a","number":"SO-1","name":"Acme","version":0})));
    }

    #[test]
    fn origin_is_checked_against_the_exact_registered_value() {
        assert!(valid_origin("https://acme.example.test"));
        assert!(!valid_origin("http://acme.example.test"));
        assert!(!valid_origin("https://acme.example.test/path"));
    }
}
