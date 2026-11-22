//! Composition for the native Manufacturing wave-3 command routers.
//!
//! This module is intentionally standalone: the lead mounts the routers built
//! by [`manufacturing_wave3_router`] and [`manufacturing_owner_write_router`]
//! into the private gateway and injects the backends. It resolves the caller
//! from the private gateway's context headers, opens one serializable Holding
//! transaction per command and injects the owner repositories. No SQL and no
//! business rules live here — the owner crates own both.
use erpx_server_manufacturing::public::accounting_profile_wave3::{
    PgProfileCommand, ProfileActor, create_profile, review_profile,
};
use erpx_server_manufacturing::public::accounting_profile_wave3_http::{
    ProfileHttpBackend, ProfileHttpFuture, ProfileHttpKind, ProfileHttpRequest,
    ProfileHttpResponse, ProfileHttpTarget, accounting_profile_wave3_http_router,
};
use erpx_server_manufacturing::public::owner_write_wave3::{
    PgOwnerWrite, company, execute_owner_write,
};
use erpx_server_manufacturing::public::owner_write_wave3_http::{
    OwnerWriteBackend, OwnerWriteFuture, OwnerWriteRequest, OwnerWriteResponse, OwnerWriteTarget,
    manufacturing_owner_write_router as manufacturing_owner_write_router_impl,
};
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_PERMISSIONS: HeaderName = HeaderName::from_static("x-erpx-context-permissions");
const CONTEXT_MODULES: HeaderName = HeaderName::from_static("x-erpx-context-modules");

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_ACCOUNTING_REQUEST_INVALID",
        "Use one bounded accounting profile command.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_ACCOUNTING_UNAVAILABLE",
        "The native Manufacturing accounting service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn one(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 200)
        .map(str::to_owned)
        .ok_or_else(malformed)
}

fn list(headers: &HeaderMap, name: HeaderName) -> Vec<String> {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 100)
        .map(str::to_owned)
        .take(64)
        .collect()
}

/// Composition-owned backend. `control` is the Holding Postgres pool that owns
/// `manufacturing_accounting_profiles`; `clock` supplies the committed
/// timestamp. The Company comes from the gateway's signed context header so one
/// Holding target serves every Company in that Holding.
pub struct ManufacturingProfileBackend<C> {
    control: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ManufacturingProfileBackend<C> {
    pub fn new(control: DatabasePool, clock: Arc<C>) -> Self {
        Self { control, clock }
    }

    fn actor(headers: &HeaderMap) -> Result<ProfileActor, BusinessError> {
        let user_id = one(headers, CONTEXT_USER)?;
        let company_id = one(headers, CONTEXT_COMPANY)?;
        let assignment_id = one(headers, CONTEXT_ASSIGNMENT)?;
        let permissions = list(headers, CONTEXT_PERMISSIONS);
        let modules = list(headers, CONTEXT_MODULES);
        if permissions.is_empty() || modules.is_empty() {
            return Err(BusinessError::new(
                FailureKind::Unauthenticated,
                "SIGN_IN_REQUIRED",
                "Sign in to manage the production accounting profile.",
            ));
        }
        let authority_json = serde_json::json!({
            "company_id": company_id,
            "permissions": permissions,
            "modules": modules,
        })
        .to_string();
        Ok(ProfileActor {
            user_id,
            company_id,
            branch_id: None,
            department_id: None,
            assignment_id,
            authority_json,
        })
    }
}

impl<C: Clock + Send + Sync + 'static> ProfileHttpBackend for ManufacturingProfileBackend<C> {
    fn command<'a>(
        &'a self,
        request: ProfileHttpRequest<'a>,
    ) -> ProfileHttpFuture<'a, ProfileHttpResponse> {
        Box::pin(async move {
            let actor = Self::actor(request.headers)?;
            let body = request.body.clone();
            let key = request.key.cloned();
            let kind = request.kind.clone();
            let scope = actor.company_id.clone();
            let clock = self.clock.clone();
            let control = self.control.clone();
            let data = PgUnitOfWork::new(control)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let body = body.clone();
                    let key = key.clone();
                    let kind = kind.clone();
                    let scope = scope.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let mut repository = PgProfileCommand::new(&mut *connection, &scope);
                        let result = match kind {
                            ProfileHttpKind::Create => {
                                create_profile(
                                    &actor,
                                    format!(
                                        "mfg-accounting-{}",
                                        key.as_ref().map(CommandKey::as_str).unwrap_or("command")
                                    ),
                                    &body,
                                    key.as_ref(),
                                    &mut repository,
                                    clock.now(),
                                )
                                .await?
                            }
                            ProfileHttpKind::Review { profile_id } => {
                                review_profile(
                                    &actor,
                                    &profile_id,
                                    &body,
                                    key.as_ref(),
                                    &mut repository,
                                    clock.now(),
                                )
                                .await?
                            }
                        };
                        drop(repository);
                        Ok(result)
                    })
                })
                .await
                .map_err(business)?;
            Ok(ProfileHttpResponse { data })
        })
    }
}

/// Build the owner router bound to a backend. The lead mounts the returned
/// Router; no route or SQL changes are needed in the gateway.
pub fn manufacturing_wave3_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn ProfileHttpBackend>,
) -> Result<Router, BusinessError> {
    accounting_profile_wave3_http_router(vec![ProfileHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}

/// Composition backend for the single Manufacturing owner-write axum router.
/// The owner crate selects the authoritative SQL for a typed `kind`; this
/// backend only resolves the current caller/Company and opens the caller-owned
/// serializable unit. Receipt/audit/outbox are written there by the owner
/// executor in the same transaction.
pub struct ManufacturingOwnerWriteBackend<C> {
    holding: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ManufacturingOwnerWriteBackend<C> {
    pub fn new(holding: DatabasePool, clock: Arc<C>) -> Self {
        Self { holding, clock }
    }

    fn caller(headers: &HeaderMap) -> Result<String, BusinessError> {
        let mut values = headers.get_all(CONTEXT_USER).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(malformed());
        }
        first
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .map(str::to_owned)
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unauthenticated,
                    "SIGN_IN_REQUIRED",
                    "Sign in to change Manufacturing data.",
                )
            })
    }
}

impl<C: Clock + Send + Sync + 'static> OwnerWriteBackend for ManufacturingOwnerWriteBackend<C> {
    fn write<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a, OwnerWriteResponse> {
        Box::pin(async move {
            let actor = Self::caller(request.headers)?;
            let scope = company(request.body)?;
            let body = request.body.clone();
            let key = request.key.cloned();
            let holding = self.holding.clone();
            let clock = self.clock.clone();
            let data = PgUnitOfWork::new(holding)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let body = body.clone();
                    let key = key.clone();
                    let scope = scope.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let mut repository = PgOwnerWrite::new(&mut *connection, &scope);
                        let receipt = execute_owner_write(
                            &actor,
                            &body,
                            key.as_ref(),
                            &mut repository,
                            clock.now(),
                        )
                        .await?;
                        drop(repository);
                        Ok(receipt)
                    })
                })
                .await
                .map_err(business)?;
            Ok(OwnerWriteResponse { data })
        })
    }
}

/// Build the single owner-write router bound to a backend. The lead mounts it
/// alongside the other Manufacturing routes; no gateway SQL or TS writer remains.
pub fn manufacturing_owner_write_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn OwnerWriteBackend>,
) -> Result<Router, BusinessError> {
    manufacturing_owner_write_router_impl(vec![OwnerWriteTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
