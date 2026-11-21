//! Composition backend for the wf1a3 Files wave-1b native command router.
//!
//! Self-contained: it does not edit shared composition files. The lead adds
//! `mod files_wave1b;` and mounts `files_command_router` with exact registered
//! Holding targets. The owner crate owns all SQL and executes writes in Rust.
use erpx_server_files::public::files_wave1b::{
    CommandResult, FilesCommandInput, PostgresFilesCommandPort, unit_error, validate,
};
use erpx_server_files::public::files_wave1b_http::{
    FilesCommandBackend, FilesCommandFuture, FilesCommandRequest, FilesCommandTarget,
    files_command_router,
};
use erpx_server_runtime::public::http_support::{HeaderMap, Router, header};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::PgConnection;
use std::sync::Arc;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "FILES_WAVE1B_REQUEST_INVALID",
        "Review the Files command request.",
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

/// Resolve and validate the current Company from the native bridge header.
/// Fail-closed for missing/ambiguous context.
pub fn browser_company(headers: &HeaderMap) -> Result<String, BusinessError> {
    let company = unique_header(headers, header::HeaderName::from_static("x-erpx-context-company"))?;
    if company.is_empty() || company.len() > 200 {
        return Err(invalid());
    }
    Ok(company)
}

/// Executor owned by composition: opens the caller-owned serializable Holding
/// unit and invokes the owner adapter inside it.
pub trait FilesCommandExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        company_id: &'a str,
        key: &'a str,
        input: &'a FilesCommandInput,
    ) -> FilesCommandFuture<'a>;
}

pub struct FilesCommandBackendImpl {
    executor: Arc<dyn FilesCommandExecutor>,
}

impl FilesCommandBackendImpl {
    pub fn new(executor: Arc<dyn FilesCommandExecutor>) -> Self {
        Self { executor }
    }

    pub fn target(
        self: Arc<Self>,
        host: String,
        holding_slug: String,
        origin: String,
    ) -> FilesCommandTarget {
        FilesCommandTarget { host, holding_slug, origin, backend: self }
    }
}

impl FilesCommandBackend for FilesCommandBackendImpl {
    fn execute<'a>(&'a self, request: FilesCommandRequest<'a>) -> FilesCommandFuture<'a> {
        Box::pin(async move {
            let company = browser_company(request.headers)?;
            CompanyId::new(&company).map_err(|_| invalid())?;
            validate(request.input)?;
            self.executor.execute(&company, request.key.as_str(), request.input).await
        })
    }
}

/// Execute the validated owner write plus receipt/audit/outbox in the unit.
pub async fn execute_on_unit(
    connection: &mut PgConnection,
    company_id: &str,
    key: &str,
    input: &FilesCommandInput,
) -> Result<CommandResult, BusinessError> {
    let company = CompanyId::new(company_id).map_err(|_| invalid())?;
    let mut port = PostgresFilesCommandPort;
    port.execute(
        Some(connection),
        &company,
        key,
        &format!("files.command.{}", input.kind),
        &input.kind,
        &input.values,
        input.now,
    )
    .await
    .map_err(unit_error)
}

/// Build the exact registered-Holding router (lead supplies targets).
pub fn mounted_router(targets: Vec<FilesCommandTarget>) -> Result<Router, BusinessError> {
    files_command_router(targets)
}
