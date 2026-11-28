//! Owner-local axum ingress for the Finance overview-counts and invoice reads.
//!
//! Composition supplies the sqlx pool and the exact registered Holding targets;
//! this router accepts no SQL and no caller predicate. Holding host, slug and
//! Company identity are isolated per request, and every failure maps to the
//! owner's explicit error envelope so the TypeScript client fails closed.

use crate::adapters::native_lane_2::{
    application::{
        InvoiceReadQuery, OverviewCountsQuery, load, load_aging, load_page,
    },
    domain::{
        AgingRequest, InvoicePageRequest, configuration_invalid, invalid_request, missing,
        unavailable, valid_company_id, valid_slug,
    },
    postgres::{PgInvoiceReadQuery, PgOverviewCountsQuery},
};
use erpx_platform_runtime_server::public::{
    http_support::{
        HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Response, Router,
        State, StatusCode, header, routing::get, serde_json,
    },
    postgres::DatabasePool,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// A registered Holding host/slug pair. The host header and the path slug must
/// both match an exact configured target; no suffix or fallback match is made.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    overview: BTreeMap<(String, String), Arc<dyn OverviewCountsQuery>>,
    invoices: BTreeMap<(String, String), Arc<dyn InvoiceReadQuery>>,
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
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
        return Err(invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_request()))
        .transpose()
}

fn valid_host(host: &str) -> bool {
    !host.is_empty() && host.len() <= 512 && !host.chars().any(|c| c.is_control())
}

/// Resolve the exact Host and Company headers. HeaderMap is a parts extractor
/// and is Sync, so the borrows stay valid across the owner read future.
fn request_scope(headers: &HeaderMap) -> Result<(&str, &str), BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| valid_host(host))
        .ok_or_else(missing)?;
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_company_id(value))
        .ok_or_else(invalid_request)?;
    Ok((host, company))
}

/// Validate the exact, distinct registered targets without touching the pool.
fn validate_targets(targets: &[LaneTarget]) -> Result<(), BusinessError> {
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for target in targets {
        if !valid_host(&target.host)
            || !valid_slug(&target.holding_slug)
            || !seen.insert((target.host.clone(), target.holding_slug.clone()))
        {
            return Err(configuration_invalid());
        }
    }
    Ok(())
}

/// Build the target map keyed by the exact `(host, holding_slug)` pair. Every
/// constructor below funnels through here, so the same isolation rules apply.
fn state_for_readers(
    overview: Vec<(LaneTarget, Arc<dyn OverviewCountsQuery>)>,
    invoices: Vec<(LaneTarget, Arc<dyn InvoiceReadQuery>)>,
) -> Result<Arc<LaneState>, BusinessError> {
    let targets: Vec<LaneTarget> = overview.iter().map(|(target, _)| target.clone()).collect();
    validate_targets(&targets)?;
    let mut overview_map: BTreeMap<(String, String), Arc<dyn OverviewCountsQuery>> = BTreeMap::new();
    for (target, reader) in overview {
        overview_map.insert((target.host, target.holding_slug), reader);
    }
    let mut invoice_map: BTreeMap<(String, String), Arc<dyn InvoiceReadQuery>> = BTreeMap::new();
    for (target, reader) in invoices {
        invoice_map.insert((target.host, target.holding_slug), reader);
    }
    Ok(Arc::new(LaneState { overview: overview_map, invoices: invoice_map }))
}

fn mounted(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/native-lane-2/overview-counts",
            get(counts),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/native-lane-2/invoices-page",
            get(invoices_page),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/native-lane-2/aging",
            get(aging),
        )
        .with_state(state)
}

/// Build the native read router with one reader shared by every target.
/// Retained for tests and single-pool callers.
pub fn router_with_reader(
    reader: Arc<dyn OverviewCountsQuery>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let state = state_for_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
        Vec::new(),
    )?;
    Ok(mounted(state))
}

/// Build the native read router from a single pool. The lead mounts it on the
/// private native API; no composition or public module is edited by the lane.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let overview: Arc<dyn OverviewCountsQuery> = Arc::new(PgOverviewCountsQuery::new(pool.clone()));
    let invoices: Arc<dyn InvoiceReadQuery> = Arc::new(PgInvoiceReadQuery::new(pool));
    let state = state_for_readers(
        targets.iter().cloned().map(|target| (target, overview.clone())).collect(),
        targets.into_iter().map(|target| (target, invoices.clone())).collect(),
    )?;
    Ok(mounted(state))
}

/// Build the native read router with one independent reader per Holding pool.
/// `pools[0]` is the control plane; entry `i` pairs `config.holdings[i]` with
/// `pools[i + 1]`. A request can only reach the reader registered for its exact
/// `(Host, holding_slug)` target.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut overview = Vec::with_capacity(targets.len());
    let mut invoices = Vec::with_capacity(targets.len());
    for (target, pool) in targets {
        let counts: Arc<dyn OverviewCountsQuery> =
            Arc::new(PgOverviewCountsQuery::new(pool.clone()));
        let invoice: Arc<dyn InvoiceReadQuery> = Arc::new(PgInvoiceReadQuery::new(pool));
        overview.push((target.clone(), counts));
        invoices.push((target, invoice));
    }
    let state = state_for_readers(overview, invoices)?;
    Ok(mounted(state))
}

async fn counts(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    // `HeaderMap` is a parts extractor and is Sync, unlike `&Request<Body>`;
    // borrowing it across the owner read keeps the handler future Send.
    let result = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| valid_host(host))
            .ok_or_else(missing)?;
        let reader = state
            .overview
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_company_id(value))
            .ok_or_else(invalid_request)?;
        let data = load(reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(json!({"data": data}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn invoices_page(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(request): Query<InvoicePageRequest>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let (host, company) = request_scope(&headers)?;
        let reader = state
            .invoices
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(unavailable)?;
        let data = load_page(reader.as_ref(), company, &request).await?;
        Ok::<_, BusinessError>(json!({"data": data}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn aging(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(request): Query<AgingRequest>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let (host, company) = request_scope(&headers)?;
        let reader = state
            .invoices
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(unavailable)?;
        let data = load_aging(reader.as_ref(), company, &request).await?;
        Ok::<_, BusinessError>(json!({"data": data}))
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
    use crate::adapters::native_lane_2::{
        application::{AgingFuture, InvoicePageFuture, InvoiceReadQuery, OverviewCountsFuture},
        domain::{AgingRequest, InvoicePageRequest, InvoicePageWire, OverviewCounts, unavailable},
    };
    use erpx_platform_runtime_server::public::http_support::body::to_bytes;

    struct FixedCounts(i64);
    impl OverviewCountsQuery for FixedCounts {
        fn counts<'a>(&'a self, company_id: &'a str) -> OverviewCountsFuture<'a> {
            let journals = self.0;
            let company_id = company_id.to_owned();
            Box::pin(async move {
                if company_id != "acme:co" {
                    return Err(unavailable());
                }
                Ok(OverviewCounts {
                    journals,
                    posting: 0,
                    invoices: 0,
                    payments: 0,
                    receivable: 0,
                    payable: 0,
                    reconcile: 0,
                })
            })
        }
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    async fn get(state: Arc<LaneState>, host: &'static str, slug: &'static str) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static(host));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("acme:co"));
        counts(State(state), Path(slug.to_owned()), headers).await
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let a: Arc<dyn OverviewCountsQuery> = Arc::new(FixedCounts(11));
        let b: Arc<dyn OverviewCountsQuery> = Arc::new(FixedCounts(22));
        assert!(!Arc::ptr_eq(&a, &b));

        let state = state_for_readers(vec![
            (
                LaneTarget { host: "a.example.com".into(), holding_slug: "acme".into() },
                a.clone(),
            ),
            (
                LaneTarget { host: "b.example.com".into(), holding_slug: "northstar".into() },
                b.clone(),
            ),
        ], Vec::new())
        .expect("two-target state");

        let from_a = body_json(get(state.clone(), "a.example.com", "acme").await).await;
        let from_b = body_json(get(state.clone(), "b.example.com", "northstar").await).await;
        assert_eq!(from_a["data"]["journals"], 11);
        assert_eq!(from_b["data"]["journals"], 22);

        // A valid host with the other Holding's slug must not reach its reader.
        assert_eq!(
            get(state.clone(), "b.example.com", "acme").await.status(),
            StatusCode::NOT_FOUND
        );
        // The single-reader constructor still builds a valid multi-target router.
        let shared = router_with_reader(
            a,
            vec![
                LaneTarget { host: "a.example.com".into(), holding_slug: "acme".into() },
                LaneTarget { host: "b.example.com".into(), holding_slug: "northstar".into() },
            ],
        );
        assert!(shared.is_ok());
    }

    struct FixedInvoice(i64);
    impl InvoiceReadQuery for FixedInvoice {
        fn page<'a>(
            &'a self,
            company_id: &'a str,
            _request: &'a InvoicePageRequest,
        ) -> InvoicePageFuture<'a> {
            let n = self.0;
            let company_id = company_id.to_owned();
            Box::pin(async move {
                if company_id != "acme:co" {
                    return Err(unavailable());
                }
                Ok(InvoicePageWire {
                    items: vec![serde_json::json!({"id": format!("i-{n}"), "page_key": "2026-01-01:INV"})],
                    next_cursor: None,
                    previous_cursor: None,
                    has_more: false,
                })
            })
        }
        fn aging<'a>(
            &'a self,
            company_id: &'a str,
            _request: &'a AgingRequest,
        ) -> AgingFuture<'a> {
            let n = self.0;
            let company_id = company_id.to_owned();
            Box::pin(async move {
                if company_id != "acme:co" {
                    return Err(unavailable());
                }
                Ok(vec![serde_json::json!({
                    "id": format!("a-{n}"),
                    "due_date": "2026-01-01",
                    "historical_outstanding_minor": 125,
                    "historical_base_outstanding_minor": 4500,
                    "outstanding_minor": 125,
                    "base_outstanding_minor": 4500,
                    "outstanding": "1.25",
                    "baseOutstanding": "45.00",
                    "bucket": "31-60",
                    "asOf": "2026-02-01",
                    "legacy_date_evidence": false,
                    "source_id": null
                })])
            })
        }
    }

    #[tokio::test]
    async fn invoice_routes_use_the_reader_registered_for_each_holding() {
        let a: Arc<dyn InvoiceReadQuery> = Arc::new(FixedInvoice(1));
        let b: Arc<dyn InvoiceReadQuery> = Arc::new(FixedInvoice(2));
        assert!(!Arc::ptr_eq(&a, &b));
        let state = state_for_readers(
            vec![
                (LaneTarget { host: "a.example.com".into(), holding_slug: "acme".into() }, Arc::new(FixedCounts(0)) as Arc<dyn OverviewCountsQuery>),
                (LaneTarget { host: "b.example.com".into(), holding_slug: "northstar".into() }, Arc::new(FixedCounts(0)) as Arc<dyn OverviewCountsQuery>),
            ],
            vec![
                (LaneTarget { host: "a.example.com".into(), holding_slug: "acme".into() }, a),
                (LaneTarget { host: "b.example.com".into(), holding_slug: "northstar".into() }, b),
            ],
        )
        .expect("invoice state");

        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("a.example.com"));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("acme:co"));
        let page_a = body_json(
            invoices_page(
                State(state.clone()),
                Path("acme".into()),
                Query(InvoicePageRequest::default()),
                headers.clone(),
            )
            .await,
        )
        .await;
        assert_eq!(page_a["data"]["items"][0]["id"], "i-1");

        let mut headers_b = HeaderMap::new();
        headers_b.insert(header::HOST, HeaderValue::from_static("b.example.com"));
        headers_b.insert(CONTEXT_COMPANY, HeaderValue::from_static("acme:co"));
        let age_b = body_json(
            aging(
                State(state),
                Path("northstar".into()),
                Query(AgingRequest { kind: "receivable".into(), as_of: "2026-02-01".into(), chart_id: None }),
                headers_b,
            )
            .await,
        )
        .await;
        let row = &age_b["data"][0];
        assert_eq!(row["id"], "a-2");
        // Full legacy aging payload parity: the Axum handler passes every
        // owner-derived field through, including the decimal presentation.
        for (field, expected) in [
            ("outstanding_minor", serde_json::json!(125)),
            ("base_outstanding_minor", serde_json::json!(4500)),
            ("outstanding", serde_json::json!("1.25")),
            ("baseOutstanding", serde_json::json!("45.00")),
            ("bucket", serde_json::json!("31-60")),
            ("asOf", serde_json::json!("2026-02-01")),
            ("legacy_date_evidence", serde_json::json!(false)),
            ("source_id", serde_json::Value::Null),
        ] {
            assert_eq!(&row[field], &expected, "missing aging field {field}");
        }
    }

    #[test]
    fn rejects_duplicate_and_empty_targets() {
        // A pool is not needed to reject invalid configuration.
        let invalid = [
            vec![],
            vec![LaneTarget { host: "h".into(), holding_slug: "A".into() }],
            vec![LaneTarget { host: "".into(), holding_slug: "a".into() }],
            vec![
                LaneTarget { host: "h".into(), holding_slug: "a".into() },
                LaneTarget { host: "h".into(), holding_slug: "a".into() },
            ],
        ];
        for targets in invalid {
            assert_eq!(validate_targets(&targets), Err(configuration_invalid()));
        }
        assert_eq!(
            validate_targets(&[LaneTarget {
                host: "acme.jesadakorn.com".into(),
                holding_slug: "acme".into(),
            }]),
            Ok(())
        );
    }

    #[test]
    fn maps_all_failure_kinds_to_the_owner_statuses() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unauthenticated), StatusCode::UNAUTHORIZED);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Conflict), StatusCode::CONFLICT);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn host_validation_is_bounded() {
        assert!(valid_host("acme.jesadakorn.com"));
        assert!(!valid_host(""));
        assert!(!valid_host("bad\nhost"));
        assert!(!valid_host(&"x".repeat(513)));
    }
}
