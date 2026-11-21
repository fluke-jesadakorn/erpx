//! Inventory wave-2 native retirement slice.
//!
//! This module owns the composition-side binding for every Inventory write
//! statement retired from TypeScript. The owner crate holds the typed kind
//! registry, the pure planner, the application use case and the Postgres
//! adapter; this module resolves the private caller from internal context
//! headers, opens ONE serializable unit on the owning pool, executes the
//! admitted owner statement and commits the receipt/audit/outbox ledger in
//! that same unit. No TypeScript writer or SQL mirror remains on this path.
use erpx_server_inventory::public::native_command::{
    PgNativeCommandRepository, admit, execute_native_command,
};
use erpx_server_inventory::public::native_command_http::{
    NativeCommand, NativeCommandHttpBackend, NativeCommandHttpFuture, NativeCommandHttpRequest,
    NativeCommandHttpTarget, native_command_router, native_company_command_router,
};
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde_json::json;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_REVISION: HeaderName = HeaderName::from_static("x-erpx-context-revision");
const CONTEXT_ASSIGNMENT_VERSION: HeaderName =
    HeaderName::from_static("x-erpx-context-assignment-version");

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_NATIVE_COMMAND_UNAVAILABLE",
        "The native Inventory service is unavailable.",
    )
}
fn business(e: Error) -> BusinessError {
    match e {
        PgTransactionError::Business(e) => e,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Debug)]
struct Caller {
    user_id: String,
    company_id: String,
    assignment_id: String,
    revision: i64,
    assignment_version: i64,
}

fn header(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::invalid(
            "INVENTORY_NATIVE_COMMAND_REQUEST_INVALID",
            "Use one bounded native command.",
        ));
    }
    first
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 200)
        .map(str::to_owned)
        .ok_or_else(unavailable)
}
fn caller(headers: &HeaderMap) -> Result<Caller, BusinessError> {
    Ok(Caller {
        user_id: header(headers, CONTEXT_USER).unwrap_or_default(),
        company_id: header(headers, CONTEXT_COMPANY)?,
        assignment_id: header(headers, CONTEXT_ASSIGNMENT).unwrap_or_else(|_| "system".to_owned()),
        revision: header(headers, CONTEXT_REVISION)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        assignment_version: header(headers, CONTEXT_ASSIGNMENT_VERSION)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1),
    })
}

async fn ledger(
    connection: &mut PgConnection,
    caller: &Caller,
    kind: &str,
    key: &str,
    now: i64,
    rows: u64,
) -> Result<(), Error> {
    let authority = json!({"companyId": caller.company_id, "assignmentId": caller.assignment_id, "revision": caller.revision, "assignmentVersion": caller.assignment_version}).to_string();
    let result = json!({"kind": kind, "rows": rows}).to_string();
    let stored = sqlx::query_scalar::<_, String>(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO UPDATE SET command_id=excluded.command_id RETURNING request_hash",
    )
    .bind(key).bind(kind).bind(&caller.user_id).bind(key).bind(kind).bind(&result).bind(now).bind(&authority)
    .fetch_one(&mut *connection).await?;
    if stored != key {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        )
        .into());
    }
    let record_type = kind.split('_').next().unwrap_or(kind).to_owned();
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$9,$5,$6,$7,$9,$8,(SELECT authority_json FROM command_receipts WHERE command_id=$2)) ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("inventory-native-audit-{key}")).bind(key).bind(&caller.user_id).bind(&caller.assignment_id)
    .bind(&record_type).bind(kind).bind(1_i64).bind(now).bind(kind)
    .execute(&mut *connection).await?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("inventory-native-outbox-{key}")).bind(key).bind(kind).bind(&record_type).bind(kind).bind(1_i64).bind(&result).bind(now)
    .execute(&mut *connection).await?;
    Ok(())
}

/// Composition-owned backend: `holding` owns owner rows, `clock` supplies the
/// committed timestamp.
pub struct InventoryNativeBackend<C> {
    holding: DatabasePool,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> InventoryNativeBackend<C> {
    pub fn new(holding: DatabasePool, clock: Arc<C>) -> Self {
        Self { holding, clock }
    }
}

impl<C: Clock + Send + Sync + 'static> NativeCommandHttpBackend for InventoryNativeBackend<C> {
    fn execute<'a>(&'a self, request: NativeCommandHttpRequest<'a>) -> NativeCommandHttpFuture<'a> {
        Box::pin(async move {
            let caller = caller(request.headers)?;
            let command: NativeCommand = request.command.clone();
            let key = request.key.to_owned();
            let kind = command.kind.clone();
            let now = if command.now > 0 {
                command.now
            } else {
                self.clock.now().get()
            };
            admit(&command)?;
            let write_command = command.clone();
            let write_key = key.clone();
            let rows = PgUnitOfWork::new(self.holding.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let write_command = write_command.clone();
                    let write_key = write_key.clone();
                    let caller = caller.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        let mut repository = PgNativeCommandRepository::new(connection);
                        let outcome =
                            execute_native_command(&write_command, &write_key, &mut repository)
                                .await?;
                        drop(repository);
                        ledger(connection, &caller, &kind, &write_key, now, outcome.rows)
                            .await
                            .map_err(business)?;
                        Ok(outcome.rows)
                    })
                })
                .await
                .map_err(business)?;
            Ok(json!({"data": {"kind": kind, "rows": rows}, "context": {"companyId": caller.company_id}}))
        })
    }
}

/// Browser/Waiter-facing native command router bound to this backend. Lead mounts the result.
pub fn inventory_wave2_native_router(
    host: impl Into<String>,
    slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn NativeCommandHttpBackend>,
) -> Result<Router, BusinessError> {
    native_command_router(vec![NativeCommandHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: origin.into(),
        backend,
    }])
}

/// Private Company-keyed router for workers without a browser WorkContext.
pub fn inventory_wave2_company_router(
    companies: Vec<String>,
    backend: Arc<dyn NativeCommandHttpBackend>,
) -> Result<Router, BusinessError> {
    native_company_command_router(companies, backend)
}
