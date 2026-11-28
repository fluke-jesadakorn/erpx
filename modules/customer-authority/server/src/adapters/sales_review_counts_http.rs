//! Axum HTTP ingress for the native sales customer review counts read.
//!
//! The handler validates transport facts, delegates to the application port
//! and returns the complete `{kind,count}` payload. It never prepares or
//! executes SQL and never falls back to a TypeScript route.
use crate::application::sales_review_counts::ReviewCountsFuture;
use crate::domain::sales_review_counts::{REVIEW_KINDS, ReviewCount, SalesReviewCountsQuery};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, IntoResponse, Json, Path, Query, Request, Response, Router, State, StatusCode, get,
    header, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Native route mounted by composition for the sales review-count family.
pub const CUSTOMER_SALES_REVIEW_COUNTS_PATH: &str =
    "/api/v1/holdings/{holding_slug}/customer-reviews/counts";

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SalesReviewCountsParams {
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
}

pub struct SalesReviewCountsRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: SalesReviewCountsQuery,
}

/// Composition-owned session, Company scope and owner-unit adapter. The router
/// never mints a session or a Company and accepts no SQL.
pub trait SalesReviewCountsBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: SalesReviewCountsRequest<'a>,
    ) -> ReviewCountsFuture<'a, Vec<ReviewCount>>;
}

pub struct SalesReviewCountsTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub company_id: String,
    pub backend: Arc<dyn SalesReviewCountsBackend>,
}

struct SalesReviewCountsState {
    targets: BTreeMap<(String, String), SalesReviewCountsTarget>,
}

fn invalid_input() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_SALES_REVIEW_COUNTS_REQUEST_INVALID",
        "Use one bounded sales customer review count request.",
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
        "CUSTOMER_SALES_REVIEW_COUNTS_CONFIG_INVALID",
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
    state: &'a SalesReviewCountsState,
    headers: &'a HeaderMap,
    slug: &str,
) -> Result<(&'a SalesReviewCountsTarget, &'a str), BusinessError> {
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

/// Pin one `{kind,count}` fact. An extra or missing field, an open kind or a
/// negative count fails closed instead of masquerading as parity.
fn valid_count(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 2
            && object.contains_key("kind")
            && object.contains_key("count")
            && value["kind"]
                .as_str()
                .is_some_and(|kind| REVIEW_KINDS.contains(&kind))
            && value["count"].as_i64().is_some_and(|count| count >= 0)
    })
}

fn success(rows: Vec<ReviewCount>) -> Response {
    let reviews: Vec<Value> = rows.iter().map(ReviewCount::to_json).collect();
    if !reviews.iter().all(valid_count) {
        return error_response(BusinessError::new(
            FailureKind::Unavailable,
            "CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE",
            "The sales customer review counts are not available.",
        ));
    }
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({"data":{"reviews":reviews}})),
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

async fn read_counts(
    State(state): State<Arc<SalesReviewCountsState>>,
    Path(slug): Path<String>,
    Query(params): Query<SalesReviewCountsParams>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let (found, host) = target(&state, &headers, &slug)?;
        let query = SalesReviewCountsQuery {
            company_id: found.company_id.clone(),
            branch_id: params.branch,
            department_id: params.department,
        };
        found
            .backend
            .execute(SalesReviewCountsRequest {
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

/// Build the native sales review-count router from exact registered targets.
/// Duplicate or malformed targets fail closed at construction.
pub fn sales_review_counts_router(
    targets: Vec<SalesReviewCountsTarget>,
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
        .route(CUSTOMER_SALES_REVIEW_COUNTS_PATH, get(read_counts))
        .with_state(Arc::new(SalesReviewCountsState {
            targets: configured,
        })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FakeBackend;
    impl SalesReviewCountsBackend for FakeBackend {
        fn execute<'a>(
            &'a self,
            _: SalesReviewCountsRequest<'a>,
        ) -> ReviewCountsFuture<'a, Vec<ReviewCount>> {
            Box::pin(async { Ok(vec![]) })
        }
    }

    fn target() -> SalesReviewCountsTarget {
        SalesReviewCountsTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            company_id: "company-1".into(),
            backend: Arc::new(FakeBackend),
        }
    }

    fn count(kind: &str, count: i64) -> Value {
        json!({"kind": kind, "count": count})
    }

    #[test]
    fn configuration_rejects_invalid_targets() {
        assert!(sales_review_counts_router(vec![target()]).is_ok());
        let mut bad = target();
        bad.company_id.clear();
        assert!(sales_review_counts_router(vec![bad]).is_err());
        let mut bad = target();
        bad.origin = "http://acme.example.test".into();
        assert!(sales_review_counts_router(vec![bad]).is_err());
        assert!(sales_review_counts_router(vec![target(), target()]).is_err());
    }

    #[test]
    fn payload_validation_requires_exact_two_field_facts() {
        assert!(valid_count(&count("identity", 3)));
        assert!(!valid_count(&count("unknown", 3)));
        assert!(!valid_count(&count("identity", -1)));
        assert!(!valid_count(&json!({"kind": "identity"})));
        assert!(!valid_count(&json!({"kind": "identity", "count": 1, "extra": true})));
    }

    #[test]
    fn origin_is_checked_against_the_exact_registered_value() {
        assert!(valid_origin("https://acme.example.test"));
        assert!(!valid_origin("http://acme.example.test"));
        assert!(!valid_origin("https://acme.example.test/path"));
    }
}
