//! Composition backend for the wf3a13 native Customer Authority write routes.
//! Self-contained: the lead mounts the returned routers next to the other
//! Customer Authority routes. The owner crate owns the SQL; this module only
//! resolves the validated Company/Holding scope and delegates to a composition
//! executor that opens the caller's serializable unit.
use erpx_server_runtime::public::http_support::{HeaderMap, Router, header};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::workflow_wave3::{
    WorkflowWave3Backend, WorkflowWave3Command, WorkflowWave3Future, WorkflowWave3Request,
    WorkflowWave3Target, plan_workflow_wave3, workflow_wave3_browser_router,
    workflow_wave3_company_router, workflow_wave3_holding_router,
};
use serde_json::Value;
use std::sync::Arc;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "WORKFLOW_WAVE3_REQUEST_INVALID",
        "Review the Customer Authority owner write request.",
    )
}

fn unique_header(headers: &HeaderMap, name: header::HeaderName) -> Result<String, BusinessError> {
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

/// Stateless planner: returns the owner SQL to the caller, which executes it in
/// its own serializable unit together with the receipt/audit/outbox effects.
pub struct WorkflowWave3Planner;

impl WorkflowWave3Executor for WorkflowWave3Planner {
    fn execute<'a>(
        &'a self,
        _scope: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
        _key: &'a str,
    ) -> WorkflowWave3Future<'a> {
        Box::pin(async move {
            let command = WorkflowWave3Command {
                kind: kind.to_owned(),
                values: values.to_vec(),
                now,
            };
            plan_workflow_wave3(&command)
                .map_err(|e| BusinessError::new(FailureKind::InvalidInput, e.code, e.message))
        })
    }
}

pub trait WorkflowWave3Executor: Send + Sync {
    fn execute<'a>(
        &'a self,
        scope: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
        key: &'a str,
    ) -> WorkflowWave3Future<'a>;
}

pub struct WorkflowWave3BackendAdapter {
    executor: Arc<dyn WorkflowWave3Executor>,
}

impl WorkflowWave3BackendAdapter {
    pub fn new(executor: Arc<dyn WorkflowWave3Executor>) -> Self {
        Self { executor }
    }
}

impl WorkflowWave3Backend for WorkflowWave3BackendAdapter {
    fn statement<'a>(&'a self, request: WorkflowWave3Request<'a>) -> WorkflowWave3Future<'a> {
        Box::pin(async move {
            let scope = if !request.holding_slug.is_empty() {
                unique_header(
                    request.headers,
                    header::HeaderName::from_static("x-erpx-context-holding"),
                )?
            } else {
                unique_header(
                    request.headers,
                    header::HeaderName::from_static("x-erpx-context-company"),
                )?
            };
            if scope.is_empty() || scope.len() > 200 {
                return Err(invalid());
            }
            self.executor
                .execute(
                    &scope,
                    request.kind,
                    request.values,
                    request.now,
                    request.key,
                )
                .await
        })
    }
}

pub fn workflow_wave3_browser(
    executor: Arc<dyn WorkflowWave3Executor>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn WorkflowWave3Backend> =
        Arc::new(WorkflowWave3BackendAdapter::new(executor));
    workflow_wave3_browser_router(
        targets
            .into_iter()
            .map(|(host, holding_slug, origin)| WorkflowWave3Target {
                host,
                holding_slug,
                origin,
                backend: backend.clone(),
            })
            .collect(),
    )
}

pub fn workflow_wave3_holding(
    executor: Arc<dyn WorkflowWave3Executor>,
    holdings: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn WorkflowWave3Backend> =
        Arc::new(WorkflowWave3BackendAdapter::new(executor));
    workflow_wave3_holding_router(
        holdings
            .into_iter()
            .map(|holding| (holding, backend.clone()))
            .collect(),
    )
}

pub fn workflow_wave3_company(
    executor: Arc<dyn WorkflowWave3Executor>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn WorkflowWave3Backend> =
        Arc::new(WorkflowWave3BackendAdapter::new(executor));
    workflow_wave3_company_router(
        companies
            .into_iter()
            .map(|company| (company, backend.clone()))
            .collect(),
    )
}

