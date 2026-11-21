//! Composition for the native Account time-preference command HTTP route.
//!
//! This module is intentionally standalone: the lead mounts the router built by
//! [`account_time_preference_router`] into the private gateway and injects the
//! backend built by [`AccountTimePreferenceBackend::new`]. It resolves the caller
//! identity from the private caller's `x-erpx-context-user` header (the public
//! Holding route maps the session to that header upstream), opens one
//! serializable Control transaction per command, and injects the owner adapter.
//! No SQL and no business rules live here.
use erpx_server_identity_access::public::account_time_preference::{
    PgTimePreferenceCommand, change_time_preference,
};
use erpx_server_identity_access::public::account_time_preference_http::{
    AccountTimePreferenceHttpBackend, AccountTimePreferenceHttpFuture,
    AccountTimePreferenceHttpKind, AccountTimePreferenceHttpRequest,
    AccountTimePreferenceHttpResponse, AccountTimePreferenceHttpTarget,
    account_time_preference_http_router,
};
use erpx_server_identity_access::public::identity_write::execute as execute_identity_write;
use erpx_server_identity_access::public::identity_write_http::{
    IdentityWriteBackend, IdentityWriteFuture, IdentityWriteRequest, IdentityWriteTarget,
    identity_write_company_router, identity_write_router,
};
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde_json::Value;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TIME_PREFERENCE_UNAVAILABLE",
        "The native time preference service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Composition-owned backend. `control` is the Control Postgres pool that owns
/// `account_time_preferences`; `scope` is the Control company id written on the
/// receipt/audit/outbox rows; `clock` supplies the committed timestamp.
pub struct AccountTimePreferenceBackend<C> {
    control: DatabasePool,
    scope: String,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> AccountTimePreferenceBackend<C> {
    pub fn new(control: DatabasePool, scope: impl Into<String>, clock: Arc<C>) -> Self {
        Self {
            control,
            scope: scope.into(),
            clock,
        }
    }

    /// Resolve the internal caller identity. The private gateway admits the
    /// request; this header is the session-to-user mapping it supplies.
    fn caller(headers: &HeaderMap) -> Result<String, BusinessError> {
        let mut values = headers.get_all(CONTEXT_USER).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "ACCOUNT_TIME_PREFERENCE_REQUEST_INVALID",
                "Use one bounded account time-preference command.",
            ));
        }
        first
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .map(str::to_owned)
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unauthenticated,
                    "SIGN_IN_REQUIRED",
                    "Sign in to edit your time settings.",
                )
            })
    }
}

impl<C: Clock + Send + Sync + 'static> AccountTimePreferenceHttpBackend
    for AccountTimePreferenceBackend<C>
{
    fn command<'a>(
        &'a self,
        request: AccountTimePreferenceHttpRequest<'a>,
    ) -> AccountTimePreferenceHttpFuture<'a, AccountTimePreferenceHttpResponse> {
        Box::pin(async move {
            let AccountTimePreferenceHttpKind::Change { user_id } = request.kind else {
                return Err(BusinessError::invalid(
                    "ACCOUNT_TIME_PREFERENCE_REQUEST_INVALID",
                    "Use one bounded account time-preference command.",
                ));
            };
            // The private caller identity must match the aggregate in the body.
            if Self::caller(request.headers)? != user_id {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ACCOUNT_CONTEXT_CHANGED",
                    "Your account changed. Reload before editing these settings.",
                ));
            }
            let body = request.body.clone();
            let key = request.key.cloned();
            let scope = self.scope.clone();
            let clock = self.clock.clone();
            let control = self.control.clone();
            let data = PgUnitOfWork::new(control)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let user_id = user_id.clone();
                    let body = body.clone();
                    let key = key.clone();
                    let scope = scope.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let mut repository = PgTimePreferenceCommand::new(&mut *connection, &scope);
                        let preference = change_time_preference(
                            &user_id,
                            &body,
                            key.as_ref(),
                            &mut repository,
                            clock.now(),
                        )
                        .await?;
                        drop(repository);
                        Ok(serde_json::json!({"preference": preference}))
                    })
                })
                .await
                .map_err(business)?;
            Ok(AccountTimePreferenceHttpResponse { data })
        })
    }
}

/// Build the owner router bound to a backend. Lead mounts the returned Router.
pub fn account_time_preference_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn AccountTimePreferenceHttpBackend>,
) -> Result<Router, BusinessError> {
    account_time_preference_http_router(vec![AccountTimePreferenceHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}

// ---------------------------------------------------------------------------
// Generic identity owner-write executor (one router, allow-listed kinds).
// ---------------------------------------------------------------------------

/// Composition backend for the single identity owner-write router. It opens
/// one serializable Control transaction per command, resolves the internal
/// caller identity, and delegates the allow-listed kind + full-parity values to
/// the owner executor, which owns all SQL and the receipt/audit/outbox ledger.
pub struct IdentityOwnerWriteBackend<C> {
    control: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> IdentityOwnerWriteBackend<C> {
    pub fn new(control: DatabasePool, clock: Arc<C>) -> Self {
        Self { control, clock }
    }
}

fn write_actor(headers: &HeaderMap) -> String {
    headers
        .get_all(CONTEXT_USER)
        .iter()
        .next()
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 200)
        .unwrap_or("system")
        .to_owned()
}

impl<C: Clock + Send + Sync + 'static> IdentityWriteBackend for IdentityOwnerWriteBackend<C> {
    fn execute<'a>(&'a self, request: IdentityWriteRequest<'a>) -> IdentityWriteFuture<'a> {
        Box::pin(async move {
            let company = request.company_id.to_owned();
            let kind = request.command.kind.clone();
            let values: Vec<Value> = request.command.values.clone();
            let now = if request.command.now > 0 {
                request.command.now
            } else {
                self.clock.now().get()
            };
            let key = request.key.as_str().to_owned();
            let actor = write_actor(request.headers);
            let scope = company.clone();
            let control = self.control.clone();
            let outcome = PgUnitOfWork::new(control)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let scope = scope.clone();
                    let actor = actor.clone();
                    let kind = kind.clone();
                    let values = values.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        execute_identity_write(
                            &mut *connection,
                            &scope,
                            &actor,
                            &kind,
                            &values,
                            now,
                            &key,
                        )
                        .await
                    })
                })
                .await
                .map_err(business)?;
            Ok(outcome)
        })
    }
}

/// Build the Holding-facing owner-write router. Lead mounts this alongside the
/// other Identity routes with exact registered Host/slug/origin targets.
pub fn identity_owner_write_router<C: Clock + Send + Sync + 'static>(
    control: DatabasePool,
    clock: Arc<C>,
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn IdentityWriteBackend> =
        Arc::new(IdentityOwnerWriteBackend::new(control, clock));
    let targets = targets
        .into_iter()
        .map(|(host, holding_slug, origin)| IdentityWriteTarget {
            host,
            holding_slug,
            origin,
            backend: backend.clone(),
        })
        .collect();
    identity_write_router(targets)
}

/// Build the Company-keyed private owner-write router for internal workers.
pub fn identity_owner_write_company_router<C: Clock + Send + Sync + 'static>(
    control: DatabasePool,
    clock: Arc<C>,
    companies: Vec<String>,
) -> Result<Router, BusinessError> {
    let backend: Arc<dyn IdentityWriteBackend> =
        Arc::new(IdentityOwnerWriteBackend::new(control, clock));
    identity_write_company_router(
        companies
            .into_iter()
            .map(|company| (company, backend.clone()))
            .collect(),
    )
}
