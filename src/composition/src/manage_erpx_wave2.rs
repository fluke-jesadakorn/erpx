//! Composition for the native sandbox email delivery command HTTP route.
//!
//! Standalone module: the lead mounts [`email_delivery_router`] into the
//! private gateway and injects the backend built by
//! [`EmailDeliveryBackend::new`]. It opens one Control transaction per command
//! and injects the owner `PgEmailDelivery` adapter. No SQL and no encryption
//! live here: the caller encrypts sensitive bodies before dispatch.
use erpx_server_manage_erpx::public::email_delivery::{PgEmailDelivery, reserve_email_delivery};
use erpx_server_manage_erpx::public::owner_write::{PgOwnerWrite, execute_owner_write};
use erpx_server_manage_erpx::public::owner_write_http::{
    OwnerWriteFuture, OwnerWriteHttpBackend, OwnerWriteHttpRequest, OwnerWriteHttpTarget,
    owner_write_router,
};
use erpx_server_manage_erpx::public::email_delivery_http::{
    EmailDeliveryHttpBackend, EmailDeliveryHttpFuture, EmailDeliveryHttpKind,
    EmailDeliveryHttpRequest, EmailDeliveryHttpResponse, EmailDeliveryHttpTarget,
    email_delivery_http_router,
};
use erpx_server_runtime::public::http_support::Router;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_MANAGE_ERPX_UNAVAILABLE",
        "The native Manage ERPX service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct EmailDeliveryBackend {
    control: DatabasePool,
}

impl EmailDeliveryBackend {
    pub fn new(control: DatabasePool) -> Self {
        Self { control }
    }
}

impl EmailDeliveryHttpBackend for EmailDeliveryBackend {
    fn command<'a>(
        &'a self,
        request: EmailDeliveryHttpRequest<'a>,
    ) -> EmailDeliveryHttpFuture<'a, EmailDeliveryHttpResponse> {
        Box::pin(async move {
            let EmailDeliveryHttpKind::Reserve(input) = request.kind else {
                return Err(BusinessError::invalid(
                    "EMAIL_DELIVERY_REQUEST_INVALID",
                    "Use one bounded sandbox email delivery command.",
                ));
            };
            let control = self.control.clone();
            let data = PgUnitOfWork::new(control)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let input = input.clone();
                    Box::pin(async move {
                        let mut repository = PgEmailDelivery::new(&mut *connection);
                        reserve_email_delivery(&mut repository, &input).await
                    })
                })
                .await
                .map_err(business)?;
            Ok(EmailDeliveryHttpResponse { data })
        })
    }
}

/// Build the owner router bound to a backend. Lead mounts the returned Router.
pub fn email_delivery_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn EmailDeliveryHttpBackend>,
) -> Result<Router, BusinessError> {
    email_delivery_http_router(vec![EmailDeliveryHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}

/// Executor for the owner-wide Manage ERPX command router. It selects the
/// exact registered Holding pool or the Control pool from the route target,
/// opens one transaction and injects `PgOwnerWrite`. No SQL lives here.
pub struct ManageErpxOwnerWriteBackend {
    holdings: std::collections::BTreeMap<String, DatabasePool>,
    control: DatabasePool,
}

impl ManageErpxOwnerWriteBackend {
    pub fn new(
        holdings: impl IntoIterator<Item = (String, DatabasePool)>,
        control: DatabasePool,
    ) -> Self {
        Self {
            holdings: holdings.into_iter().collect(),
            control,
        }
    }
}

impl OwnerWriteHttpBackend for ManageErpxOwnerWriteBackend {
    fn execute<'a>(&'a self, request: OwnerWriteHttpRequest<'a>) -> OwnerWriteFuture<'a> {
        Box::pin(async move {
            let pool = if request.control {
                self.control.clone()
            } else {
                self.holdings
                    .get(request.holding_slug)
                    .ok_or_else(unavailable)?
                    .clone()
            };
            let kind = request.command.kind.clone();
            let values = request.command.values.clone();
            let company = request.command.company_id.clone();
            let now = request.command.now;
            let key = request.key.to_owned();
            let actor = request
                .headers
                .get(CONTEXT_USER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("owner-write")
                .to_owned();
            PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let kind = kind.clone();
                    let values = values.clone();
                    let company = company.clone();
                    let actor = actor.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        let mut repository = PgOwnerWrite::new(&mut *connection);
                        let command = erpx_server_manage_erpx::public::owner_write::OwnerWriteCommand {
                            kind: &kind,
                            values: &values,
                            company_id: &company,
                            actor_id: &actor,
                            now,
                        };
                        execute_owner_write(&mut repository, &command, &key).await
                    })
                })
                .await
                .map_err(business)
        })
    }
}

const CONTEXT_USER: erpx_server_runtime::public::http_support::HeaderName =
    erpx_server_runtime::public::http_support::HeaderName::from_static("x-erpx-context-user");

/// Build the owner-wide router from a configured backend and exact targets.
pub fn manage_erpx_owner_write_router(
    backend: Arc<dyn OwnerWriteHttpBackend>,
    targets: Vec<(String, String, String, bool)>,
) -> Result<Router, BusinessError> {
    owner_write_router(
        targets
            .into_iter()
            .map(|(host, holding_slug, origin, control)| OwnerWriteHttpTarget {
                host,
                holding_slug,
                origin,
                control,
                backend: backend.clone(),
            })
            .collect(),
    )
}
