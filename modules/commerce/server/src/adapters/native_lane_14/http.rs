//! Owner-local axum read router for the lane 14 Commerce workflow order reads.
//!
//! The router owns the Holding target allow-list, header and Company isolation,
//! and error mapping. The Pg reader executes the owner SQL; TypeScript only
//! transports the bounded facts and fails closed when this route is absent.

use super::application::{
    CommerceLane14Read, CommerceLane14SalesRead, read_document, read_open_orders,
    read_order_counts, read_page, read_pricing_product, read_pricing_products, read_references,
    read_support_source,
};
use super::domain::{SalesScope, sales_invalid, valid_id};
use super::postgres::{PgCommerceLane14Read, PgCommerceLane14SalesRead};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body, header, routing::get, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_RECORD: HeaderName = HeaderName::from_static("x-erpx-context-record");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");

/// Exact registered Holding targets. Composition supplies these; the router
/// never infers a Holding from a caller-supplied header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn CommerceLane14Read>>,
    sales_targets: BTreeMap<(String, String), Arc<dyn CommerceLane14SalesRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_LANE14_READ_REQUEST_INVALID",
        "Use one bounded Commerce workflow order read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_LANE14_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
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

fn target<'a>(
    state: &'a LaneState,
    slug: &str,
    request: &Request,
) -> Result<&'a Arc<dyn CommerceLane14Read>, BusinessError> {
    let host = unique_header(request.headers(), header::HOST)?
        .filter(|host| !host.is_empty() && host.len() <= 253)
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn scope(request: &Request) -> Result<(&str, &str), BusinessError> {
    let company = unique_header(request.headers(), CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let record = unique_header(request.headers(), CONTEXT_RECORD)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    Ok((company, record))
}

/// Build the native read router with one shared pool. Both read families are
/// registered, so no route silently degrades when the lead mounts this router.
/// The lead mounts it; no composition or public module is edited inside this lane.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let mut workflow = Vec::with_capacity(targets.len());
    let mut sales = Vec::with_capacity(targets.len());
    for target in targets {
        workflow.push((
            target.clone(),
            Arc::new(PgCommerceLane14Read::new(pool.clone())) as Arc<dyn CommerceLane14Read>,
        ));
        sales.push((
            target,
            Arc::new(PgCommerceLane14SalesRead::new(pool.clone())) as Arc<dyn CommerceLane14SalesRead>,
        ));
    }
    router_with_readers(workflow, sales)
}

/// Per-Holding constructor: each `(target, pool)` pair gets its own Pg reader
/// for both read families, so a request can only reach its Holding's pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut workflow = Vec::with_capacity(targets.len());
    let mut sales = Vec::with_capacity(targets.len());
    for (target, pool) in targets {
        workflow.push((
            target.clone(),
            Arc::new(PgCommerceLane14Read::new(pool.clone())) as Arc<dyn CommerceLane14Read>,
        ));
        sales.push((
            target,
            Arc::new(PgCommerceLane14SalesRead::new(pool)) as Arc<dyn CommerceLane14SalesRead>,
        ));
    }
    router_with_readers(workflow, sales)
}

/// Reader-injected constructor used by the owner workflow unit tests.
pub fn router_with_reader(
    reader: Arc<dyn CommerceLane14Read>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
        Vec::new(),
    )
}

fn insert_targets<T>(
    entries: Vec<(LaneTarget, T)>,
    map: &mut BTreeMap<(String, String), T>,
) -> Result<(), BusinessError> {
    for (target, reader) in entries {
        if target.host.is_empty()
            || target.host.len() > 253
            || !valid_slug(&target.holding_slug)
            || map
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(())
}

fn build_state_with_sales(
    targets: Vec<(LaneTarget, Arc<dyn CommerceLane14Read>)>,
    sales_targets: Vec<(LaneTarget, Arc<dyn CommerceLane14SalesRead>)>,
) -> Result<Arc<LaneState>, BusinessError> {
    let mut configured = BTreeMap::new();
    insert_targets(targets, &mut configured)?;
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    let mut sales = BTreeMap::new();
    insert_targets(sales_targets, &mut sales)?;
    Ok(Arc::new(LaneState {
        targets: configured,
        sales_targets: sales,
    }))
}

#[cfg(test)]
fn build_state(
    targets: Vec<(LaneTarget, Arc<dyn CommerceLane14Read>)>,
) -> Result<Arc<LaneState>, BusinessError> {
    build_state_with_sales(targets, Vec::new())
}

fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn CommerceLane14Read>)>,
    sales_targets: Vec<(LaneTarget, Arc<dyn CommerceLane14SalesRead>)>,
) -> Result<Router, BusinessError> {
    Ok(assemble_router(build_state_with_sales(
        targets,
        sales_targets,
    )?))
}

fn assemble_router(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/workflow-document",
            get(document),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/workflow-document-page",
            get(document_page),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/workflow-reference-rows",
            get(reference_rows),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/sales-pricing-products",
            get(sales_pricing_products),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/sales-pricing-product",
            get(sales_pricing_product),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/sales-order-counts",
            post(sales_order_counts),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/sales-customer-open-orders",
            get(sales_open_orders),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/lane14/sales-support-order-source",
            get(sales_support_source),
        )
        .with_state(state)
}

async fn document(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = target(&state, &slug, &request)?;
        let (company, id) = scope(&request)?;
        let row = read_document(reader.as_ref(), company, id).await?;
        Ok::<_, BusinessError>(serde_json::json!({"row": row}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn document_page(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = target(&state, &slug, &request)?;
        let (company, after) = scope(&request)?;
        let rows = read_page(reader.as_ref(), company, after).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn reference_rows(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = target(&state, &slug, &request)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let rows = read_references(reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

fn sales_target<'a>(
    state: &'a LaneState,
    slug: &str,
    request: &Request,
) -> Result<&'a Arc<dyn CommerceLane14SalesRead>, BusinessError> {
    let host = unique_header(request.headers(), header::HOST)?
        .filter(|host| !host.is_empty() && host.len() <= 253)
        .ok_or_else(missing)?;
    state
        .sales_targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn sales_scope(request: &Request) -> Result<SalesScope, BusinessError> {
    let company = unique_header(request.headers(), CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(sales_invalid)?
        .to_owned();
    let branch = unique_header(request.headers(), CONTEXT_BRANCH)?
        .filter(|value| valid_id(value))
        .map(str::to_owned);
    let department = unique_header(request.headers(), CONTEXT_DEPARTMENT)?
        .filter(|value| valid_id(value))
        .map(str::to_owned);
    Ok(SalesScope {
        company,
        branch,
        department,
    })
}

fn sales_record(request: &Request) -> Result<&str, BusinessError> {
    unique_header(request.headers(), CONTEXT_RECORD)?
        .filter(|value| valid_id(value))
        .ok_or_else(sales_invalid)
}

async fn sales_pricing_products(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = sales_target(&state, &slug, &request)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(sales_invalid)?;
        let rows = read_pricing_products(reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn sales_pricing_product(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = sales_target(&state, &slug, &request)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(sales_invalid)?;
        let id = sales_record(&request)?;
        let row = read_pricing_product(reader.as_ref(), company, id).await?;
        Ok::<_, BusinessError>(serde_json::json!({"row": row}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn sales_order_counts(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = sales_target(&state, &slug, &request)?;
        let scope = sales_scope(&request)?;
        let bytes = body::to_bytes(request.into_body(), 16 * 1024)
            .await
            .map_err(|_| sales_invalid())?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| sales_invalid())?;
        let values = value
            .get("partyIds")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(sales_invalid)?;
        if values.is_empty() || values.len() > 64 {
            return Err(sales_invalid());
        }
        let ids = values
            .iter()
            .map(|value| value.as_str().filter(|id| valid_id(id)).map(str::to_owned))
            .collect::<Option<Vec<String>>>()
            .ok_or_else(sales_invalid)?;
        let rows = read_order_counts(reader.as_ref(), &scope, &ids).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn sales_open_orders(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = sales_target(&state, &slug, &request)?;
        let scope = sales_scope(&request)?;
        let party = sales_record(&request)?;
        let rows = read_open_orders(reader.as_ref(), &scope, party).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn sales_support_source(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = sales_target(&state, &slug, &request)?;
        let scope = sales_scope(&request)?;
        let party = sales_record(&request)?;
        let row = read_support_source(reader.as_ref(), &scope, party).await?;
        Ok::<_, BusinessError>(serde_json::json!({"row": row}))
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
    use crate::adapters::native_lane_14::application::LaneReadFuture;
    use crate::adapters::native_lane_14::domain::{COLUMNS, ColumnKind};
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use erpx_platform_runtime_server::public::http_support::serde_json::{self, Value};
    use std::sync::Mutex;

    fn good_row() -> Value {
        good_row_with("order-1")
    }

    fn good_row_with(id: &str) -> Value {
        let mut object = serde_json::Map::new();
        for (index, column) in COLUMNS.iter().enumerate() {
            let value = match column.kind {
                ColumnKind::Text => Value::String(format!("{}-{}", column.name, index)),
                ColumnKind::Int => Value::from(index as i64),
            };
            object.insert(column.name.to_owned(), value);
        }
        object.insert("id".to_owned(), Value::String(id.to_owned()));
        Value::Object(object)
    }

    #[derive(Clone)]
    struct Probe {
        row: Option<Value>,
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl CommerceLane14Read for Probe {
        fn document<'a>(
            &'a self,
            company: &'a str,
            id: &'a str,
        ) -> LaneReadFuture<'a, Option<Value>> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            let row = self.row.clone();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, id));
                Ok(row)
            })
        }
        fn page<'a>(
            &'a self,
            _company: &'a str,
            _after: &'a str,
        ) -> LaneReadFuture<'a, Vec<Value>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn references<'a>(&'a self, _company: &'a str) -> LaneReadFuture<'a, Vec<Value>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    fn state(row: Option<Value>) -> (Arc<LaneState>, Arc<Mutex<Vec<(String, String)>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Probe {
            row,
            seen: seen.clone(),
        };
        let mut targets = BTreeMap::new();
        targets.insert(
            ("acme.jesadakorn.com".to_owned(), "acme".to_owned()),
            Arc::new(reader) as Arc<dyn CommerceLane14Read>,
        );
        (
            Arc::new(LaneState {
                targets,
                sales_targets: BTreeMap::new(),
            }),
            seen,
        )
    }

    fn request(host: &str, company: &str, record: &str, extra_company: bool) -> Request {
        let mut builder = Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, company)
            .header(CONTEXT_RECORD, record);
        if extra_company {
            builder = builder.header(CONTEXT_COMPANY, "co-2");
        }
        builder.body(Body::empty()).expect("request")
    }

    async fn body(response: Response) -> (StatusCode, Value) {
        let status = response.status();
        let bytes = erpx_platform_runtime_server::public::http_support::body::to_bytes(
            response.into_body(),
            1024 * 1024,
        )
        .await
        .expect("body");
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn router_rejects_empty_and_duplicate_targets() {
        let reader: Arc<dyn CommerceLane14Read> = Arc::new(Probe {
            row: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(router_with_reader(reader.clone(), Vec::new()).is_err());
        let duplicate = vec![
            LaneTarget {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
            },
            LaneTarget {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
            },
        ];
        assert!(router_with_reader(reader.clone(), duplicate).is_err());
        let valid = vec![LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        }];
        assert!(router_with_reader(reader, valid).is_ok());
    }

    #[tokio::test]
    async fn handler_isolates_host_and_company() {
        let (state, seen) = state(Some(good_row()));
        let (status, value) = body(
            document(
                State(state.clone()),
                Path("acme".to_owned()),
                request("acme.jesadakorn.com", "co-1", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value, serde_json::json!({"row": good_row()}));
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![("co-1".to_owned(), "order-1".to_owned())]
        );
        let (status, _) = body(
            document(
                State(state.clone()),
                Path("acme".to_owned()),
                request("other.jesadakorn.com", "co-1", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = body(
            document(
                State(state.clone()),
                Path("acme".to_owned()),
                request("acme.jesadakorn.com", "co-1", "order-1", true),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = body(
            document(
                State(state),
                Path("acme".to_owned()),
                request("acme.jesadakorn.com", "bad company!", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        let acme_seen = Arc::new(Mutex::new(Vec::new()));
        let north_seen = Arc::new(Mutex::new(Vec::new()));
        // Reader injection is the testable equivalent of the per-target Pg
        // reader that `router_for_holdings` builds from each Holding's pool.
        let state = build_state(vec![
            (
                LaneTarget {
                    host: "acme.jesadakorn.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                Arc::new(Probe {
                    row: Some(good_row_with("acme-order")),
                    seen: acme_seen.clone(),
                }) as Arc<dyn CommerceLane14Read>,
            ),
            (
                LaneTarget {
                    host: "northstar.jesadakorn.com".to_owned(),
                    holding_slug: "northstar".to_owned(),
                },
                Arc::new(Probe {
                    row: Some(good_row_with("northstar-order")),
                    seen: north_seen.clone(),
                }) as Arc<dyn CommerceLane14Read>,
            ),
        ])
        .expect("two distinct readers");

        let (status, value) = body(
            document(
                State(state.clone()),
                Path("acme".to_owned()),
                request("acme.jesadakorn.com", "co-1", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            value.get("row").and_then(|row| row.get("id")),
            Some(&Value::String("acme-order".to_owned()))
        );

        let (status, value) = body(
            document(
                State(state.clone()),
                Path("northstar".to_owned()),
                request("northstar.jesadakorn.com", "co-1", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            value.get("row").and_then(|row| row.get("id")),
            Some(&Value::String("northstar-order".to_owned()))
        );

        assert_eq!(
            *acme_seen.lock().expect("lock"),
            vec![("co-1".to_owned(), "order-1".to_owned())]
        );
        assert_eq!(
            *north_seen.lock().expect("lock"),
            vec![("co-1".to_owned(), "order-1".to_owned())]
        );

        // A Holding slug must not fall back to the other Holding's reader.
        let (status, _) = body(
            document(
                State(state),
                Path("northstar".to_owned()),
                request("acme.jesadakorn.com", "co-1", "order-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn handler_maps_unavailable_to_503() {
        struct Down;
        impl CommerceLane14Read for Down {
            fn document<'a>(
                &'a self,
                _company: &'a str,
                _id: &'a str,
            ) -> LaneReadFuture<'a, Option<Value>> {
                Box::pin(async { Err(super::super::domain::unavailable()) })
            }
            fn page<'a>(
                &'a self,
                _company: &'a str,
                _after: &'a str,
            ) -> LaneReadFuture<'a, Vec<Value>> {
                Box::pin(async { Err(super::super::domain::unavailable()) })
            }
            fn references<'a>(&'a self, _company: &'a str) -> LaneReadFuture<'a, Vec<Value>> {
                Box::pin(async { Err(super::super::domain::unavailable()) })
            }
        }
        let mut targets = BTreeMap::new();
        targets.insert(
            ("acme.jesadakorn.com".to_owned(), "acme".to_owned()),
            Arc::new(Down) as Arc<dyn CommerceLane14Read>,
        );
        let state = Arc::new(LaneState {
            targets,
            sales_targets: BTreeMap::new(),
        });
        let (status, value) = body(
            page_endpoint(
                state,
                request("acme.jesadakorn.com", "co-1", "cursor-1", false),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            value
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str),
            Some("COMMERCE_LANE14_READ_UNAVAILABLE")
        );
    }

    async fn page_endpoint(state: Arc<LaneState>, request: Request) -> Response {
        document_page(State(state), Path("acme".to_owned()), request).await
    }
}

#[cfg(test)]
mod sales_http_tests {
    use super::*;
    use crate::adapters::native_lane_14::application::SalesReadFuture;
    use crate::adapters::native_lane_14::domain::{
        OPEN_ORDER_KEYS, ORDER_COUNT_KEYS, PRODUCT_KEYS, PRODUCT_POINT_KEYS, SUPPORT_ORDER_KEYS,
        int_key,
    };
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use erpx_platform_runtime_server::public::http_support::serde_json::{self, Value};
    use std::sync::Mutex;

    #[derive(Clone)]
    struct SalesProbe {
        marker: String,
        seen: Arc<Mutex<Vec<String>>>,
    }

    fn named(marker: &str, keys: &[&str]) -> Value {
        let mut object = serde_json::Map::new();
        for key in keys {
            object.insert(
                (*key).to_owned(),
                if int_key(key) {
                    Value::from(1)
                } else if *key == "id" {
                    Value::String(marker.to_owned())
                } else {
                    Value::String((*key).to_owned())
                },
            );
        }
        Value::Object(object)
    }

    impl CommerceLane14SalesRead for SalesProbe {
        fn pricing_products<'a>(&'a self, company: &'a str) -> SalesReadFuture<'a, Vec<Value>> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let marker = self.marker.clone();
            Box::pin(async move {
                seen.lock().expect("lock").push(company);
                Ok(vec![named(&marker, PRODUCT_KEYS)])
            })
        }
        fn pricing_product<'a>(
            &'a self,
            _company: &'a str,
            _id: &'a str,
        ) -> SalesReadFuture<'a, Option<Value>> {
            let marker = self.marker.clone();
            Box::pin(async move { Ok(Some(named(&marker, PRODUCT_POINT_KEYS))) })
        }
        fn order_counts<'a>(
            &'a self,
            _scope: &'a SalesScope,
            _ids: &'a [String],
        ) -> SalesReadFuture<'a, Vec<Value>> {
            let marker = self.marker.clone();
            Box::pin(async move { Ok(vec![named(&marker, ORDER_COUNT_KEYS)]) })
        }
        fn open_orders<'a>(
            &'a self,
            _scope: &'a SalesScope,
            _party: &'a str,
        ) -> SalesReadFuture<'a, Vec<Value>> {
            let marker = self.marker.clone();
            Box::pin(async move { Ok(vec![named(&marker, OPEN_ORDER_KEYS)]) })
        }
        fn support_source<'a>(
            &'a self,
            _scope: &'a SalesScope,
            _party: &'a str,
        ) -> SalesReadFuture<'a, Option<Value>> {
            let marker = self.marker.clone();
            Box::pin(async move { Ok(Some(named(&marker, SUPPORT_ORDER_KEYS))) })
        }
    }

    fn target(host: &str, slug: &str) -> LaneTarget {
        LaneTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
        }
    }

    fn request(host: &str, company: &str) -> Request {
        Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, company)
            .body(Body::empty())
            .expect("request")
    }

    async fn body(response: Response) -> (StatusCode, Value) {
        let status = response.status();
        let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_sales_readers() {
        let acme_seen = Arc::new(Mutex::new(Vec::new()));
        let north_seen = Arc::new(Mutex::new(Vec::new()));
        let workflow: Arc<dyn CommerceLane14Read> = Arc::new(NoopWorkflow);
        let state = build_state_with_sales(
            vec![
                (target("acme.jesadakorn.com", "acme"), workflow.clone()),
                (target("northstar.jesadakorn.com", "northstar"), workflow),
            ],
            vec![
                (
                    target("acme.jesadakorn.com", "acme"),
                    Arc::new(SalesProbe {
                        marker: "acme-product".to_owned(),
                        seen: acme_seen.clone(),
                    }) as Arc<dyn CommerceLane14SalesRead>,
                ),
                (
                    target("northstar.jesadakorn.com", "northstar"),
                    Arc::new(SalesProbe {
                        marker: "northstar-product".to_owned(),
                        seen: north_seen.clone(),
                    }) as Arc<dyn CommerceLane14SalesRead>,
                ),
            ],
        )
        .expect("two distinct sales readers");

        for (host, slug, marker) in [
            ("acme.jesadakorn.com", "acme", "acme-product"),
            ("northstar.jesadakorn.com", "northstar", "northstar-product"),
        ] {
            let (status, value) = body(
                sales_pricing_products(
                    State(state.clone()),
                    Path(slug.to_owned()),
                    request(host, "co-1"),
                )
                .await,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                value
                    .get("rows")
                    .and_then(Value::as_array)
                    .and_then(|rows| rows.first()),
                Some(&named(marker, PRODUCT_KEYS))
            );
        }
        assert_eq!(*acme_seen.lock().expect("lock"), vec!["co-1".to_owned()]);
        assert_eq!(*north_seen.lock().expect("lock"), vec!["co-1".to_owned()]);

        let (status, _) = body(
            sales_pricing_products(
                State(state),
                Path("northstar".to_owned()),
                request("acme.jesadakorn.com", "co-1"),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    fn router_with_sales_reader(
        reader: Arc<dyn CommerceLane14SalesRead>,
        targets: Vec<LaneTarget>,
    ) -> Result<Router, BusinessError> {
        router_with_readers(
            targets
                .iter()
                .cloned()
                .map(|target| (target, Arc::new(NoopWorkflow) as Arc<dyn CommerceLane14Read>))
                .collect(),
            targets
                .into_iter()
                .map(|target| (target, reader.clone()))
                .collect(),
        )
    }

    struct NoopWorkflow;

    impl CommerceLane14Read for NoopWorkflow {
        fn document<'a>(
            &'a self,
            _company: &'a str,
            _id: &'a str,
        ) -> super::super::application::LaneReadFuture<'a, Option<Value>> {
            Box::pin(async { Ok(None) })
        }
        fn page<'a>(
            &'a self,
            _company: &'a str,
            _after: &'a str,
        ) -> super::super::application::LaneReadFuture<'a, Vec<Value>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn references<'a>(
            &'a self,
            _company: &'a str,
        ) -> super::super::application::LaneReadFuture<'a, Vec<Value>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    async fn get(
        address: std::net::SocketAddr,
        host: &str,
        path: &str,
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let mut stream = TcpStream::connect(address).await.expect("connect");
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: https://{host}\r\nx-erpx-context-company: co-1\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.expect("write");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read");
        String::from_utf8(bytes).expect("utf8")
    }

    #[tokio::test]
    async fn assembled_router_serves_sales_routes_and_isolates_holdings() {
        use erpx_platform_runtime_server::public::http_support::serve;
        use std::future::IntoFuture;
        use tokio::net::TcpListener;

        let acme = target("acme.jesadakorn.com", "acme");
        let reader: Arc<dyn CommerceLane14SalesRead> = Arc::new(SalesProbe {
            marker: "acme-product".to_owned(),
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let app = router_with_sales_reader(reader, vec![acme]).expect("sales router");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let task = tokio::spawn(serve(listener, app).into_future());

        let response = get(
            address,
            "acme.jesadakorn.com",
            "/api/v1/holdings/acme/commerce/lane14/sales-pricing-products",
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains(r#""rows""#), "{response}");
        assert!(response.contains("acme-product"), "{response}");

        let response = get(
            address,
            "northstar.jesadakorn.com",
            "/api/v1/holdings/acme/commerce/lane14/sales-pricing-products",
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");

        task.abort();
    }
}
