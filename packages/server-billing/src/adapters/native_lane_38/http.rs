//! Owner-local axum read router for native lane 38. Host/Holding target
//! isolation, exact company-header scope and error mapping follow the Commerce
//! `quality_product_read` reference. No session, actor or Holding authority is
//! minted here; the caller's private transport supplies the bound context.
use super::application::{BillingWatermarkRead, read_watermark_body};
use super::domain::invalid;
use super::postgres::PgBillingWatermarkRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

type SharedWatermarkRead = Arc<dyn BillingWatermarkRead>;

/// Exact registered Holding target; the lead mounts one router per owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Retained name for callers written against the first lane-38 export.
pub type NativeLane38Target = LaneTarget;

struct Lane38State {
    targets: BTreeMap<(String, String), SharedWatermarkRead>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LANE_38_BILLING_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
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

/// Build the validated `(host, holding_slug) -> reader` state map. Each entry
/// carries its own reader, so distinct Holdings may use distinct pools.
fn build_state(
    entries: impl IntoIterator<Item = (LaneTarget, SharedWatermarkRead)>,
) -> Result<Arc<Lane38State>, BusinessError> {
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
    Ok(Arc::new(Lane38State {
        targets: configured,
    }))
}

fn build_router(state: Arc<Lane38State>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/billing/native-lane-38/watermarks",
            get(watermarks),
        )
        .with_state(state)
}

fn router_for_readers(
    targets: Vec<(LaneTarget, SharedWatermarkRead)>,
) -> Result<Router, BusinessError> {
    Ok(build_router(build_state(targets)?))
}

/// Build with one caller-supplied reader shared by every Holding target. Kept
/// for tests and for the single-pool contract.
pub fn router_with_reader(
    reader: SharedWatermarkRead,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_for_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Pool-backed variant matching the lane contract. The lead mounts this router.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgBillingWatermarkRead::new(pool)), targets)
}

/// Per-Holding constructor: one `PgBillingWatermarkRead` (and therefore one
/// database pool) per registered target. The state map stays keyed by
/// `(host, holding_slug)`, so a request can only reach its own Holding's pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_for_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: SharedWatermarkRead = Arc::new(PgBillingWatermarkRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn watermarks(
    State(state): State<Arc<Lane38State>>,
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
        let holding = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| super::domain::valid_holding(value))
            .ok_or_else(invalid)?;
        read_watermark_body(reader.as_ref(), holding).await
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
    use crate::adapters::native_lane_38::application::Lane38Future;
    use crate::adapters::native_lane_38::domain::WatermarkRow;

    struct Fake;
    impl BillingWatermarkRead for Fake {
        fn watermarks<'a>(&'a self, _holding: &'a str) -> Lane38Future<'a, Option<WatermarkRow>> {
            Box::pin(async { Ok(None) })
        }
    }

    fn target(host: &str, slug: &str) -> LaneTarget {
        LaneTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
        }
    }

    #[test]
    fn native_lane_38_targets_are_exact_and_distinct() {
        let configured = build_state(vec![(
            target("acme.jesadakorn.com", "acme"),
            Arc::new(Fake) as SharedWatermarkRead,
        )])
        .expect("one valid target");
        assert_eq!(configured.targets.len(), 1);
        assert!(router_with_reader(Arc::new(Fake), vec![]).is_err());
        assert!(router_with_reader(Arc::new(Fake), vec![target("", "acme")]).is_err());
        assert!(
            router_with_reader(Arc::new(Fake), vec![target("acme.jesadakorn.com", "Bad")]).is_err()
        );
        assert!(
            router_with_reader(
                Arc::new(Fake),
                vec![
                    target("acme.jesadakorn.com", "acme"),
                    target("acme.jesadakorn.com", "acme"),
                ],
            )
            .is_err()
        );
        // Distinct hosts with the same slug are separate targets.
        assert!(
            router_with_reader(
                Arc::new(Fake),
                vec![
                    target("a.example.com", "acme"),
                    target("b.example.com", "acme"),
                ],
            )
            .is_ok()
        );
    }

    #[test]
    fn native_lane_38_status_mapping_is_fail_closed() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(FailureKind::Unauthenticated),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Conflict), StatusCode::CONFLICT);
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            super::super::domain::unavailable().kind,
            FailureKind::Unavailable
        );
    }
}
