//! Composition backend for the single native Organization owner-write route.
//!
//! This module is intentionally standalone: the lead mounts the router built
//! by [`organization_owner_write_router`] into the private gateway and injects
//! the backend built by [`OrganizationOwnerWriteBackend::new`]. It resolves the
//! current Company/authority from the private caller's bounded context headers,
//! opens one serializable Holding transaction per command and delegates the SQL
//! and ledger to the Organization owner executor. No SQL and no business rules
//! live here.
use erpx_organization_server::public::organization_owner_write::{
    execute_owner_write, owner_statement, OrganizationOwnerWriteBackend, OwnerWriteFuture,
    OwnerWriteRequest, OwnerWriteResult, OrganizationOwnerWriteTarget, OwnerWriteActor,
    organization_owner_write_router, organization_owner_write_company_router,
};
use erpx_platform_runtime_server::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, Clock, CompanyId, FailureKind};
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_REVISION: HeaderName = HeaderName::from_static("x-erpx-context-revision");
const CONTEXT_ASSIGNMENT_VERSION: HeaderName =
    HeaderName::from_static("x-erpx-context-assignment-version");

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "ORGANIZATION_OWNER_WRITE_INVALID",
        "Review the organization write request.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_OWNER_UNAVAILABLE",
        "The native Organization service is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values
        .next()
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 200)
        .ok_or_else(invalid)?
        .to_owned();
    if values.next().is_some() {
        return Err(invalid());
    }
    Ok(first)
}

fn maybe_header(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?.to_owned();
    if values.next().is_some() || first.len() > 200 {
        return None;
    }
    Some(first)
}

/// Composition-owned backend. `pool` is the Holding Postgres pool that owns
/// the Organization tables; `clock` supplies the committed timestamp.
pub struct OrganizationOwnerWriteBackendImpl<C> {
    pool: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> OrganizationOwnerWriteBackendImpl<C> {
    pub fn new(pool: DatabasePool, clock: Arc<C>) -> Self {
        Self { pool, clock }
    }

    /// Resolve a fresh, bounded actor snapshot. The private gateway admits the
    /// request and maps the Account session to these context headers; this
    /// function never mints an identity or a permission.
    fn actor(headers: &HeaderMap) -> Result<OwnerWriteActor, BusinessError> {
        let company = unique_header(headers, CONTEXT_COMPANY)?;
        // Browser writes carry the full mapped session. Company-keyed system
        // writes carry only the Company; empty actor facts are recorded on the
        // ledger exactly as the retained worker paths did.
        let user_id = maybe_header(headers, CONTEXT_USER).unwrap_or_default();
        let assignment_id = maybe_header(headers, CONTEXT_ASSIGNMENT).unwrap_or_default();
        let revision = maybe_header(headers, CONTEXT_REVISION)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        let assignment_version = maybe_header(headers, CONTEXT_ASSIGNMENT_VERSION)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        Ok(OwnerWriteActor {
            user_id: user_id.clone(),
            assignment_id: assignment_id.clone(),
            company_id: CompanyId::new(&company).map_err(|_| invalid())?,
            authority_json: serde_json::json!({
                "userId": user_id,
                "assignmentId": assignment_id,
                "companyId": company,
                "revision": revision,
                "assignmentVersion": assignment_version,
            })
            .to_string(),
        })
    }
}

impl<C: Clock + Send + Sync + 'static> OrganizationOwnerWriteBackend
    for OrganizationOwnerWriteBackendImpl<C>
{
    fn write<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a> {
        Box::pin(async move {
            let actor = Self::actor(request.headers)?;
            let company = actor.company_id.clone();
            let command = request.command.clone();
            if command.statement_mode() {
                let (sql, values) = owner_statement(&command.kind, &command.values).map_err(business)?;
                return Ok(OwnerWriteResult {
                    kind: command.kind,
                    rows: 0,
                    version: 1,
                    sql: Some(sql),
                    values,
                });
            }
            let key = request.key.as_str().to_owned();
            let now = if command.now > 0 {
                command.now
            } else {
                self.clock.now().get()
            };
            let pool = self.pool.clone();
            PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let company = company.clone();
                    let command = command.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        execute_owner_write(
                            &mut *connection,
                            &company,
                            &actor,
                            &command.kind,
                            &command.values,
                            &key,
                            now,
                        )
                        .await
                    })
                })
                .await
                .map_err(business)
        })
    }
}

/// Build the one owner-write router with exact registered Holding targets.
pub fn organization_owner_write_routes(
    targets: Vec<OrganizationOwnerWriteTarget>,
) -> Result<Router, BusinessError> {
    organization_owner_write_router(targets)
}

/// Build the browser-facing owner-write router with exact registered Holding
/// targets. Mirrors the POS owner-write mount: one backend shared across the
/// targets of this assembly batch.
pub fn organization_owner_write_browser_router(
    backend: Arc<dyn OrganizationOwnerWriteBackend>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let targets = targets
        .into_iter()
        .map(
            |(host, holding_slug, origin)| OrganizationOwnerWriteTarget {
                host,
                holding_slug,
                origin,
                backend: backend.clone(),
            },
        )
        .collect();
    organization_owner_write_router(targets)
}

/// Build the Company-keyed system owner-write router for callers holding only
/// a Company id (the TypeScript statement bridge). Mirrors the POS company
/// backend mount.
pub fn organization_owner_write_company_backend_router(
    backend: Arc<dyn OrganizationOwnerWriteBackend>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let pairs = companies
        .into_iter()
        .map(|company| (company, backend.clone()))
        .collect();
    organization_owner_write_company_router(pairs)
}
