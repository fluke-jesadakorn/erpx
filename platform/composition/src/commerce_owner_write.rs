//! Composition for the native Commerce owner-write command HTTP route.
use erpx_commerce_server::public::owner_write::{PgOwnerWrite, execute_owner_write};
use erpx_commerce_server::public::owner_write_http::{
    OwnerWriteBackend, OwnerWriteFuture, OwnerWriteRequest, OwnerWriteResponse, OwnerWriteTarget,
    commerce_owner_write_router,
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
        "COMMERCE_OWNER_WRITE_UNAVAILABLE",
        "The native Commerce service is unavailable.",
    )
}
fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct CommerceOwnerWriteBackend<C> {
    holding: DatabasePool,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> CommerceOwnerWriteBackend<C> {
    pub fn new(holding: DatabasePool, clock: Arc<C>) -> Self {
        Self { holding, clock }
    }
    fn caller(headers: &HeaderMap) -> Result<String, BusinessError> {
        let mut values = headers.get_all(CONTEXT_USER).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "COMMERCE_OWNER_WRITE_REQUEST_INVALID",
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
                    "Sign in to change Commerce data.",
                )
            })
    }
}
impl<C: Clock + Send + Sync + 'static> OwnerWriteBackend for CommerceOwnerWriteBackend<C> {
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

pub fn commerce_owner_write_composed_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn OwnerWriteBackend>,
) -> Result<Router, BusinessError> {
    commerce_owner_write_router(vec![OwnerWriteTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
