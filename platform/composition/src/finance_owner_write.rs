//! Native Finance owner-write composition.
//!
//! Company-keyed system callers have no browser WorkContext. Composition
//! resolves the Company from the validated path/header, opens one serializable
//! unit on the owning pool and runs the owner command with its
//! receipt/audit/outbox effects. The owner crate owns the typed kind allow-list
//! and SQL; this module only owns the pool and unit.
use erpx_finance_server::public::finance_owner_write_application::{
    OwnerWriteOutcome, execute_command,
};
use erpx_finance_server::public::finance_owner_write_http::{
    FinanceOwnerWriteBackend, FinanceOwnerWriteFuture, FinanceOwnerWriteRequest,
    owner_write_company_router,
};
use erpx_finance_server::public::finance_owner_write_postgres::PgFinanceOwnerWriteRepository;
use erpx_platform_runtime_server::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "FINANCE_OWNER_WRITE_INVALID",
        "Review the finance owner write request.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_OWNER_WRITE_UNAVAILABLE",
        "The native Finance owner write is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn company(headers: &HeaderMap) -> Result<String, BusinessError> {
    let mut values = headers.get_all(CONTEXT_COMPANY).iter();
    let value = values
        .next()
        .and_then(|value| value.to_str().ok())
        .ok_or_else(invalid)?;
    if values.next().is_some() || value.is_empty() || value.len() > 200 {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

pub struct FinanceOwnerWriteBackendImpl {
    holding: DatabasePool,
    companies: BTreeMap<String, DatabasePool>,
}

impl FinanceOwnerWriteBackendImpl {
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

impl FinanceOwnerWriteBackend for FinanceOwnerWriteBackendImpl {
    fn execute<'a>(&'a self, request: FinanceOwnerWriteRequest<'a>) -> FinanceOwnerWriteFuture<'a> {
        Box::pin(async move {
            let company = company(request.headers)?;
            let kind = request.command.kind.clone();
            let values = request.command.values.clone();
            let now = request.command.now;
            let key = request.key.to_owned();
            let pool = self.pool(&company);
            let outcome = PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let kind = kind.clone();
                    let values = values.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        let mut repository = PgFinanceOwnerWriteRepository::new(connection);
                        execute_command(&kind, values, &key, now, &mut repository)
                            .await
                            .map_err(PgTransactionError::Business)
                    })
                })
                .await
                .map_err(business)?;
            Ok(OwnerWriteOutcome {
                kind: outcome.kind,
                rows: outcome.rows,
            })
        })
    }
}

/// Mount the Company-keyed system owner-write route. The private boundary
/// keeps it loopback-only; the Company in the path and header must agree.
pub fn finance_owner_write_companies(
    holding: DatabasePool,
    companies: BTreeMap<String, DatabasePool>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn FinanceOwnerWriteBackend> = Arc::new(
        FinanceOwnerWriteBackendImpl::new(holding).with_companies(companies.clone()),
    );
    owner_write_company_router(
        companies
            .keys()
            .cloned()
            .map(|company| (company, backend.clone()))
            .collect(),
    )
}
