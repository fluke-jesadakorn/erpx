//! Composition for the native Commercial owner-write command HTTP route.
//! Standalone module; the lead mounts the router and injects this backend.
use erpx_server_commercial::public::owner_write::{PgOwnerWrite, execute_owner_write};
use erpx_server_commercial::public::owner_write_http::{
    OwnerWriteBackend, OwnerWriteFuture, OwnerWriteRequest, OwnerWriteResponse, OwnerWriteTarget,
    commercial_owner_write_router,
};
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCIAL_OWNER_WRITE_UNAVAILABLE",
        "The native Commercial service is unavailable.",
    )
}
fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct CommercialOwnerWriteBackend<C> {
    holding: DatabasePool,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> CommercialOwnerWriteBackend<C> {
    pub fn new(holding: DatabasePool, clock: Arc<C>) -> Self {
        Self { holding, clock }
    }
    fn caller(headers: &HeaderMap) -> Result<String, BusinessError> {
        let mut values = headers.get_all(CONTEXT_USER).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "COMMERCIAL_OWNER_WRITE_REQUEST_INVALID",
                "Use one bounded command.",
            ));
        }
        first
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty() && v.len() <= 200)
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

impl<C: Clock + Send + Sync + 'static> OwnerWriteBackend for CommercialOwnerWriteBackend<C> {
    fn write<'a>(
        &'a self,
        request: OwnerWriteRequest<'a>,
    ) -> OwnerWriteFuture<'a, OwnerWriteResponse> {
        Box::pin(async move {
            let actor = Self::caller(request.headers)?;
            let body = request.body.clone();
            let key = request.key.cloned();
            let holding = self.holding.clone();
            let clock = self.clock.clone();
            let scope = body
                .get("companyId")
                .and_then(|v| v.as_str())
                .or_else(|| body.get("holdingId").and_then(|v| v.as_str()))
                .filter(|v| !v.is_empty() && v.len() <= 200)
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
            Ok(OwnerWriteResponse { data })
        })
    }
}

pub fn commercial_owner_write_composed_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn OwnerWriteBackend>,
) -> Result<Router, BusinessError> {
    commercial_owner_write_router(vec![OwnerWriteTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
