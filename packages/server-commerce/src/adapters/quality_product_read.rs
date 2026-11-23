//! Native read companion for the Quality active-product guard.
//!
//! This is the owner-local successor of the SELECT in
//! `adapters/quality-product.ts`. The axum read router owns the SQL and the
//! projection; the retained TS consumer only transports bounded Company and
//! product identities. There is no SQL fallback: a missing or malformed native
//! runtime fails closed with an explicit 503.

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use sqlx::Row;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_PRODUCT: HeaderName = HeaderName::from_static("x-erpx-context-product");

pub type QualityProductReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityProductFacts {
    pub id: String,
    pub version: i64,
}

/// Composition injects the owner reader behind this port. The owner owns the
/// SQL, the transaction and the projection; the router only transports facts.
pub trait QualityProductRead: Send + Sync {
    fn current<'a>(
        &'a self,
        company: &'a str,
        product: &'a str,
    ) -> QualityProductReadFuture<'a, Option<QualityProductFacts>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_PRODUCT_READ_UNAVAILABLE",
        "The native Quality product service is unavailable.",
    )
}

/// Pg-backed owner reader. `commerce_products` is Commerce-owned; no Quality
/// permission, purchase or stock authority is inferred from this read.
pub struct PgQualityProductRead {
    pool: DatabasePool,
}

impl PgQualityProductRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl QualityProductRead for PgQualityProductRead {
    fn current<'a>(
        &'a self,
        company: &'a str,
        product: &'a str,
    ) -> QualityProductReadFuture<'a, Option<QualityProductFacts>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let product = product.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let product = product.clone();
                Box::pin(async move {
                    let row = sqlx::query(
                        "SELECT id,version FROM commerce_products WHERE id=$1 AND company_id=$2 AND status='active'",
                    )
                    .bind(product.as_str())
                    .bind(company.as_str())
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(|_| unavailable())?;
                    row.map(|r| {
                        Ok(QualityProductFacts {
                            id: r.try_get::<String, _>("id").map_err(|_| unavailable())?,
                            version: r.try_get::<i64, _>("version").map_err(|_| unavailable())?,
                        })
                    })
                    .transpose()
                })
            })
            .await
            .map_err(|error| match error {
                erpx_server_runtime::public::postgres::PgTransactionError::Business(error) => error,
                erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_) => {
                    unavailable()
                }
            })
        })
    }
}

pub struct QualityProductReadTarget {
    pub host: String,
    pub holding_slug: String,
}

struct QualityProductReadState {
    targets: BTreeMap<(String, String), Arc<dyn QualityProductRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_PRODUCT_READ_REQUEST_INVALID",
        "Use one bounded Quality product read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_PRODUCT_READ_CONFIG_INVALID",
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

/// Build the native read router. The lead mounts it alongside the other
/// Commerce owner routers; no new TS SQL or business rule is introduced here.
pub fn quality_product_read_router(
    reader: Arc<dyn QualityProductRead>,
    targets: Vec<QualityProductReadTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
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
            "/api/v1/holdings/{holding_slug}/commerce/quality-product",
            get(current),
        )
        .with_state(Arc::new(QualityProductReadState {
            targets: configured,
        })))
}

async fn current(
    State(state): State<Arc<QualityProductReadState>>,
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
        let product = unique_header(request.headers(), CONTEXT_PRODUCT)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let facts = reader.current(company, product).await?;
        let product = facts.map(|facts| {
            serde_json::json!({"id": facts.id, "version": facts.version})
        });
        Ok::<_, BusinessError>(serde_json::json!({"product": product}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}
