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
    PgTimePreferenceCommand, PgTimePreferenceReader, TimePreference, change_time_preference,
};
use erpx_server_identity_access::public::{
    AccountPolicy,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    account_sessions::verify_session_in_transaction,
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
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Request, Response, Router, State, StatusCode,
    body::to_bytes, routing::get,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use serde_json::Value;
use sha2::{Digest, Sha256};
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

struct BrowserTimePreferences<C> {
    control: DatabasePool,
    policy: Arc<AccountPolicy>,
    holdings: Arc<ConfiguredHoldingRegistry>,
    clock: Arc<C>,
}

/// The browser and SSR use the signed session cookie for the exact Account
/// audience. They never supply the preference's aggregate identity.
pub fn browser_time_preference_router<C: Clock + Send + Sync + 'static>(
    control: DatabasePool,
    policy: Arc<AccountPolicy>,
    holdings: Arc<ConfiguredHoldingRegistry>,
    clock: Arc<C>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/account/time-preference",
            get(browser_preference::<C>).patch(browser_preference::<C>),
        )
        .with_state(Arc::new(BrowserTimePreferences {
            control,
            policy,
            holdings,
            clock,
        }))
}

async fn browser_preference<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<BrowserTimePreferences<C>>>,
    request: Request,
) -> Response {
    let result: Result<TimePreference, Error> = async {
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        let audience = inbound.audience().clone();
        let change = if request.method() == axum::http::Method::PATCH {
            if unique_request_header(request.headers(), "origin")?
                != Some(inbound.expected_origin())
            {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "Use the application origin.",
                )
                .into());
            }
            let invalid = || {
                BusinessError::invalid("TIME_PREFERENCE_INVALID", "Choose a valid time preference.")
            };
            let bytes = to_bytes(request.into_body(), 4096)
                .await
                .map_err(|_| invalid())?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
            Some(
                value
                    .as_object()
                    .filter(|body| !body.contains_key("userId"))
                    .ok_or_else(invalid)?
                    .clone(),
            )
        } else {
            None
        };
        let proof = inbound.into_proof();
        PgUnitOfWork::new(state.control.clone())
            .run(TransactionOptions::default(), move |connection| {
                let (state, audience, proof, change) = (
                    state.clone(),
                    audience.clone(),
                    proof.clone(),
                    change.clone(),
                );
                Box::pin(async move {
                    let now = state.clock.now();
                    let session = verify_session_in_transaction(
                        connection,
                        &proof,
                        &audience,
                        &state.policy.environment,
                        now.get(),
                        &state.holdings,
                    )
                    .await?
                    .ok_or_else(|| {
                        BusinessError::new(
                            FailureKind::Unauthenticated,
                            "SIGN_IN_REQUIRED",
                            "Sign in to access your time settings.",
                        )
                    })?;
                    let user = &session.user.id;
                    if let Some(change) = change {
                        let input = serde_json::json!({
                            "mode": change.get("mode"),
                            "timeZone": change.get("timeZone"),
                            "hourCycle": change.get("hourCycle"),
                            "version": change.get("version"),
                            "userId": user,
                        });
                        // A retry of the same version and settings returns its
                        // existing receipt alongside the original audit/outbox.
                        let key =
                            CommandKey::new(format!("{:x}", Sha256::digest(input.to_string())))?;
                        let mut owner = PgTimePreferenceCommand::new(connection, "control");
                        change_time_preference(user, &input, Some(&key), &mut owner, now).await
                    } else {
                        Ok(PgTimePreferenceReader::new(connection)
                            .current(user)
                            .await?
                            .unwrap_or_default())
                    }
                })
            })
            .await
    }
    .await;
    match result {
        Ok(preference) => Json(serde_json::json!({"data": preference})).into_response(),
        Err(error) => {
            let error = business(error);
            let status = match error.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (
                status,
                Json(serde_json::json!({"error": {"code": error.code, "message": error.message}})),
            )
                .into_response()
        }
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
