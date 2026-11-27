//! Native read companion for the Quality receiving-origin fact lookup.
//!
//! This is the owner-local successor of the receipt half of
//! `adapters/quality-purchase-sources.ts`. The axum read router owns transport
//! and projection; the Pg reader reuses the Commerce-owned receipt-origin port
//! in `quality_receipt_origin`. No TS SQL is issued here: a missing, malformed
//! or failing native runtime fails closed with an explicit 503.

use crate::adapters::quality_receipt_origin::{
    QualityReceiptCommerceFacts, inspect_quality_receipt_commerce,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_LINE: HeaderName = HeaderName::from_static("x-erpx-context-fulfillment-line");

pub type QualityReceiptOriginReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Exact Commerce-owned receiving facts. No Quality hold, Inventory allocation
/// or purchase authority is inferred from this projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityReceiptOriginFacts {
    pub order_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub fulfillment_line_id: String,
}

impl From<QualityReceiptCommerceFacts> for QualityReceiptOriginFacts {
    fn from(facts: QualityReceiptCommerceFacts) -> Self {
        Self {
            order_id: facts.order_id,
            company_id: facts.company_id,
            branch_id: facts.branch_id,
            warehouse_id: facts.warehouse_id,
            product_id: facts.product_id,
            fulfillment_line_id: facts.fulfillment_line_id,
        }
    }
}

/// Composition injects the owner reader behind this port. The owner owns the
/// SQL, the serializable unit and the projection; the router only transports
/// bounded facts.
pub trait QualityReceiptOriginRead: Send + Sync {
    fn inspect<'a>(
        &'a self,
        company: &'a str,
        branch: Option<&'a str>,
        line: &'a str,
    ) -> QualityReceiptOriginReadFuture<'a, Option<QualityReceiptOriginFacts>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_RECEIPT_ORIGIN_READ_UNAVAILABLE",
        "The native Quality receipt-origin service is unavailable.",
    )
}

/// Pg-backed owner reader. It reuses the existing Commerce receipt-origin port
/// inside the shared serializable unit; no new SQL or business rule is added.
pub struct PgQualityReceiptOriginRead {
    pool: DatabasePool,
}

impl PgQualityReceiptOriginRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl QualityReceiptOriginRead for PgQualityReceiptOriginRead {
    fn inspect<'a>(
        &'a self,
        company: &'a str,
        branch: Option<&'a str>,
        line: &'a str,
    ) -> QualityReceiptOriginReadFuture<'a, Option<QualityReceiptOriginFacts>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let branch = branch.map(str::to_owned);
        let line = line.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch = branch.clone();
                let line = line.clone();
                Box::pin(async move {
                    let company_id = CompanyId::new(company).map_err(|_| unavailable())?;
                    let branch_id = match branch {
                        Some(value) => Some(RecordId::new(value).map_err(|_| unavailable())?),
                        None => None,
                    };
                    let current = inspect_quality_receipt_commerce(
                        connection,
                        &company_id,
                        branch_id.as_ref(),
                        &line,
                    )
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        PgTransactionError::Persistence(_) => unavailable(),
                    })?;
                    Ok(current.facts().cloned().map(QualityReceiptOriginFacts::from))
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

pub struct QualityReceiptOriginReadTarget {
    pub host: String,
    pub holding_slug: String,
}

struct QualityReceiptOriginReadState {
    targets: BTreeMap<(String, String), Arc<dyn QualityReceiptOriginRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_RECEIPT_ORIGIN_READ_REQUEST_INVALID",
        "Use one bounded Quality receipt-origin read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_RECEIPT_ORIGIN_READ_CONFIG_INVALID",
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

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
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

/// Build the native read router with one distinct Commerce-owned reader per
/// exact Holding target. Routing keys on `(host, holding_slug)`, so two
/// Holdings sharing the same holding path still resolve to separate readers
/// and cannot observe each other's receiving-origin facts.
pub fn quality_receipt_origin_read_router_for_targets(
    targets: Vec<(QualityReceiptOriginReadTarget, Arc<dyn QualityReceiptOriginRead>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
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
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/quality-receipt-origin",
            get(current),
        )
        .with_state(Arc::new(QualityReceiptOriginReadState {
            targets: configured,
        })))
}

/// Backward-compatible single-reader constructor. Every registered target
/// resolves to the same reader; new multi-Holding bindings should use
/// `quality_receipt_origin_read_router_for_targets` so each Holding owns its
/// reader.
pub fn quality_receipt_origin_read_router(
    reader: Arc<dyn QualityReceiptOriginRead>,
    targets: Vec<QualityReceiptOriginReadTarget>,
) -> Result<Router, BusinessError> {
    quality_receipt_origin_read_router_for_targets(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

async fn current(
    State(state): State<Arc<QualityReceiptOriginReadState>>,
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
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let branch = unique_header(request.headers(), CONTEXT_BRANCH)?
            .filter(|value| valid_id(value));
        let line = unique_header(request.headers(), CONTEXT_LINE)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let facts = reader.inspect(company, branch, line).await?;
        let origin = facts.map(|facts| {
            serde_json::json!({
                "orderId": facts.order_id,
                "companyId": facts.company_id,
                "branchId": facts.branch_id,
                "warehouseId": facts.warehouse_id,
                "productId": facts.product_id,
                "fulfillmentLineId": facts.fulfillment_line_id,
            })
        });
        Ok::<_, BusinessError>(serde_json::json!({"origin": origin}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}
