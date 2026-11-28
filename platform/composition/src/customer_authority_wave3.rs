//! Composition backend for the wf3a13 native Customer Authority write routes.
//! Self-contained: the lead mounts the returned routers next to the other
//! Customer Authority routes. The owner crate owns the SQL; this module only
//! resolves the validated Company/Holding scope and delegates to a composition
//! executor that opens the caller's serializable unit.
use erpx_customer_authority_server::public::customer_authority_wave3::{
    CustomerAuthorityWave3Backend, CustomerAuthorityWave3Future, CustomerAuthorityWave3Request,
    CustomerAuthorityWave3Target, customer_authority_wave3_browser_router,
    customer_authority_wave3_company_router,
};
use erpx_platform_runtime_server::public::http_support::{HeaderMap, Router, header};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::sync::Arc;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_AUTHORITY_WAVE3_REQUEST_INVALID",
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

pub trait CustomerAuthorityWave3Executor: Send + Sync {
    fn execute<'a>(
        &'a self,
        scope: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
        key: &'a str,
    ) -> CustomerAuthorityWave3Future<'a>;
}

pub struct CustomerAuthorityWave3BackendAdapter {
    executor: Arc<dyn CustomerAuthorityWave3Executor>,
}

impl CustomerAuthorityWave3BackendAdapter {
    pub fn new(executor: Arc<dyn CustomerAuthorityWave3Executor>) -> Self {
        Self { executor }
    }
}

impl CustomerAuthorityWave3Backend for CustomerAuthorityWave3BackendAdapter {
    fn statement<'a>(
        &'a self,
        request: CustomerAuthorityWave3Request<'a>,
    ) -> CustomerAuthorityWave3Future<'a> {
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

pub fn customer_authority_wave3_browser(
    executor: Arc<dyn CustomerAuthorityWave3Executor>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn CustomerAuthorityWave3Backend> =
        Arc::new(CustomerAuthorityWave3BackendAdapter::new(executor));
    customer_authority_wave3_browser_router(
        targets
            .into_iter()
            .map(
                |(host, holding_slug, origin)| CustomerAuthorityWave3Target {
                    host,
                    holding_slug,
                    origin,
                    backend: backend.clone(),
                },
            )
            .collect(),
    )
}

pub fn customer_authority_wave3_company(
    executor: Arc<dyn CustomerAuthorityWave3Executor>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn CustomerAuthorityWave3Backend> =
        Arc::new(CustomerAuthorityWave3BackendAdapter::new(executor));
    customer_authority_wave3_company_router(
        companies
            .into_iter()
            .map(|company| (company, backend.clone()))
            .collect(),
    )
}
