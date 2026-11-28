//! Composition backend for the wf3a13 native Customer Authority write routes.
//! Self-contained: the lead mounts the returned routers next to the other
//! Customer Authority routes. The owner crate owns the SQL; this module only
//! resolves the validated Company/Holding scope and delegates to a composition
//! executor that opens the caller's serializable unit.
use erpx_platform_runtime_server::public::http_support::{HeaderMap, Router, header};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use erpx_workflow_server::public::workflow_wave3::{
    WorkflowWave3Backend, WorkflowWave3Command, WorkflowWave3ExecuteBackend,
    WorkflowWave3ExecuteFuture, WorkflowWave3Future, WorkflowWave3Request, WorkflowWave3Target,
    plan_workflow_wave3, workflow_wave3_browser_router, workflow_wave3_company_router,
    workflow_wave3_execute_router, workflow_wave3_holding_router,
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

/// Composition backend that EXECUTES an owner write in Rust on a fresh
/// serializable unit. Used by synchronous native-write descriptors.
pub struct WorkflowWave3ExecuteBackendImpl {
    pool: erpx_platform_runtime_server::public::postgres::DatabasePool,
}

impl WorkflowWave3ExecuteBackendImpl {
    pub fn new(pool: erpx_platform_runtime_server::public::postgres::DatabasePool) -> Self {
        Self { pool }
    }
}

impl WorkflowWave3ExecuteBackend for WorkflowWave3ExecuteBackendImpl {
    fn execute<'a>(
        &'a self,
        kind: &'a str,
        values: &'a [serde_json::Value],
        now: i64,
        _key: &'a str,
    ) -> WorkflowWave3ExecuteFuture<'a> {
        Box::pin(async move {
            let command = WorkflowWave3Command {
                kind: kind.to_owned(),
                values: values.to_vec(),
                now,
            };
            let statement = plan_workflow_wave3(&command)
                .map_err(|e| BusinessError::new(FailureKind::InvalidInput, e.code, e.message))?;
            let mut connection = self.pool.raw().acquire().await.map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "WORKFLOW_WAVE3_UNAVAILABLE",
                    "The native Workflow store is unavailable.",
                )
            })?;
            // Render the planner's positional values as escaped SQL literals.
            // PostgreSQL then infers each target type from the statement, so a
            // nullable value can compare against either text or bigint columns,
            // matching the retired D1-style binder.
            let statement_sql = interpolate(&statement.sql, &statement.values)?;
            // The owner planner produced this statement from a bounded typed
            // kind and positional values; audit the finished text before executing.
            sqlx::query(sqlx::AssertSqlSafe(statement_sql))
                .execute(&mut *connection)
                .await
                .map_err(|_| {
                    BusinessError::new(
                        FailureKind::Unavailable,
                        "WORKFLOW_WAVE3_FAILED",
                        "The native Workflow write failed.",
                    )
                })?;
            Ok(serde_json::json!({"ok": true}))
        })
    }
}

fn quote(value: &str) -> Result<String, BusinessError> {
    if value.contains('\0') {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "WORKFLOW_WAVE3_WRITE_INVALID",
            "Review the Workflow owner write request.",
        ));
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

fn literal(value: &serde_json::Value) -> Result<String, BusinessError> {
    Ok(match value {
        serde_json::Value::Null => "NULL".to_owned(),
        serde_json::Value::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.to_owned(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text) => quote(text)?,
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => quote(&value.to_string())?,
    })
}

fn interpolate(sql: &str, values: &[serde_json::Value]) -> Result<String, BusinessError> {
    let mut rendered = String::with_capacity(sql.len() + values.len() * 8);
    let mut values = values.iter();
    for ch in sql.chars() {
        if ch == '?' {
            let value = values.next().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::InvalidInput,
                    "WORKFLOW_WAVE3_WRITE_INVALID",
                    "Review the Workflow owner write request.",
                )
            })?;
            rendered.push_str(&literal(value)?);
        } else {
            rendered.push(ch);
        }
    }
    if values.next().is_some() {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "WORKFLOW_WAVE3_WRITE_INVALID",
            "Review the Workflow owner write request.",
        ));
    }
    Ok(rendered)
}

/// Build the executing holding-keyed router from the configured pool.
pub fn workflow_wave3_execute(
    pool: erpx_platform_runtime_server::public::postgres::DatabasePool,
    holdings: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn WorkflowWave3ExecuteBackend> =
        Arc::new(WorkflowWave3ExecuteBackendImpl::new(pool));
    workflow_wave3_execute_router(
        holdings
            .into_iter()
            .map(|holding| (holding, backend.clone()))
            .collect(),
    )
}

/// Build the executing holding-keyed router with one pool per holding. The
/// route path is holding-parameterized, so all holding targets must live in a
/// single merged router; merging one router per holding panics on the
/// duplicate path. Each holding keeps its own pool-backed backend.
pub fn workflow_wave3_execute_pools(
    pairs: Vec<(String, erpx_platform_runtime_server::public::postgres::DatabasePool)>,
) -> Result<Router, BusinessError> {
    workflow_wave3_execute_router(
        pairs
            .into_iter()
            .map(|(holding, pool)| {
                (
                    holding,
                    Arc::new(WorkflowWave3ExecuteBackendImpl::new(pool))
                        as Arc<dyn WorkflowWave3ExecuteBackend>,
                )
            })
            .collect(),
    )
}
