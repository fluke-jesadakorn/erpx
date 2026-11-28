//! Owner-local axum ingress for the lane-40 Marketing read family.
//!
//! Transport facts only: an exact registered Host/Holding target, a validated
//! current Company header and a closed body (`kind` plus bounded scope). No SQL
//! is accepted from the wire. The reader injected here owns the statement and
//! projection, and the application layer rechecks Company isolation on every
//! returned row. Failures map to explicit HTTP statuses with no TypeScript
//! fallback.
use crate::adapters::native_lane_40::application::{Lane40Reader, read};
use crate::adapters::native_lane_40::domain::{
    Lane40ReadInput, configuration_invalid, invalid, missing, valid_id,
};
use crate::adapters::native_lane_40::postgres::PgLane40Reader;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const BODY_LIMIT: usize = 8 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// A registered Holding origin for this owner-local router.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct AppState {
    targets: BTreeMap<(String, String), Arc<dyn Lane40Reader>>,
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

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn build_router(
    configured: BTreeMap<(String, String), Arc<dyn Lane40Reader>>,
) -> Result<Router, BusinessError> {
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/marketing/lane-40/read",
            post(read_lane_40),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
        })))
}

/// Build the owner router with one shared pool. The lead mounts it;
/// composition is not edited here.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let mut configured: BTreeMap<(String, String), Arc<dyn Lane40Reader>> = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgLane40Reader::new(pool.clone())) as Arc<dyn Lane40Reader>,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    build_router(configured)
}

/// Build the owner router with one reader per registered Holding pool. Each
/// `(host, holding_slug)` key owns its own `DatabasePool`, so a target is never
/// served from another Holding's pool. Existing constructors are unchanged.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured: BTreeMap<(String, String), Arc<dyn Lane40Reader>> = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgLane40Reader::new(pool)) as Arc<dyn Lane40Reader>,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    build_router(configured)
}

async fn read_lane_40(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let input: Lane40ReadInput = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let result = read(reader.as_ref(), &input, company).await?;
        Ok::<_, BusinessError>(serde_json::json!({"data": result}))
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
    use crate::adapters::native_lane_40::application::Lane40ReadFuture;
    use crate::adapters::native_lane_40::domain::{
        Lane40ReadResult, Lane40Request, RD_CANDIDATE_LIST, unavailable,
    };
    #[test]
    fn status_mapping_is_explicit() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn header_and_slug_rules_fail_closed() {
        let mut headers = HeaderMap::new();
        headers.append(header::HOST, HeaderValue::from_static("acme.example.test"));
        headers.append(header::HOST, HeaderValue::from_static("other.example.test"));
        assert!(unique_header(&headers, header::HOST).is_err());
        let mut headers = HeaderMap::new();
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        assert_eq!(
            unique_header(&headers, CONTEXT_COMPANY).unwrap(),
            Some("company-1")
        );
        assert!(valid_slug("acme-dev_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
    }

    struct Broken;
    impl Lane40Reader for Broken {
        fn read<'a>(&'a self, _request: Lane40Request) -> Lane40ReadFuture<'a, Lane40ReadResult> {
            Box::pin(async { Err(unavailable()) })
        }
    }

    #[test]
    fn holding_pool_router_rejects_an_empty_target_set() {
        // No pool is needed to prove the empty-set guard: no reader is built.
        assert_eq!(
            router_for_holdings(Vec::new()).unwrap_err().code,
            "MARKETING_LANE40_CONFIG_INVALID"
        );
    }

    #[tokio::test]
    async fn application_fail_closed_maps_to_503() {
        let input: Lane40ReadInput = serde_json::from_str(r#"{"kind":"candidate-list"}"#).unwrap();
        let error = read(&Broken, &input, "company-1").await.unwrap_err();
        assert_eq!(error.code, "MARKETING_LANE40_READ_UNAVAILABLE");
        assert_eq!(status(error.kind), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(RD_CANDIDATE_LIST, "candidate-list");
    }
}
