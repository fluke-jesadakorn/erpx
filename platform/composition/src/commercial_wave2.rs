//! Composition for the native Commercial wave2 grant-save command HTTP route.
//!
//! This module is intentionally standalone: the lead mounts the router built by
//! [`commercial_grant_save_router`] into the private gateway and injects the
//! backend built by [`GrantSaveBackend::new`]. It opens one serializable Holding
//! transaction per command and injects the owner adapter. No SQL and no
//! business rules live here; current Company/authority resolution is the lead's
//! composition responsibility (the private gateway admits the request first).
use erpx_commercial_server::public::wave2::{PgGrantSave, save_grant};
use erpx_commercial_server::public::wave2_http::{
    GrantSaveHttpBackend, GrantSaveHttpFuture, GrantSaveHttpRequest, GrantSaveHttpResponse,
    GrantSaveHttpTarget, commercial_grant_save_router,
};
use erpx_platform_runtime_server::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCIAL_WAVE2_UNAVAILABLE",
        "The native Commercial service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Composition-owned backend. `holding` is the Holding Postgres pool that owns
/// the Company package grant tables; `clock` supplies the committed timestamp.
pub struct GrantSaveBackend<C> {
    holding: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> GrantSaveBackend<C> {
    pub fn new(holding: DatabasePool, clock: Arc<C>) -> Self {
        Self { holding, clock }
    }

    fn caller(headers: &HeaderMap) -> Result<String, BusinessError> {
        let mut values = headers.get_all(CONTEXT_USER).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "COMMERCIAL_WAVE2_REQUEST_INVALID",
                "Use one bounded Commercial grant command.",
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
                    "Sign in to change Company packages.",
                )
            })
    }
}

impl<C: Clock + Send + Sync + 'static> GrantSaveHttpBackend for GrantSaveBackend<C> {
    fn save<'a>(
        &'a self,
        request: GrantSaveHttpRequest<'a>,
    ) -> GrantSaveHttpFuture<'a, GrantSaveHttpResponse> {
        Box::pin(async move {
            let actor = Self::caller(request.headers)?;
            let body = request.body.clone();
            let key = request.key.cloned();
            let holding = self.holding.clone();
            let clock = self.clock.clone();
            // The ledger scope is the current Company carried on the private
            // caller context. The lead may replace this with a resolved
            // Company authority before production mount; the private gateway
            // admits only an already authorized Company workspace.
            let scope = body
                .get("companyId")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty() && value.len() <= 200)
                .ok_or_else(unavailable)?
                .to_owned();
            let data = PgUnitOfWork::new(holding)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let body = body.clone();
                    let key = key.clone();
                    let scope = scope.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let mut repository = PgGrantSave::new(&mut *connection, &scope);
                        let receipt =
                            save_grant(&actor, &body, key.as_ref(), &mut repository, clock.now())
                                .await?;
                        drop(repository);
                        Ok(receipt)
                    })
                })
                .await
                .map_err(business)?;
            Ok(GrantSaveHttpResponse { data })
        })
    }
}

/// Build the owner router bound to a backend. Lead mounts the returned Router.
pub fn commercial_grant_save_wave2_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn GrantSaveHttpBackend>,
) -> Result<Router, BusinessError> {
    commercial_grant_save_router(vec![GrantSaveHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
