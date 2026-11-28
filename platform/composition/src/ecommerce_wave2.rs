//! Composition for the native Ecommerce wave2 cart/checkout command HTTP route.
//!
//! Standalone module: the lead mounts the router built by
//! [`ecommerce_checkout_wave2_router`] and injects the backend built by
//! [`CheckoutWave2Backend::new`]. It opens one serializable Holding transaction
//! per command and injects the owner adapter. No SQL or business rules live
//! here; current Company/authority resolution is the lead's responsibility.
use erpx_ecommerce_server::public::wave2::{PgOwnerWrite, execute_owner_write};
use erpx_ecommerce_server::public::wave2_http::{
    CheckoutWave2HttpBackend, CheckoutWave2HttpFuture, CheckoutWave2HttpRequest,
    CheckoutWave2HttpResponse, CheckoutWave2HttpTarget, ecommerce_checkout_wave2_router,
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
        "ECOMMERCE_WAVE2_UNAVAILABLE",
        "The native E-commerce service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct CheckoutWave2Backend<C> {
    holding: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CheckoutWave2Backend<C> {
    pub fn new(holding: DatabasePool, clock: Arc<C>) -> Self {
        Self { holding, clock }
    }

    fn caller(headers: &HeaderMap) -> Result<String, BusinessError> {
        let mut values = headers.get_all(CONTEXT_USER).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "ECOMMERCE_WAVE2_REQUEST_INVALID",
                "Use one bounded E-commerce check-out command.",
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
                    "Sign in to change this E-commerce workspace.",
                )
            })
    }
}

impl<C: Clock + Send + Sync + 'static> CheckoutWave2HttpBackend for CheckoutWave2Backend<C> {
    fn write<'a>(
        &'a self,
        request: CheckoutWave2HttpRequest<'a>,
    ) -> CheckoutWave2HttpFuture<'a, CheckoutWave2HttpResponse> {
        Box::pin(async move {
            let actor = Self::caller(request.headers)?;
            let body = request.body.clone();
            let key = request.key.cloned();
            let holding = self.holding.clone();
            let clock = self.clock.clone();
            // The private gateway admits only an already authorized current
            // Company workspace. Lead may replace this with a resolved Company.
            let scope = request
                .headers
                .get(HeaderName::from_static("x-erpx-context-company"))
                .and_then(|value| value.to_str().ok())
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
            Ok(CheckoutWave2HttpResponse { data })
        })
    }
}

pub fn ecommerce_checkout_wave2_composed_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn CheckoutWave2HttpBackend>,
) -> Result<Router, BusinessError> {
    ecommerce_checkout_wave2_router(vec![CheckoutWave2HttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
