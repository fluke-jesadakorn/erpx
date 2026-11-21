//! Composition for the native Commerce wave2 warehouse-master command route.
//!
//! Standalone module: the lead mounts the router built by
//! [`commerce_warehouse_save_router`] and injects the backend built by
//! [`WarehouseSaveBackend::new`]. It opens one serializable Holding transaction
//! per command and injects the owner adapter. No SQL or business rules live
//! here; current Company/authority resolution is the lead's responsibility.
use erpx_server_commerce::public::wave2::{PgWarehouseSave, save_warehouse};
use erpx_server_commerce::public::wave2_http::{
    WarehouseSaveHttpBackend, WarehouseSaveHttpFuture, WarehouseSaveHttpRequest,
    WarehouseSaveHttpResponse, WarehouseSaveHttpTarget, commerce_warehouse_save_router,
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
        "COMMERCE_WAVE2_UNAVAILABLE",
        "The native Commerce service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct WarehouseSaveBackend<C> {
    holding: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> WarehouseSaveBackend<C> {
    pub fn new(holding: DatabasePool, clock: Arc<C>) -> Self {
        Self { holding, clock }
    }

    fn caller(headers: &HeaderMap) -> Result<String, BusinessError> {
        let mut values = headers.get_all(CONTEXT_USER).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "COMMERCE_WAVE2_REQUEST_INVALID",
                "Use one bounded warehouse command.",
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
                    "Sign in to change the warehouse master.",
                )
            })
    }
}

impl<C: Clock + Send + Sync + 'static> WarehouseSaveHttpBackend for WarehouseSaveBackend<C> {
    fn save<'a>(
        &'a self,
        request: WarehouseSaveHttpRequest<'a>,
    ) -> WarehouseSaveHttpFuture<'a, WarehouseSaveHttpResponse> {
        Box::pin(async move {
            let actor = Self::caller(request.headers)?;
            let body = request.body.clone();
            let key = request.key.cloned();
            let holding = self.holding.clone();
            let clock = self.clock.clone();
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
                        let mut repository = PgWarehouseSave::new(&mut *connection, &scope);
                        let receipt = save_warehouse(
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
            Ok(WarehouseSaveHttpResponse { data })
        })
    }
}

pub fn commerce_warehouse_save_composed_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn WarehouseSaveHttpBackend>,
) -> Result<Router, BusinessError> {
    commerce_warehouse_save_router(vec![WarehouseSaveHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
