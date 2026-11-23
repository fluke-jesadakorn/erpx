//! Composition backend for the wf1a4 native POS owner-write routes.
//!
//! Self-contained so the shared composition files are not edited. The lead
//! mounts `pos_owner_write_router` / `pos_owner_write_company_router` next to
//! the other POS routes, supplying a `PosOwnerWriteExecutor` that opens the
//! authorized serializable Holding unit. All SQL remains in the POS owner
//! crate (`erpx_server_pos::public::owner_writes_http`); this module only
//! resolves the current Company and builds the statement on the caller's unit.
use erpx_server_pos::public::owner_writes_http::{
    pos_owner_write_company_router, pos_owner_write_router, owner_write_statement,
    PosOwnerWriteBackend, PosOwnerWriteFuture, PosOwnerWriteRequest, PosOwnerWriteTarget,
    PosOwnerStatement,
};
use erpx_server_runtime::public::http_support::{header, HeaderMap, Router};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::PgConnection;
use std::collections::BTreeMap;
use std::sync::Arc;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "POS_OWNER_WRITE_INVALID",
        "Review the POS owner write request.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "POS_OWNER_UNAVAILABLE",
            "The native POS owner service is unavailable.",
        ),
    }
}

fn unique_header(headers: &HeaderMap, name: header::HeaderName) -> Result<String, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values
        .next()
        .and_then(|value| value.to_str().ok())
        .ok_or_else(invalid)?
        .to_owned();
    if values.next().is_some() {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "POS_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    Ok(first)
}

/// Resolve and validate the current Company for a browser write. The native
/// bridge sends `x-erpx-context-company`; the concrete account policy must
/// additionally verify the current WorkContext/assignment before admission.
pub fn browser_company(headers: &HeaderMap) -> Result<String, BusinessError> {
    let company = unique_header(
        headers,
        header::HeaderName::from_static("x-erpx-context-company"),
    )?;
    if company.is_empty() || company.len() > 200 {
        return Err(invalid());
    }
    Ok(company)
}

/// Build the owner statement from a validated typed command on the caller's
/// serializable unit. Receipt/audit/outbox effects belong to that same unit and
/// are written by the caller, never here.
pub async fn statement_on_unit(
    connection: &mut PgConnection,
    company_id: &str,
    kind: &str,
    values: Vec<Value>,
) -> Result<PosOwnerStatement, BusinessError> {
    let company = CompanyId::new(company_id).map_err(|_| invalid())?;
    owner_write_statement(connection, &company, kind, values)
        .await
        .map_err(business)
}

/// Executor owned by composition: receives a validated Company and typed write
/// and runs it inside the already-opened serializable unit.
pub trait PosOwnerWriteExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        company_id: &'a str,
        kind: &'a str,
        values: &'a [Value],
        key: &'a str,
    ) -> PosOwnerWriteFuture<'a>;
}

pub struct PosOwnerWriteBackendImpl {
    executor: Arc<dyn PosOwnerWriteExecutor>,
}

impl PosOwnerWriteBackendImpl {
    pub fn new(executor: Arc<dyn PosOwnerWriteExecutor>) -> Self {
        Self { executor }
    }
}

impl PosOwnerWriteBackend for PosOwnerWriteBackendImpl {
    fn statement<'a>(&'a self, request: PosOwnerWriteRequest<'a>) -> PosOwnerWriteFuture<'a> {
        Box::pin(async move {
            let company = if request.holding_slug.is_empty() {
                unique_header(
                    request.headers,
                    header::HeaderName::from_static("x-erpx-context-company"),
                )?
            } else {
                browser_company(request.headers)?
            };
            self.executor
                .execute(&company, request.kind, request.values, request.key)
                .await
        })
    }
}

/// Pool-backed executor: opens the authorized serializable Holding unit and
/// returns the planned owner statement for the caller to execute together
/// with its receipt/audit/outbox. Mirrors FinanceOwnerWriteBackendImpl pool
/// fallback: per-company pool when registered, otherwise the Holding pool.
pub struct PosOwnerWriteCompanyExecutor {
    holding: DatabasePool,
    companies: BTreeMap<String, DatabasePool>,
}

impl PosOwnerWriteCompanyExecutor {
    pub fn new(holding: DatabasePool) -> Self {
        Self {
            holding,
            companies: BTreeMap::new(),
        }
    }

    pub fn with_companies(mut self, companies: BTreeMap<String, DatabasePool>) -> Self {
        self.companies = companies;
        self
    }

    fn pool(&self, company: &str) -> DatabasePool {
        self.companies
            .get(company)
            .cloned()
            .unwrap_or_else(|| self.holding.clone())
    }
}

impl PosOwnerWriteExecutor for PosOwnerWriteCompanyExecutor {
    fn execute<'a>(
        &'a self,
        company_id: &'a str,
        kind: &'a str,
        values: &'a [Value],
        _key: &'a str,
    ) -> PosOwnerWriteFuture<'a> {
        Box::pin(async move {
            let pool = self.pool(company_id);
            let company = company_id.to_owned();
            let kind = kind.to_owned();
            let values = values.to_vec();
            PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company = company.clone();
                    let kind = kind.clone();
                    let values = values.clone();
                    Box::pin(async move {
                        statement_on_unit(connection, &company, &kind, values)
                            .await
                            .map_err(PgTransactionError::Business)
                    })
                })
                .await
                .map_err(business)
        })
    }
}

/// Build the browser-facing owner-write router with exact registered targets.
pub fn pos_owner_write_browser_router(
    executor: Arc<dyn PosOwnerWriteExecutor>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn PosOwnerWriteBackend> = Arc::new(PosOwnerWriteBackendImpl::new(executor));
    let targets = targets
        .into_iter()
        .map(|(host, holding_slug, origin)| PosOwnerWriteTarget {
            host,
            holding_slug,
            origin,
            backend: backend.clone(),
        })
        .collect();
    pos_owner_write_router(targets)
}

/// Build the Company-keyed system owner-write router.
pub fn pos_owner_write_company_backend_router(
    executor: Arc<dyn PosOwnerWriteExecutor>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn PosOwnerWriteBackend> = Arc::new(PosOwnerWriteBackendImpl::new(executor));
    let pairs = companies
        .into_iter()
        .map(|company| (company, backend.clone()))
        .collect();
    pos_owner_write_company_router(pairs)
}

/// Kept for parity with other owner composition modules; the owner statement
/// builder itself rejects non-serializable units.
pub fn serializable_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_OWNER_SERIALIZABLE_REQUIRED",
        "The POS owner write requires a serializable unit.",
    )
}

