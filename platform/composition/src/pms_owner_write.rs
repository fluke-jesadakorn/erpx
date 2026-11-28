//! Composition backend for the wf1a2 native PMS owner-write routes.
//!
//! Self-contained: the lead mounts `pms_owner_write_router` /
//! `pms_owner_write_company_router` alongside the other PMS routes. The owner
//! crate owns every SQL statement and the receipt/audit/outbox; this module
//! only resolves the current Company/authority and opens the caller-owned
//! serializable unit. It does not edit the shared composition files.
use erpx_pms_server::public::pms_owner_write::{
    PmsOwnerWriteActor, PmsOwnerWriteBackend, PmsOwnerWriteFuture as OwnerFuture,
    PmsOwnerWriteRequest, PmsOwnerWriteTarget, PgPmsOwnerWrite, execute_owner_write,
    pms_owner_write_company_router, pms_owner_write_router,
};
use erpx_platform_runtime_server::public::http_support::Router;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_OWNER_WRITE_UNAVAILABLE",
        "The native hotel service is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Execute one owner write inside an already-opened serializable unit.
pub async fn execute_on_unit(
    connection: &mut PgConnection,
    actor: &PmsOwnerWriteActor,
    kind: &str,
    values: Vec<Value>,
    now: i64,
    key: &str,
) -> Result<Value, BusinessError> {
    let mut repository = PgPmsOwnerWrite::new(connection, actor);
    execute_owner_write(actor, kind, values, now, key, &mut repository)
        .await
        .map_err(business)
}

/// Executor owned by composition: receives a resolved Company/actor and runs
/// the owner-selected statement inside the serializable unit.
pub trait PmsOwnerWriteExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        company_id: &'a str,
        user_id: &'a str,
        assignment_id: &'a str,
        authority_json: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
        key: &'a str,
    ) -> OwnerFuture<'a, Value>;
}

pub struct PmsOwnerWriteBackendAdapter {
    executor: Arc<dyn PmsOwnerWriteExecutor>,
}

impl PmsOwnerWriteBackendAdapter {
    pub fn new(executor: Arc<dyn PmsOwnerWriteExecutor>) -> Self {
        Self { executor }
    }
}

impl PmsOwnerWriteBackend for PmsOwnerWriteBackendAdapter {
    fn execute<'a>(&'a self, request: PmsOwnerWriteRequest<'a>) -> OwnerFuture<'a, Value> {
        Box::pin(async move {
            self.executor
                .execute(
                    request.company_id,
                    request.user_id,
                    request.assignment_id,
                    request.authority_json,
                    request.kind,
                    request.values,
                    request.now,
                    request.key,
                )
                .await
        })
    }
}

/// Browser-facing router with exact registered Holding host/origin targets.
pub fn browser_router(
    executor: Arc<dyn PmsOwnerWriteExecutor>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn PmsOwnerWriteBackend> = Arc::new(PmsOwnerWriteBackendAdapter::new(executor));
    let targets = targets
        .into_iter()
        .map(|(host, holding_slug, origin)| PmsOwnerWriteTarget {
            host,
            holding_slug,
            origin,
            backend: backend.clone(),
        })
        .collect();
    pms_owner_write_router(targets)
}

/// Company-keyed system router for workers without a browser WorkContext.
pub fn company_router(
    executor: Arc<dyn PmsOwnerWriteExecutor>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn PmsOwnerWriteBackend> = Arc::new(PmsOwnerWriteBackendAdapter::new(executor));
    let pairs = companies
        .into_iter()
        .map(|company| (company, backend.clone()))
        .collect();
    pms_owner_write_company_router(pairs)
}
