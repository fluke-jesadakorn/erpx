//! Native PMS import-batch list HTTP ingress for the `.../pms/imports` family.
//!
//! Rust owns the exact registered Host/Holding target, the bounded query facts,
//! the application use case and the complete response payload. The handler
//! executes no SQL: it calls the application use case, which delegates to the
//! sqlx adapter. Account session resolution and IAM authority remain
//! composition responsibilities behind the selected target and header scope.
use std::{collections::BTreeMap, sync::Arc};

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::json;

use crate::adapters::import_batches_postgres::PgPmsImportBatchRead;
use crate::application::import_batches::{
    PmsImportBatchFuture, pms_import_batches,
};
use crate::domain::import_batches::{
    PmsImportBatchFacts, PmsImportBatchView, request_invalid,
};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const QUERY_LIMIT: usize = 4 * 1024;

/// Exact registered Holding target for the mounted owner router.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportBatchTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Bounded owner reader injected into the router. Production supplies the
/// sqlx adapter; tests inject a reader that proves the payload without a DB.
pub trait ImportBatchRead: Send + Sync {
    fn batches<'a>(
        &'a self,
        facts: &'a PmsImportBatchFacts,
    ) -> PmsImportBatchFuture<'a, Vec<PmsImportBatchView>, BusinessError>;
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_IMPORT_BATCHES_UNAVAILABLE",
        "The native import batch read is unavailable.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_IMPORT_BATCHES_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn valid_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.chars().count() <= max
        && !value.chars().any(char::is_control)
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
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

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(request_invalid());
            };
            out.push(high * 16 + low);
            index += 3;
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| request_invalid())
}

/// Bounded `property` query fact with duplicate-name refusal.
fn query_property(request: &Request) -> Result<String, BusinessError> {
    let Some(raw) = request.uri().query() else {
        return Err(request_invalid());
    };
    if raw.len() > QUERY_LIMIT {
        return Err(request_invalid());
    }
    let mut property = None;
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name)?;
        if name != "property" {
            continue;
        }
        if property.is_some() {
            return Err(request_invalid());
        }
        property = Some(percent_decode(value)?);
    }
    let value = property.ok_or_else(request_invalid)?;
    if !valid_token(&value, 160) {
        return Err(request_invalid());
    }
    Ok(value)
}

struct BatchState {
    targets: BTreeMap<(String, String), Arc<dyn ImportBatchRead>>,
}

fn configured_targets(
    entries: impl IntoIterator<Item = (ImportBatchTarget, Arc<dyn ImportBatchRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn ImportBatchRead>>, BusinessError> {
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
    Ok(configured)
}

fn read_router_from(configured: BTreeMap<(String, String), Arc<dyn ImportBatchRead>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/imports",
            get(import_batches),
        )
        .with_state(Arc::new(BatchState { targets: configured }))
}

/// Reader-injection path for tests: one distinct reader per Holding.
pub fn router_with_readers(
    entries: Vec<(ImportBatchTarget, Arc<dyn ImportBatchRead>)>,
) -> Result<Router, BusinessError> {
    Ok(read_router_from(configured_targets(entries)?))
}

/// Mount entry point with a single shared injected reader.
pub fn router_with_reader(
    reader: Arc<dyn ImportBatchRead>,
    targets: Vec<ImportBatchTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Mount entry point with one shared Pg pool.
pub fn router(pool: DatabasePool, targets: Vec<ImportBatchTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgImportBatchRead::new(pool)), targets)
}

/// Mount entry point with one Pg reader per Holding pool. `pools[i]` belongs to
/// `targets[i]`, so no Holding can reach another's data.
pub fn router_for_holdings(
    targets: Vec<(ImportBatchTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn ImportBatchRead> = Arc::new(PgImportBatchRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn import_batches(
    State(state): State<Arc<BatchState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(not_found)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(not_found)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_token(value, 200))
            .ok_or_else(request_invalid)?
            .to_owned();
        let property = query_property(&request)?;
        let facts = PmsImportBatchFacts {
            company_id: company,
            property_id: property,
        };
        let batches = reader.batches(&facts).await?;
        Ok::<_, BusinessError>(json!({"data": batches}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// sqlx-backed owner reader. Executes the retained statements on the owner
/// pool inside a bounded unit; no other owner's table is read.
pub struct PgImportBatchRead {
    pool: DatabasePool,
}

impl PgImportBatchRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ImportBatchRead for PgImportBatchRead {
    fn batches<'a>(
        &'a self,
        facts: &'a PmsImportBatchFacts,
    ) -> PmsImportBatchFuture<'a, Vec<PmsImportBatchView>, BusinessError> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let facts = facts.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let facts = facts.clone();
                Box::pin(async move {
                    let mut reader = PgPmsImportBatchRead::new(connection);
                    // The use case over the Pg adapter already returns
                    // Result<_, PgTransactionError<BusinessError>>, exactly the
                    // output channel expected by `run`.
                    pms_import_batches(&facts, &mut reader).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::body::Body;

    struct Fake(Vec<PmsImportBatchView>);

    impl ImportBatchRead for Fake {
        fn batches<'a>(
            &'a self,
            _facts: &'a PmsImportBatchFacts,
        ) -> PmsImportBatchFuture<'a, Vec<PmsImportBatchView>, BusinessError> {
            let rows = self.0.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn target() -> ImportBatchTarget {
        ImportBatchTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        }
    }

    fn state(reader: Arc<dyn ImportBatchRead>) -> Arc<BatchState> {
        Arc::new(BatchState {
            targets: configured_targets(vec![(target(), reader)]).unwrap(),
        })
    }

    async fn call(uri: &str, company: Option<&str>) -> Response {
        let mut builder = Request::builder()
            .header(header::HOST, "acme.jesadakorn.com")
            .uri(uri);
        if let Some(company) = company {
            builder = builder.header(CONTEXT_COMPANY, company);
        }
        import_batches(
            State(state(Arc::new(Fake(vec![])))),
            Path("acme".to_owned()),
            builder.body(Body::empty()).unwrap(),
        )
        .await
    }

    #[test]
    fn rejects_invalid_target_configuration() {
        let reader: Arc<dyn ImportBatchRead> = Arc::new(Fake(vec![]));
        assert_eq!(
            router_with_reader(reader.clone(), vec![])
                .unwrap_err()
                .code,
            "PMS_IMPORT_BATCHES_CONFIG_INVALID"
        );
        assert!(router_with_reader(reader, vec![target()]).is_ok());
    }

    #[tokio::test]
    async fn foreign_host_and_missing_scope_fail_closed() {
        let builder = Request::builder()
            .header(header::HOST, "attacker.example.test")
            .uri("/api/v1/holdings/acme/pms/imports?property=property-1")
            .header(CONTEXT_COMPANY, "company");
        let response = import_batches(
            State(state(Arc::new(Fake(vec![])))),
            Path("acme".to_owned()),
            builder.body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            call(
                "/api/v1/holdings/acme/pms/imports?property=property-1",
                None
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn invalid_query_is_rejected() {
        assert_eq!(
            call("/api/v1/holdings/acme/pms/imports", Some("company"))
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            call(
                "/api/v1/holdings/acme/pms/imports?property=a&property=b",
                Some("company")
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
