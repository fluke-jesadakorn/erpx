//! Composition backend for the wf1a7 native Collaboration owner-write routes.
//!
//! This module is self-contained and intentionally does not edit the shared
//! composition files. The lead mounts `wf1a7_router` / `wf1a7_company_router`
//! alongside the other Collaboration routes, supplying the current-actor
//! executor and exact registered Holding targets. The owner crate owns the SQL
//! (`erpx_server_collaboration::adapters::wf1a7`); this module only resolves the
//! current Company/authority and opens the caller-owned serializable unit.
use erpx_server_collaboration::public::notification_statement::{
    NotificationStatement, NotificationStatementInput,
};
use erpx_server_collaboration::public::wf1a7_http::{
    OwnerWriteBackend, OwnerWriteFuture, OwnerWriteRequest, OwnerWriteTarget,
};
use erpx_server_runtime::public::http_support::{HeaderMap, Router, header};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "COLLABORATION_OWNER_WRITE_INVALID",
        "Review the owner write request.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COLLABORATION_UNAVAILABLE",
        "The native Collaboration service is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
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
            "COLLABORATION_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    Ok(first)
}

/// Resolve and validate the current Company for a browser write. The native
/// bridges always send `x-erpx-context-company`; the concrete composition
/// account policy must additionally verify the current WorkContext/assignment
/// before the write is admitted. This default resolution is fail-closed for
/// any missing or ambiguous context header.
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
/// serializable unit. Receipt/audit/outbox effects belong to the same unit and
/// are written by the caller, never here.
pub async fn execute_on_unit(
    connection: &mut PgConnection,
    company_id: &str,
    kind: &str,
    values: Vec<Value>,
    now: i64,
    key: &str,
    predicate: Option<&str>,
    target: Option<&str>,
) -> Result<NotificationStatement, BusinessError> {
    let company = CompanyId::new(company_id).map_err(|_| invalid())?;
    let input = NotificationStatementInput {
        company_id: company_id.to_owned(),
        // Browser jobs and system workers may not carry a user id; the owner
        // statement selects all columns from the caller-owned values array.
        user_id: String::new(),
        record_type: String::new(),
        record_id: String::new(),
        record_version: 1,
        message: String::new(),
        now: if now > 0 { now } else { 1 },
        idempotency_key: key.to_owned(),
        kind: Some(kind.to_owned()),
        lead_id: None,
        identifier: None,
        approver_user_id: None,
        item_id: None,
        bill_id: None,
        rfq_id: None,
        audience: None,
        event: None,
        order_id: None,
        change: None,
        timestamp: None,
        post_id: None,
        source_hash: None,
        ai_draft_id: None,
        document_table: None,
        recipient_sql: None,
        node_id: None,
        scope_node_id: None,
        runtime_predicate: None,
        table_name: None,
        cte_prefix: None,
        source_access_sql: None,
        target_kind: None,
        target_id: None,
        values_json: Some(serde_json::to_string(&values).map_err(|_| invalid())?),
    };
    let predicate_owned = predicate.map(str::to_owned);
    let target_owned = target.map(str::to_owned);
    erpx_server_collaboration::public::wf1a7::wf1a7_statement(
        connection,
        &company,
        &input,
        predicate_owned.as_deref(),
        target_owned.as_deref(),
    )
    .await
    .map_err(business)
}

/// Executor owned by composition: receives a validated Company and typed write
/// and runs it inside the already-opened serializable unit.
pub trait OwnerWriteExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        company_id: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
        key: &'a str,
        predicate: Option<&'a str>,
        target: Option<&'a str>,
    ) -> OwnerWriteFuture<'a>;
}

/// Adapter binding the owner-write HTTP ingress to a composition executor.
pub struct Wf1a7Backend {
    executor: Arc<dyn OwnerWriteExecutor>,
}

impl Wf1a7Backend {
    pub fn new(executor: Arc<dyn OwnerWriteExecutor>) -> Self {
        Self { executor }
    }
}

impl OwnerWriteBackend for Wf1a7Backend {
    fn statement<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a> {
        Box::pin(async move {
            let company = if request.system || request.holding_slug.is_empty() {
                unique_header(
                    request.headers,
                    header::HeaderName::from_static("x-erpx-context-company"),
                )?
            } else {
                browser_company(request.headers)?
            };
            self.executor
                .execute(
                    &company,
                    &request.command.kind,
                    &request.command.values,
                    request.command.now,
                    request.key,
                    request.command.predicate.as_deref(),
                    request.command.target.as_deref(),
                )
                .await
        })
    }
}

/// Build the browser-facing owner-write router with exact registered targets.
pub fn wf1a7_router(
    executor: Arc<dyn OwnerWriteExecutor>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn OwnerWriteBackend> = Arc::new(Wf1a7Backend::new(executor));
    let targets = targets
        .into_iter()
        .map(|(host, holding_slug, origin)| OwnerWriteTarget {
            host,
            holding_slug,
            origin,
            backend: backend.clone(),
        })
        .collect();
    erpx_server_collaboration::public::wf1a7_http::owner_write_router(targets)
}

/// Build the private system owner-write router for self-guarded facts writes.
pub fn wf1a7_system_router(executor: Arc<dyn OwnerWriteExecutor>) -> Router {
    let backend: Arc<dyn OwnerWriteBackend> = Arc::new(Wf1a7Backend::new(executor));
    erpx_server_collaboration::public::wf1a7_http::owner_write_system_router(backend)
}

/// Build the Company-keyed system owner-write router.
pub fn wf1a7_company_router(
    executor: Arc<dyn OwnerWriteExecutor>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn OwnerWriteBackend> = Arc::new(Wf1a7Backend::new(executor));
    let pairs = companies
        .into_iter()
        .map(|company| (company, backend.clone()))
        .collect();
    erpx_server_collaboration::public::wf1a7_http::owner_write_company_router(pairs)
}

/// Holding-scoped system backend for the workbook/job workers. The caller
/// sends `x-erpx-context-holding`; each Holding keeps its own serializable
/// unit so a background write can never land in another Holding.
pub struct Wf1a7HoldingSystemBackend {
    pools: std::collections::BTreeMap<String, erpx_server_runtime::public::postgres::DatabasePool>,
}

impl Wf1a7HoldingSystemBackend {
    pub fn new(
        pools: std::collections::BTreeMap<String, erpx_server_runtime::public::postgres::DatabasePool>,
    ) -> Self {
        Self { pools }
    }
}

impl OwnerWriteBackend for Wf1a7HoldingSystemBackend {
    fn statement<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a> {
        Box::pin(async move {
            let holding = request
                .headers
                .get(header::HeaderName::from_static("x-erpx-context-holding"))
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(invalid)?
                .to_owned();
            let company = request
                .headers
                .get(header::HeaderName::from_static("x-erpx-context-company"))
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .unwrap_or("owner-system")
                .to_owned();
            let pool = self.pools.get(&holding).cloned().ok_or_else(unavailable)?;
            let kind = request.command.kind.clone();
            let values = request.command.values.clone();
            let now = request.command.now;
            let key = request.key.to_owned();
            let predicate = request.command.predicate.clone();
            let target = request.command.target.clone();
            use erpx_server_runtime::public::postgres::{
                PgUnitOfWork, TransactionOptions,
            };
            PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company = company.clone();
                    let kind = kind.clone();
                    let values = values.clone();
                    let key = key.clone();
                    let predicate = predicate.clone();
                    let target = target.clone();
                    Box::pin(async move {
                        execute_on_unit(
                            connection,
                            &company,
                            &kind,
                            values,
                            now,
                            &key,
                            predicate.as_deref(),
                            target.as_deref(),
                        )
                        .await
                        .map_err(PgTransactionError::Business)
                    })
                })
                .await
                .map_err(business)
        })
    }
}

/// Mount the private system owner-write route with one Holding pool per target.
pub fn wf1a7_holding_system_router(
    pools: std::collections::BTreeMap<String, erpx_server_runtime::public::postgres::DatabasePool>,
) -> Router {
    let backend: Arc<dyn OwnerWriteBackend> = Arc::new(Wf1a7HoldingSystemBackend::new(pools));
    erpx_server_collaboration::public::wf1a7_http::owner_write_system_router(backend)
}
