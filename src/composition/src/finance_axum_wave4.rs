//! Composition backend for the native Finance owner-write axum command routes.
//!
//! This module is self-contained and intentionally does not edit the shared
//! composition files. The lead mounts `router` / `company_router` alongside the
//! other owner routes, supplying the current-actor executor and exact
//! registered Holding targets. The owner crate owns the typed kind allow-list,
//! the canonical SQL and the Postgres execution (`erpx_server_finance::public::
//! finance_owner_write*`). This module resolves the current Company from the
//! validated browser context and hands the caller-owned serializable unit to
//! the executor. It never mints an actor, Company or authority.

use erpx_server_finance::public::finance_owner_write_application::{
    OwnerWriteFuture, OwnerWriteOutcome, execute_command,
};
use erpx_server_finance::public::finance_owner_write_http::{
    FinanceOwnerWriteBackend, FinanceOwnerWriteFuture, FinanceOwnerWriteRequest,
    FinanceOwnerWriteTarget,
};
use erpx_server_finance::public::finance_owner_write_postgres::PgFinanceOwnerWriteRepository;
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "FINANCE_OWNER_WRITE_INVALID",
        "Review the finance owner write request.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values
        .next()
        .and_then(|value| value.to_str().ok())
        .ok_or_else(invalid)?
        .to_owned();
    if values.next().is_some() {
        return Err(invalid());
    }
    Ok(first)
}

/// Resolve and validate the current Company for a browser write. The native
/// Finance bridges always send `x-erpx-context-company`; the concrete
/// composition account policy must additionally verify the current
/// WorkContext/assignment before the write is admitted. This resolution is
/// fail-closed for any missing or ambiguous context header.
pub fn browser_company(headers: &HeaderMap) -> Result<String, BusinessError> {
    let company = unique_header(headers, HeaderName::from_static("x-erpx-context-company"))?;
    if company.is_empty() || company.len() > 200 {
        return Err(invalid());
    }
    Ok(company)
}

/// Executor owned by composition: receives the already-opened serializable
/// unit, a resolved Company and a typed write. Implementations must validate
/// the current WorkContext/assignment before opening that unit.
pub trait FinanceOwnerWriteExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company_id: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
        key: &'a str,
    ) -> OwnerWriteFuture<'a, OwnerWriteOutcome>;
}

/// Default executor running the admitted DML and its receipt/audit/outbox in
/// the caller's unit. A concrete composition executor wraps this after it has
/// resolved the current actor and transaction.
pub struct PostgresOwnerWriteExecutor;

impl FinanceOwnerWriteExecutor for PostgresOwnerWriteExecutor {
    fn execute<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        _company_id: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
        key: &'a str,
    ) -> OwnerWriteFuture<'a, OwnerWriteOutcome> {
        let mut repository = PgFinanceOwnerWriteRepository::new(connection);
        Box::pin(
            async move { execute_command(kind, values.to_vec(), key, now, &mut repository).await },
        )
    }
}

/// Adapter binding the owner-write HTTP ingress to a composition executor and
/// its caller-owned transaction supplier.
pub struct FinanceOwnerWriteBackendAdapter<E> {
    executor: Arc<E>,
    unit: Arc<dyn Fn() -> OwnerWriteFuture<'static, PgConnection> + Send + Sync>,
}

impl<E: FinanceOwnerWriteExecutor> FinanceOwnerWriteBackendAdapter<E> {
    pub fn new(
        executor: Arc<E>,
        unit: Arc<dyn Fn() -> OwnerWriteFuture<'static, PgConnection> + Send + Sync>,
    ) -> Self {
        Self { executor, unit }
    }
}

impl<E: FinanceOwnerWriteExecutor> FinanceOwnerWriteBackend for FinanceOwnerWriteBackendAdapter<E> {
    fn execute<'a>(&'a self, request: FinanceOwnerWriteRequest<'a>) -> FinanceOwnerWriteFuture<'a> {
        Box::pin(async move {
            let company = if request.holding_slug.is_empty() {
                unique_header(
                    request.headers,
                    HeaderName::from_static("x-erpx-context-company"),
                )?
            } else {
                browser_company(request.headers)?
            };
            // The unit supplier is composition-owned: it opens and
            // serializes the transaction, runs the executor, and commits on
            // success. The owner crate never commits or rolls back.
            let mut connection = (self.unit)().await?;
            let outcome = self
                .executor
                .execute(
                    &mut connection,
                    &company,
                    &request.command.kind,
                    &request.command.values,
                    request.command.now,
                    request.key,
                )
                .await?;
            Ok(outcome)
        })
    }
}

/// Build the browser-facing owner-write router with exact registered targets.
pub fn router<E: FinanceOwnerWriteExecutor + 'static>(
    executor: Arc<E>,
    unit: Arc<dyn Fn() -> OwnerWriteFuture<'static, PgConnection> + Send + Sync>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn FinanceOwnerWriteBackend> =
        Arc::new(FinanceOwnerWriteBackendAdapter::new(executor, unit));
    let targets = targets
        .into_iter()
        .map(|(host, holding_slug, origin)| FinanceOwnerWriteTarget {
            host,
            holding_slug,
            origin,
            backend: backend.clone(),
        })
        .collect();
    erpx_server_finance::public::finance_owner_write_http::owner_write_router(targets)
}

/// Build the Company-keyed system owner-write router.
pub fn company_router<E: FinanceOwnerWriteExecutor + 'static>(
    executor: Arc<E>,
    unit: Arc<dyn Fn() -> OwnerWriteFuture<'static, PgConnection> + Send + Sync>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn FinanceOwnerWriteBackend> =
        Arc::new(FinanceOwnerWriteBackendAdapter::new(executor, unit));
    let pairs = companies
        .into_iter()
        .map(|company| (company, backend.clone()))
        .collect();
    erpx_server_finance::public::finance_owner_write_http::owner_write_company_router(pairs)
}
