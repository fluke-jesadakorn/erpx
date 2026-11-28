//! Wave-2 composition for the native marketing command executor.
//! Resolves the private caller from internal context headers, opens one
//! serializable unit on the owning pool, executes the admitted owner statement
//! and commits the receipt/audit/outbox ledger in that same unit.
use erpx_marketing_server::public::native_command::{
    NativeCommand, PgNativeCommandRepository, admit, execute_native_command,
};
use erpx_marketing_server::public::native_command_http::{
    NativeCommandHttpBackend, NativeCommandHttpFuture, NativeCommandHttpRequest,
    NativeCommandHttpTarget, native_command_router, native_company_command_router,
};
use erpx_platform_runtime_server::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind};
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
        "MARKETING_NATIVE_COMMAND_UNAVAILABLE",
        "The native marketing service is unavailable.",
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
            "MARKETING_NATIVE_COMMAND_REQUEST_INVALID",
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
    .bind(format!("marketing-native-audit-{key}")).bind(key).bind(&caller.user_id).bind(&caller.assignment_id)
    .bind(&record_type).bind(kind).bind(1_i64).bind(now).bind(kind)
    .execute(&mut *connection).await?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("marketing-native-outbox-{key}")).bind(key).bind(kind).bind(&record_type).bind(kind).bind(1_i64).bind(&result).bind(now)
    .execute(&mut *connection).await?;
    Ok(())
}

/// Composition-owned backend. `holding` owns owner rows; `control` owns the
/// Control-scoped statements. `clock` supplies the committed timestamp.
pub struct MarketingNativeBackend<C> {
    holding: DatabasePool,
    control: DatabasePool,
    companies: std::collections::BTreeMap<String, DatabasePool>,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> MarketingNativeBackend<C> {
    pub fn new(holding: DatabasePool, control: DatabasePool, clock: Arc<C>) -> Self {
        Self {
            holding,
            control,
            companies: std::collections::BTreeMap::new(),
            clock,
        }
    }

    /// Worker path: resolve the owner pool by the admitted Company header.
    pub fn with_companies(
        mut self,
        companies: std::collections::BTreeMap<String, DatabasePool>,
    ) -> Self {
        self.companies = companies;
        self
    }
}

impl<C: Clock + Send + Sync + 'static> NativeCommandHttpBackend for MarketingNativeBackend<C> {
    fn execute<'a>(&'a self, request: NativeCommandHttpRequest<'a>) -> NativeCommandHttpFuture<'a> {
        Box::pin(async move {
            let caller = caller(request.headers)?;
            let command = NativeCommand {
                kind: request.command.kind.clone(),
                values: request.command.values.clone(),
                now: request.command.now,
            };
            let key = request.key.to_owned();
            let kind = command.kind.clone();
            let now = if command.now > 0 {
                command.now
            } else {
                self.clock.now().get()
            };
            let scope = admit(&command)?.scope.to_owned();
            let pool = if scope == "control" {
                self.control.clone()
            } else {
                self.companies
                    .get(&caller.company_id)
                    .cloned()
                    .unwrap_or_else(|| self.holding.clone())
            };
            let company_label = caller.company_id.clone();
            let write_command = command.clone();
            let write_key = key.clone();
            let (rows, result) = PgUnitOfWork::new(pool)
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
                        // The statement's RETURNING rows travel to the TS client
                        // as `data.rows`; the public rate limiter reads count
                        // from the first returned row. The ledger keeps the
                        // numeric row count as its existing receipt evidence.
                        let returned = outcome.rows;
                        let count = returned.len() as u64;
                        ledger(connection, &caller, &kind, &write_key, now, count)
                            .await
                            .map_err(business)?;
                        Ok((count, json!({"kind": kind, "rows": returned})))
                    })
                })
                .await
                .map_err(business)?;
            let _ = rows;
            Ok(json!({"data": result, "context": {"companyId": company_label}}))
        })
    }
}

/// Browser-facing router bound to this backend. Lead mounts the result.
pub fn marketing_native_router(
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
/// Private Company-keyed router bound to this backend.
pub fn marketing_native_company_router(
    companies: Vec<String>,
    backend: Arc<dyn NativeCommandHttpBackend>,
) -> Result<Router, BusinessError> {
    native_company_command_router(companies, backend)
}
