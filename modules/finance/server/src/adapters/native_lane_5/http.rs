//! Owner-local axum read router for the Finance customer master family.
//! Holding/host, company and branch are isolated exactly like the other native
//! owner routers; all query execution and projection stays in the Rust adapter.

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::{
    CustomerRead, RecordFactsRead, handle, handle_record_facts,
};
use super::domain::{
    self, CustomerReadRequest, LaneTarget, MAX_BODY_BYTES, RecordFactsRequest,
};
use super::postgres::{PgCustomerRead, PgRecordFactsRead};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");

struct CustomerReadState {
    targets: BTreeMap<(String, String), Arc<dyn CustomerRead>>,
}

struct RecordFactsState {
    targets: BTreeMap<(String, String), Arc<dyn RecordFactsRead>>,
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

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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
        return Err(domain::invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| domain::invalid()))
        .transpose()
}

/// Validate registered targets and build the shared routing state. Every
/// target owns one reader instance; an exact `(host, holding_slug)` pair always
/// resolves to that Holding's reader and never to another Holding's.
fn state_from<I>(entries: I) -> Result<Arc<CustomerReadState>, BusinessError>
where
    I: IntoIterator<Item = (LaneTarget, Arc<dyn CustomerRead>)>,
{
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !domain::valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(domain::configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(domain::configuration_invalid());
    }
    Ok(Arc::new(CustomerReadState {
        targets: configured,
    }))
}

fn router_from(state: Arc<CustomerReadState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/customer-read",
            post(read),
        )
        .with_state(state)
}

/// Build the native customer read router with one shared reader, retained for
/// tests and injectable owners. No TypeScript SQL or business rule is added.
pub fn customer_read_router(
    reader: Arc<dyn CustomerRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let state = state_from(targets.into_iter().map(|target| (target, reader.clone())))?;
    Ok(router_from(state))
}

/// Build the native customer read router with one owner reader per Holding
/// pool. Composition pairs `config.holdings[i]` with `pools[i + 1]` (`pools[0]`
/// is the control plane); each reader binds only its own Holding database.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = state_from(targets.into_iter().map(|(target, pool)| {
        let reader: Arc<dyn CustomerRead> = Arc::new(PgCustomerRead::new(pool));
        (target, reader)
    }))?;
    Ok(router_from(state))
}

async fn read(
    State(state): State<Arc<CustomerReadState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(domain::missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(domain::missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|company| domain::valid_company(company))
            .ok_or_else(domain::invalid)?
            .to_owned();
        let branch = unique_header(request.headers(), CONTEXT_BRANCH)?
            .filter(|branch| domain::valid_branch(branch))
            .unwrap_or("")
            .to_owned();
        let bytes = to_bytes(request.into_body(), MAX_BODY_BYTES)
            .await
            .map_err(|_| domain::invalid())?;
        let payload: CustomerReadRequest =
            serde_json::from_slice(&bytes).map_err(|_| domain::invalid())?;
        let value = handle(reader.as_ref(), &company, &branch, &payload).await?;
        Ok::<_, BusinessError>(serde_json::json!({"kind":payload.kind,"value":value}))
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

/// Build the record-supplement read router with one owner reader per Holding
/// pool. Each reader is bound to its own Holding database via the exact
/// `(Host, holding_slug)` target.
fn record_facts_state_from<I>(entries: I) -> Result<Arc<RecordFactsState>, BusinessError>
where
    I: IntoIterator<Item = (LaneTarget, Arc<dyn RecordFactsRead>)>,
{
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !domain::valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(domain::configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(domain::configuration_invalid());
    }
    Ok(Arc::new(RecordFactsState {
        targets: configured,
    }))
}

pub fn record_facts_router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = record_facts_state_from(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgRecordFactsRead::new(pool)) as Arc<dyn RecordFactsRead>,
        )
    }))?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/record-facts",
            post(read_record_facts),
        )
        .with_state(state))
}

async fn read_record_facts(
    State(state): State<Arc<RecordFactsState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(domain::missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(domain::missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|company| domain::valid_company(company))
            .ok_or_else(domain::invalid)?
            .to_owned();
        let bytes = to_bytes(request.into_body(), MAX_BODY_BYTES)
            .await
            .map_err(|_| domain::invalid())?;
        let payload: RecordFactsRequest =
            serde_json::from_slice(&bytes).map_err(|_| domain::invalid())?;
        let value = handle_record_facts(reader.as_ref(), &company, &payload).await?;
        Ok::<_, BusinessError>(serde_json::json!({"kind":payload.kind,"value":value}))
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopReader;

    impl CustomerRead for NoopReader {
        fn read<'a>(
            &'a self,
            _company: &'a str,
            _branch: &'a str,
            _request: &'a CustomerReadRequest,
        ) -> super::super::application::CustomerReadFuture<'a, serde_json::Value> {
            Box::pin(async { Ok(serde_json::json!([])) })
        }
    }

    fn target(host: &str, slug: &str) -> LaneTarget {
        LaneTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
        }
    }

    #[test]
    fn native_lane_5_router_requires_distinct_registered_targets() {
        let reader: Arc<dyn CustomerRead> = Arc::new(NoopReader);
        assert!(customer_read_router(reader.clone(), vec![]).is_err());
        assert!(customer_read_router(
            reader.clone(),
            vec![target("acme.example.com", "acme")]
        )
        .is_ok());
        assert!(customer_read_router(
            reader.clone(),
            vec![
                target("acme.example.com", "acme"),
                target("acme.example.com", "acme"),
            ]
        )
        .is_err());
        assert!(customer_read_router(reader, vec![target("", "acme")]).is_err());
    }

    #[test]
    fn native_lane_5_two_targets_route_to_two_distinct_readers() {
        let acme: Arc<dyn CustomerRead> = Arc::new(NoopReader);
        let northstar: Arc<dyn CustomerRead> = Arc::new(NoopReader);
        assert!(!Arc::ptr_eq(&acme, &northstar));
        let state = state_from([
            (target("acme.example.com", "acme"), acme.clone()),
            (
                target("northstar.example.com", "northstar"),
                northstar.clone(),
            ),
        ])
        .unwrap();
        let routed_acme = state
            .targets
            .get(&("acme.example.com".to_owned(), "acme".to_owned()))
            .unwrap();
        let routed_northstar = state
            .targets
            .get(&("northstar.example.com".to_owned(), "northstar".to_owned()))
            .unwrap();
        assert!(Arc::ptr_eq(routed_acme, &acme));
        assert!(Arc::ptr_eq(routed_northstar, &northstar));
        assert!(!Arc::ptr_eq(routed_acme, routed_northstar));
    }

    struct FactsReader;

    impl RecordFactsRead for FactsReader {
        fn read<'a>(
            &'a self,
            _company: &'a str,
            _request: &'a RecordFactsRequest,
        ) -> super::super::application::CustomerReadFuture<'a, serde_json::Value> {
            Box::pin(async { Ok(serde_json::json!(null)) })
        }
    }

    #[test]
    fn native_lane_5_record_facts_routes_two_holdings_to_two_readers() {
        let acme: Arc<dyn RecordFactsRead> = Arc::new(FactsReader);
        let northstar: Arc<dyn RecordFactsRead> = Arc::new(FactsReader);
        assert!(!Arc::ptr_eq(&acme, &northstar));
        assert!(record_facts_state_from(Vec::new()).is_err());
        let state = record_facts_state_from([
            (target("acme.example.com", "acme"), acme.clone()),
            (target("northstar.example.com", "northstar"), northstar.clone()),
        ])
        .unwrap();
        let routed_acme = state
            .targets
            .get(&("acme.example.com".to_owned(), "acme".to_owned()))
            .unwrap();
        let routed_northstar = state
            .targets
            .get(&("northstar.example.com".to_owned(), "northstar".to_owned()))
            .unwrap();
        assert!(Arc::ptr_eq(routed_acme, &acme));
        assert!(Arc::ptr_eq(routed_northstar, &northstar));
        assert!(!Arc::ptr_eq(routed_acme, routed_northstar));
        assert!(record_facts_state_from([
            (target("acme.example.com", "acme"), acme),
            (target("acme.example.com", "acme"), northstar),
        ])
        .is_err());
        let invalid: Arc<dyn RecordFactsRead> = Arc::new(FactsReader);
        assert!(record_facts_state_from([(target("", "acme"), invalid)]).is_err());
    }

    #[test]
    fn native_lane_5_error_mapping_preserves_failure_kind() {
        let response = error_response(domain::unavailable());
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let response = error_response(domain::invalid());
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = error_response(domain::missing());
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn native_lane_5_bounded_response_sets_private_headers() {
        let response = bounded(serde_json::json!({"value": []}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(
            response.headers().get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
    }
}
