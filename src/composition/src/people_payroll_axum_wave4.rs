//! Composition for the native people-payroll command HTTP routes (Wave 4).
//!
//! Self-contained and intentionally does not edit the shared composition files;
//! the lead mounts the routers built here alongside the other native routes.
//! The owner crate (`erpx-server-people-payroll`) owns the domain, use case, the
//! only SQL adapter and the axum ingress. This module resolves the current
//! Company/actor from the private caller's bounded context headers, opens the
//! caller-owned Holding unit for the write and the Control unit for the
//! receipt/audit/outbox ledger. No SQL for business rows and no authority
//! minting lives here.
use erpx_server_people_payroll::public::native_command::{
    PgNativeCommandRepository, execute_native_command,
};
use erpx_server_people_payroll::public::native_command_http::{
    NativeCommandHttpBackend, NativeCommandHttpFuture, NativeCommandHttpRequest,
    NativeCommandHttpResponse, NativeCommandHttpTarget, native_command_company_router,
    native_command_router,
};
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind};
use serde_json::json;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const REVISION: HeaderName = HeaderName::from_static("x-erpx-context-revision");
const ASSIGNMENT_VERSION: HeaderName = HeaderName::from_static("x-erpx-context-assignment-version");

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_NATIVE_UNAVAILABLE",
        "The native people-payroll service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn unique(headers: &HeaderMap, name: HeaderName) -> Result<Option<String>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values
        .next()
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if values.next().is_some() {
        return Err(erpx_server_people_payroll::public::native_command::invalid());
    }
    Ok(first)
}

/// Bounded current-caller context. The private gateway maps the Account session
/// into these headers; a missing or ambiguous value fails closed.
#[derive(Clone, Debug)]
struct Caller {
    company_id: String,
    user_id: String,
    assignment_id: String,
    revision: String,
    assignment_version: String,
}

fn caller(headers: &HeaderMap) -> Result<Caller, BusinessError> {
    let company_id = unique(headers, COMPANY.clone())?
        .ok_or_else(erpx_server_people_payroll::public::native_command::context_invalid)?;
    CompanyId::new(company_id.clone())
        .map_err(|_| erpx_server_people_payroll::public::native_command::context_invalid())?;
    let user_id = unique(headers, USER.clone())?.unwrap_or_default();
    let assignment_id = unique(headers, ASSIGNMENT.clone())?.unwrap_or_default();
    let revision = unique(headers, REVISION.clone())?.unwrap_or_default();
    let assignment_version = unique(headers, ASSIGNMENT_VERSION.clone())?.unwrap_or_default();
    for value in [&user_id, &assignment_id, &revision, &assignment_version] {
        if value.len() > 200 {
            return Err(erpx_server_people_payroll::public::native_command::invalid());
        }
    }
    Ok(Caller {
        company_id,
        user_id,
        assignment_id,
        revision,
        assignment_version,
    })
}

/// Composition-owned backend. `holding` owns the people-payroll rows; `control`
/// owns the command receipt, audit and outbox retention.
pub struct PeoplePayrollNativeBackend<C> {
    holding: DatabasePool,
    control: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PeoplePayrollNativeBackend<C> {
    pub fn new(holding: DatabasePool, control: DatabasePool, clock: Arc<C>) -> Self {
        Self {
            holding,
            control,
            clock,
        }
    }
}

/// Receipt/audit/outbox committed in the caller's Control unit. The owner write
/// has already committed in the Holding unit; the command key correlates them.
async fn ledger(
    connection: &mut PgConnection,
    caller: &Caller,
    kind: &str,
    key: &str,
    now: i64,
    rows: u64,
) -> Result<(), Error> {
    let authority = format!(
        "{{\"companyId\":{},\"assignmentId\":{},\"revision\":{},\"assignmentVersion\":{}}}",
        json!(caller.company_id),
        json!(caller.assignment_id),
        json!(caller.revision),
        json!(caller.assignment_version),
    );
    let result = json!({"kind": kind, "rows": rows}).to_string();

    let stored = sqlx::query_scalar::<_, String>(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO UPDATE SET command_id=excluded.command_id RETURNING request_hash",
    )
    .bind(key)
    .bind(kind)
    .bind(caller.user_id.as_str())
    .bind(key)
    .bind(kind)
    .bind(&result)
    .bind(now)
    .bind(&authority)
    .fetch_one(&mut *connection)
    .await?;
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
         VALUES($1,$2,$3,$4,$9,$5,$6,$7,$9,$8,(SELECT authority_json FROM command_receipts WHERE command_id=$2))",
    )
    .bind(format!("pp-native-audit-{key}"))
    .bind(key)
    .bind(caller.user_id.as_str())
    .bind(caller.assignment_id.as_str())
    .bind(&record_type)
    .bind(kind)
    .bind(1_i64)
    .bind(now)
    .bind(kind)
    .execute(&mut *connection)
    .await?;

    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("pp-native-outbox-{key}"))
    .bind(key)
    .bind(kind)
    .bind(&record_type)
    .bind(kind)
    .bind(1_i64)
    .bind(&result)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

impl<C: Clock + Send + Sync + 'static> NativeCommandHttpBackend for PeoplePayrollNativeBackend<C> {
    fn execute<'a>(&'a self, request: NativeCommandHttpRequest<'a>) -> NativeCommandHttpFuture<'a> {
        Box::pin(async move {
            let caller = caller(request.headers)?;
            let command = request.command.clone();
            let key = request.key.as_str().to_owned();
            let kind = command.kind.clone();
            let now = if command.now > 0 {
                command.now
            } else {
                self.clock.now().get()
            };
            let holding = self.holding.clone();

            // Distinct clones so the write closure cannot move values that the
            // ledger phase still needs.
            let write_command = command.clone();
            let write_key = key.clone();
            let rows = PgUnitOfWork::new(holding)
                .run_once::<u64, BusinessError, _>(
                    TransactionOptions::default(),
                    move |connection| {
                        let write_command = write_command.clone();
                        let write_key = write_key.clone();
                        Box::pin(async move {
                            let mut repository = PgNativeCommandRepository::new(connection);
                            let outcome =
                                execute_native_command(&write_command, &write_key, &mut repository)
                                    .await?;
                            Ok(outcome.rows)
                        })
                    },
                )
                .await
                .map_err(business)?;

            let control = self.control.clone();
            let ledger_caller = caller.clone();
            let ledger_kind = kind.clone();
            let ledger_key = key.clone();
            PgUnitOfWork::new(control)
                .run_once::<(), BusinessError, _>(
                    TransactionOptions::default(),
                    move |connection| {
                        let ledger_caller = ledger_caller.clone();
                        let ledger_kind = ledger_kind.clone();
                        let ledger_key = ledger_key.clone();
                        Box::pin(async move {
                            ledger(
                                connection,
                                &ledger_caller,
                                &ledger_kind,
                                &ledger_key,
                                now,
                                rows,
                            )
                            .await
                        })
                    },
                )
                .await
                .map_err(business)?;

            Ok(NativeCommandHttpResponse {
                data: json!({"kind": kind, "rows": rows}),
                context: json!({"companyId": caller.company_id}),
            })
        })
    }
}

/// Build the browser-facing router bound to a backend. Lead mounts the result.
pub fn people_payroll_native_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn NativeCommandHttpBackend>,
) -> Result<Router, BusinessError> {
    native_command_router(vec![NativeCommandHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}

/// Build the private Company-keyed system router bound to a backend.
pub fn people_payroll_native_company_router(
    companies: Vec<String>,
    backend: Arc<dyn NativeCommandHttpBackend>,
) -> Result<Router, BusinessError> {
    let pairs = companies
        .into_iter()
        .map(|company| (company, backend.clone()))
        .collect();
    native_command_company_router(pairs)
}
