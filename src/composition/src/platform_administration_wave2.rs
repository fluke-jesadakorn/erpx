//! Composition for the native engineering verification enrollment command.
//!
//! Standalone module: the lead mounts [`engineering_enrollment_router`] into
//! the private gateway and injects the backend built by
//! [`EngineeringEnrollmentBackend::new`]. It opens one Control transaction per
//! command and injects the owner `PgEngineeringEnrollment` adapter. No SQL and
//! no attestation/authority logic live here: the retained application has
//! already validated the attestation, database identity, Company existence and
//! run build before the native command is dispatched.
use erpx_server_platform_administration::public::owner_write::{PgOwnerWrite, execute_owner_write};
use erpx_server_platform_administration::public::owner_write_http::{
    OwnerWriteFuture, OwnerWriteHttpBackend, OwnerWriteHttpRequest, OwnerWriteHttpTarget,
    owner_write_router,
};
use erpx_server_platform_administration::public::engineering_enrollment::{
    PgEngineeringEnrollment, enroll_engineering_verification,
};
use erpx_server_platform_administration::public::native_engineering_verification_read::{
    PgEngineeringVerificationRead, read_prior,
};
use erpx_server_platform_administration::public::native_engineering_verification_read_http::{
    EngineeringVerificationReadHttpBackend, EngineeringVerificationReadHttpFuture,
    EngineeringVerificationReadHttpRequest, EngineeringVerificationReadHttpResponse,
    EngineeringVerificationReadHttpTarget, engineering_verification_read_http_router,
};
use erpx_server_platform_administration::public::engineering_enrollment_http::{
    EngineeringEnrollmentHttpBackend, EngineeringEnrollmentHttpFuture,
    EngineeringEnrollmentHttpKind, EngineeringEnrollmentHttpRequest,
    EngineeringEnrollmentHttpResponse, EngineeringEnrollmentHttpTarget,
    engineering_enrollment_http_router,
};
use erpx_server_runtime::public::http_support::Router;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_PLATFORM_UNAVAILABLE",
        "The native Platform service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct EngineeringEnrollmentBackend<C> {
    control: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> EngineeringEnrollmentBackend<C> {
    pub fn new(control: DatabasePool, clock: Arc<C>) -> Self {
        Self { control, clock }
    }
}

impl<C: Clock + Send + Sync + 'static> EngineeringEnrollmentHttpBackend
    for EngineeringEnrollmentBackend<C>
{
    fn command<'a>(
        &'a self,
        request: EngineeringEnrollmentHttpRequest<'a>,
    ) -> EngineeringEnrollmentHttpFuture<'a, EngineeringEnrollmentHttpResponse> {
        Box::pin(async move {
            let EngineeringEnrollmentHttpKind::Enroll(input) = request.kind else {
                return Err(BusinessError::invalid(
                    "ENGINEERING_ENROLLMENT_REQUEST_INVALID",
                    "Use one bounded engineering enrollment command.",
                ));
            };
            let now = self.clock.now().get();
            let control = self.control.clone();
            let data = PgUnitOfWork::new(control)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let input = input.clone();
                    Box::pin(async move {
                        let mut repository = PgEngineeringEnrollment::new(&mut *connection);
                        enroll_engineering_verification(&mut repository, &input, now).await
                    })
                })
                .await
                .map_err(business)?;
            Ok(EngineeringEnrollmentHttpResponse { data })
        })
    }
}

/// Build the owner router bound to a backend. Lead mounts the returned Router.
pub fn engineering_enrollment_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn EngineeringEnrollmentHttpBackend>,
) -> Result<Router, BusinessError> {
    engineering_enrollment_http_router(vec![EngineeringEnrollmentHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}

/// Executor for the owner-wide Manage ERPX command router. It selects the
/// exact registered Holding pool or the Control pool from the route target,
/// opens one transaction and injects `PgOwnerWrite`. No SQL lives here.
pub struct PlatformAdministrationOwnerWriteBackend {
    holdings: std::collections::BTreeMap<String, DatabasePool>,
    control: DatabasePool,
}

impl PlatformAdministrationOwnerWriteBackend {
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

impl OwnerWriteHttpBackend for PlatformAdministrationOwnerWriteBackend {
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
                        let command = erpx_server_platform_administration::public::owner_write::OwnerWriteCommand {
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
pub fn platform_administration_owner_write_router(
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

/// Execute the native engineering verification `prior` read on one Control
/// transaction per request. No SQL or authority decision lives here.
pub struct EngineeringVerificationReadBackend {
    control: DatabasePool,
}

impl EngineeringVerificationReadBackend {
    pub fn new(control: DatabasePool) -> Self {
        Self { control }
    }
}

impl EngineeringVerificationReadHttpBackend for EngineeringVerificationReadBackend {
    fn prior<'a>(
        &'a self,
        request: EngineeringVerificationReadHttpRequest<'a>,
    ) -> EngineeringVerificationReadHttpFuture<'a, EngineeringVerificationReadHttpResponse> {
        Box::pin(async move {
            let control = self.control.clone();
            let company_id = request.prior.company_id.clone();
            let run = request.prior.run.clone();
            let data = PgUnitOfWork::new(control)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company_id = company_id.clone();
                    let run = run.clone();
                    Box::pin(async move {
                        let mut repository = PgEngineeringVerificationRead::new(&mut *connection);
                        let prior = read_prior(&mut repository, &company_id, &run)
                            .await
                            .map_err(business)?;
                        Ok(match prior {
                            None => erpx_server_runtime::public::http_support::serde_json::Value::Null,
                            Some(prior) => erpx_server_runtime::public::http_support::serde_json::json!({
                                "source_fingerprint": prior.source_fingerprint,
                                "software_version": prior.software_version,
                                "release_manifest_sha256": prior.release_manifest_sha256,
                            }),
                        })
                    })
                })
                .await
                .map_err(business)?;
            Ok(EngineeringVerificationReadHttpResponse { data })
        })
    }
}

/// Build one Holding's engineering verification read router on the Control pool.
pub fn engineering_verification_read_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    control: DatabasePool,
) -> Result<Router, BusinessError> {
    engineering_verification_read_http_router(vec![EngineeringVerificationReadHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend: Arc::new(EngineeringVerificationReadBackend::new(control)),
    }])
}
