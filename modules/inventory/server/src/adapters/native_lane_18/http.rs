//! Owner-local axum router for the lane-18 inventory read.
//!
//! Host, slug and Company/branch headers are bounded exactly like the Quality
//! reference router: unknown targets and malformed headers fail closed, and a
//! reader failure is a 503 with no TypeScript SQL fallback.

use super::application::{invalid, read_stock_availability, unavailable};
use super::domain::{ROUTE, Scope};
use super::postgres::PgStockAvailabilityRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::StockAvailabilityRead;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");

pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn StockAvailabilityRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "INVENTORY_LANE18_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'+'))
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

/// Resolve the only accepted scope inputs. Each header must appear once and
/// carry a bounded identity; an empty branch means the whole Company.
fn scope_from_headers(headers: &HeaderMap) -> Result<Scope<'_>, BusinessError> {
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let branch = match unique_header(headers, CONTEXT_BRANCH)? {
        Some("") | None => None,
        Some(value) if valid_id(value) => Some(value),
        Some(_) => return Err(invalid()),
    };
    Ok(Scope { company, branch })
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

fn build_router(
    entries: Vec<(LaneTarget, Arc<dyn StockAvailabilityRead>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(ROUTE, get(current))
        .with_state(Arc::new(LaneState {
            targets: configured,
        })))
}

/// Build the router over one injected reader. The lead mounts it; this crate
/// does not edit composition.
pub fn stock_availability_router(
    reader: Arc<dyn StockAvailabilityRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    build_router(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Convenience constructor used by composition: build the owner sqlx reader
/// from the shared `DatabasePool` and register the exact Holding targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    stock_availability_router(Arc::new(PgStockAvailabilityRead::new(pool)), targets)
}

/// Per-Holding constructor: each registered `(host, holding_slug)` gets its own
/// sqlx reader over that Holding's pool, so a read can never cross a Holding
/// boundary. Duplicate targets or an empty set fail closed as configuration
/// errors, exactly like the other constructors.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    if targets.is_empty()
        || targets
            .iter()
            .any(|(target, _)| target.host.is_empty() || !valid_slug(&target.holding_slug))
    {
        return Err(configuration_invalid());
    }
    build_router(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn StockAvailabilityRead> =
                    Arc::new(PgStockAvailabilityRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let response =
            read_stock_availability(reader.as_ref(), scope_from_headers(request.headers())?)
                .await?;
        let body = serde_json::to_value(response).map_err(|_| unavailable())?;
        Ok::<_, BusinessError>(body)
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(company: &str, branch: Option<&str>) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(
            CONTEXT_COMPANY,
            HeaderValue::from_str(company).expect("company header"),
        );
        if let Some(branch) = branch {
            map.insert(
                CONTEXT_BRANCH,
                HeaderValue::from_str(branch).expect("branch header"),
            );
        }
        map
    }

    struct Reader;
    impl StockAvailabilityRead for Reader {
        fn rows<'a>(
            &'a self,
            _scope: Scope<'a>,
        ) -> super::super::application::StockAvailabilityFuture<
            'a,
            Vec<super::super::domain::StockAvailabilityRow>,
        > {
            Box::pin(async { Ok(vec![]) })
        }
    }

    #[test]
    fn router_config_is_exact_and_distinct() {
        let target = || LaneTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        };
        let reader: Arc<dyn StockAvailabilityRead> = Arc::new(Reader);
        assert!(stock_availability_router(reader.clone(), vec![target()]).is_ok());
        assert_eq!(
            stock_availability_router(reader.clone(), vec![])
                .unwrap_err()
                .code,
            "INVENTORY_LANE18_CONFIG_INVALID"
        );
        assert_eq!(
            stock_availability_router(reader.clone(), vec![target(), target()])
                .unwrap_err()
                .code,
            "INVENTORY_LANE18_CONFIG_INVALID"
        );
        assert_eq!(
            stock_availability_router(
                reader,
                vec![LaneTarget {
                    host: "acme.jesadakorn.com".into(),
                    holding_slug: "BAD".into()
                }]
            )
            .unwrap_err()
            .code,
            "INVENTORY_LANE18_CONFIG_INVALID"
        );
    }

    #[test]
    fn per_holding_router_rejects_an_empty_target_set() {
        assert_eq!(
            router_for_holdings(vec![]).unwrap_err().code,
            "INVENTORY_LANE18_CONFIG_INVALID"
        );
    }

    #[test]
    fn scope_headers_are_unique_and_bounded() {
        let with_branch = headers("company-1", Some("branch_2"));
        let scope = scope_from_headers(&with_branch).expect("valid");
        assert_eq!(scope.company, "company-1");
        assert_eq!(scope.branch, Some("branch_2"));
        let company_only = headers("company-1", None);
        let scope = scope_from_headers(&company_only).expect("valid");
        assert_eq!(scope.branch, None);
        assert_eq!(
            scope_from_headers(&headers("bad/../company", None))
                .unwrap_err()
                .code,
            "INVENTORY_LANE18_REQUEST_INVALID"
        );
        assert_eq!(
            scope_from_headers(&headers("company", Some("bad branch")))
                .unwrap_err()
                .code,
            "INVENTORY_LANE18_REQUEST_INVALID"
        );

        let mut duplicate = headers("company-1", None);
        duplicate.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert_eq!(
            scope_from_headers(&duplicate).unwrap_err().code,
            "INVENTORY_LANE18_REQUEST_INVALID"
        );
    }

    #[test]
    fn error_and_success_responses_are_bounded() {
        assert_eq!(
            error_response(unavailable()).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(error_response(missing()).status(), StatusCode::NOT_FOUND);
        assert_eq!(
            error_response(invalid()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let ok = bounded(serde_json::json!({"rows":[],"count":0}));
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.headers()[header::CACHE_CONTROL], "private, no-store");
        assert_eq!(ok.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }
}
