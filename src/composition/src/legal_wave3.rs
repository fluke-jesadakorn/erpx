//! Composition backend for the wf3a13 native Legal write routes.
//!
//! This module is self-contained: it never edits the shared composition files.
//! The lead mounts `legal_wave3_browser` and `legal_wave3_system` alongside the
//! other Legal routes, supplying the current-actor executor and exact
//! registered Holding targets. The owner crate owns the SQL
//! (`erpx_server_legal::public::legal_wave3`); this module resolves the current
//! Company/Holding and delegates to a composition executor that opens the
//! caller's serializable unit. Receipt/audit/outbox effects stay in that unit.
use erpx_server_legal::public::legal_wave3::{
    LegalWave3Backend, LegalWave3Future, LegalWave3Request, LegalWave3Statement, LegalWave3Target,
    legal_wave3_browser_router, legal_wave3_company_router, legal_wave3_holding_router,
};
use erpx_server_runtime::public::http_support::{HeaderMap, Router, header};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::sync::Arc;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "LEGAL_WAVE3_REQUEST_INVALID",
        "Review the Legal owner write request.",
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

/// Composition-owned executor. It resolves the validated scope, opens the
/// already-authorized serializable unit and builds the pure owner statement.
pub trait LegalWave3Executor: Send + Sync {
    fn execute<'a>(
        &'a self,
        scope: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
        key: &'a str,
    ) -> LegalWave3Future<'a>;
}

/// Adapter binding the HTTP ingress to a composition executor. Scope is read
/// only from validated private-gateway context headers, never from the body.
pub struct LegalWave3BackendAdapter {
    executor: Arc<dyn LegalWave3Executor>,
}

impl LegalWave3BackendAdapter {
    pub fn new(executor: Arc<dyn LegalWave3Executor>) -> Self {
        Self { executor }
    }
}

impl LegalWave3Backend for LegalWave3BackendAdapter {
    fn statement<'a>(&'a self, request: LegalWave3Request<'a>) -> LegalWave3Future<'a> {
        Box::pin(async move {
            let scope = if !request.holding_slug.is_empty() {
                unique_header(
                    request.headers,
                    header::HeaderName::from_static("x-erpx-context-holding"),
                )?
            } else {
                let holding = unique_header(
                    request.headers,
                    header::HeaderName::from_static("x-erpx-context-holding"),
                )
                .unwrap_or_default();
                if !holding.is_empty() {
                    holding
                } else {
                    unique_header(
                        request.headers,
                        header::HeaderName::from_static("x-erpx-context-company"),
                    )?
                }
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

/// Build the browser-facing router with exact registered Holding targets.
pub fn legal_wave3_browser(
    executor: Arc<dyn LegalWave3Executor>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn LegalWave3Backend> = Arc::new(LegalWave3BackendAdapter::new(executor));
    legal_wave3_browser_router(
        targets
            .into_iter()
            .map(|(host, holding_slug, origin)| LegalWave3Target {
                host,
                holding_slug,
                origin,
                backend: backend.clone(),
            })
            .collect(),
    )
}

/// Build the private-API system routers keyed by exact Holding or Company id.
pub fn legal_wave3_system(
    executor: Arc<dyn LegalWave3Executor>,
    holdings: Vec<String>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn LegalWave3Backend> = Arc::new(LegalWave3BackendAdapter::new(executor));
    let holding_router = legal_wave3_holding_router(
        holdings
            .into_iter()
            .map(|holding| (holding, backend.clone()))
            .collect(),
    )?;
    let company_router = legal_wave3_company_router(
        companies
            .into_iter()
            .map(|company| (company, backend.clone()))
            .collect(),
    )?;
    Ok(Router::new().merge(holding_router).merge(company_router))
}

/// Re-export the owner statement type for concrete executors.
pub type OwnerStatement = LegalWave3Statement;
