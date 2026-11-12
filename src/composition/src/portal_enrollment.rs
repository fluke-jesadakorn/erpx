//! Native public Customer Portal enrollment assembly.
//!
//! The Account origin is the only public entry. The token claims select the
//! exact Holding; the Control session is verified natively and re-proved
//! between owner steps. No TypeScript writer, caller identity or fallback is
//! accepted.
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_customer_authority::public::portal_enrollment::{
    PgPortalEnrollmentCommand, PortalEnrollmentHttpBackend, PortalEnrollmentHttpCommand,
    PortalEnrollmentHttpFuture, PortalEnrollmentHttpRequest, PortalSession, PortalSessionPort,
    accept, release, reserve,
};
use erpx_server_customer_authority::public::portal_invitation_delivery::verify_invitation_token;
use erpx_server_identity_access::public::portal_session::{
    PortalSessionReadRequest, require_current_portal_session, verify_portal_session,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_PORTAL_ENROLLMENT_UNAVAILABLE",
        "The Customer Portal enrollment service is unavailable.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE",
        "This customer invitation is no longer available.",
    )
}

fn session_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "CUSTOMER_PORTAL_SESSION_REQUIRED",
        "Sign in again to accept this invitation.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone)]
pub struct PortalEnrollmentConfiguration {
    control: DatabasePool,
    holdings: BTreeMap<String, DatabasePool>,
    environment: String,
    secret: String,
    mode: PackageEnforcementMode,
    production: bool,
}

impl PortalEnrollmentConfiguration {
    pub fn new(
        control: DatabasePool,
        holdings: BTreeMap<String, DatabasePool>,
        environment: String,
        secret: String,
        mode: PackageEnforcementMode,
        production: bool,
    ) -> Self {
        Self {
            control,
            holdings,
            environment,
            secret,
            mode,
            production,
        }
    }

    fn pool(&self, holding_slug: &str) -> Result<DatabasePool, BusinessError> {
        self.holdings
            .get(holding_slug)
            .cloned()
            .ok_or_else(not_found)
    }
}

/// Exact Control-owned session verification for one presented product token.
struct ControlSessionPort {
    control: DatabasePool,
    token: String,
    audience: Option<String>,
    environment: String,
    now: i64,
}

impl PortalSessionPort for ControlSessionPort {
    fn current<'a>(
        &'a mut self,
        expected: Option<&'a PortalSession>,
    ) -> erpx_server_customer_authority::public::portal_enrollment::EnrollmentFuture<
        'a,
        PortalSession,
        BusinessError,
    > {
        Box::pin(async move {
            let mut connection = self
                .control
                .raw()
                .acquire()
                .await
                .map_err(|_| unavailable())?;
            let request = PortalSessionReadRequest {
                token: &self.token,
                audience: self.audience.as_deref(),
                environment: &self.environment,
            };
            match expected {
                None => verify_portal_session(&mut connection, &request, self.now)
                    .await
                    .map_err(business)?
                    .ok_or_else(session_required),
                Some(expected) => {
                    require_current_portal_session(&mut connection, &request, expected, self.now)
                        .await
                        .map_err(business)
                }
            }
        })
    }
}

pub struct PortalEnrollmentService<C> {
    configuration: Arc<PortalEnrollmentConfiguration>,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PortalEnrollmentService<C> {
    pub fn new(configuration: Arc<PortalEnrollmentConfiguration>, clock: Arc<C>) -> Self {
        Self {
            configuration,
            clock,
        }
    }

    fn claims(
        &self,
        token: &str,
    ) -> Result<
        erpx_server_customer_authority::public::portal_invitation_delivery::InvitationClaims,
        BusinessError,
    > {
        verify_invitation_token(&self.configuration.secret, token).ok_or_else(not_found)
    }

    async fn execute(
        &self,
        command: PortalEnrollmentHttpCommand,
        host: &str,
    ) -> Result<Value, BusinessError> {
        let now = self.clock.now().get();
        let configuration = self.configuration.clone();
        match command {
            PortalEnrollmentHttpCommand::Reserve { token } => {
                let claims = self.claims(&token)?;
                let pool = configuration.pool(&claims.holding_slug)?;
                let mode = configuration.mode;
                let production = configuration.production;
                let control = configuration.control.clone();
                let environment = configuration.environment.clone();
                let _ = host;
                let invite = PgUnitOfWork::new(pool)
                    .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                        let token = token.clone();
                        let claims = claims.clone();
                        let control = control.clone();
                        let environment = environment.clone();
                        Box::pin(async move {
                            // Reserve never reads the session; the port still
                            // verifies if an owner step ever asks.
                            let mut session = ControlSessionPort {
                                control,
                                token: String::new(),
                                audience: None,
                                environment,
                                now,
                            };
                            let mut command = PgPortalEnrollmentCommand::new(
                                connection,
                                &mut session,
                                mode,
                                production,
                            );
                            reserve(&mut command, &claims, &token, now).await
                        })
                    })
                    .await
                    .map_err(business)?;
                serde_json::to_value(invite).map_err(|_| unavailable())
            }
            PortalEnrollmentHttpCommand::Release { reservation } => {
                let pool = configuration.pool(&reservation.holding_slug)?;
                let mode = configuration.mode;
                let production = configuration.production;
                let control = configuration.control.clone();
                let environment = configuration.environment.clone();
                let _ = host;
                PgUnitOfWork::new(pool)
                    .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                        let reservation = reservation.clone();
                        let control = control.clone();
                        let environment = environment.clone();
                        Box::pin(async move {
                            let mut session = ControlSessionPort {
                                control,
                                token: String::new(),
                                audience: None,
                                environment,
                                now,
                            };
                            let mut command = PgPortalEnrollmentCommand::new(
                                connection,
                                &mut session,
                                mode,
                                production,
                            );
                            release(&mut command, &reservation).await
                        })
                    })
                    .await
                    .map_err(business)?;
                Ok(serde_json::json!({"released": true}))
            }
            PortalEnrollmentHttpCommand::Accept { token, session } => {
                let claims = self.claims(&token)?;
                let pool = configuration.pool(&claims.holding_slug)?;
                let mode = configuration.mode;
                let production = configuration.production;
                let control = configuration.control.clone();
                let environment = configuration.environment.clone();
                let accepted = PgUnitOfWork::new(pool)
                    .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                        let token = token.clone();
                        let claims = claims.clone();
                        let control = control.clone();
                        let environment = environment.clone();
                        let session = session.clone();
                        Box::pin(async move {
                            let mut verifier = ControlSessionPort {
                                control,
                                token: session,
                                audience: None,
                                environment,
                                now,
                            };
                            let mut command = PgPortalEnrollmentCommand::new(
                                connection,
                                &mut verifier,
                                mode,
                                production,
                            );
                            accept(&mut command, &claims, &token, now).await
                        })
                    })
                    .await
                    .map_err(business)?;
                serde_json::to_value(accepted).map_err(|_| unavailable())
            }
        }
    }
}

pub struct PortalEnrollmentBackend<C> {
    service: Arc<PortalEnrollmentService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PortalEnrollmentBackend<C> {
    pub fn new(service: Arc<PortalEnrollmentService<C>>) -> Self {
        Self { service }
    }
}

impl<C: Clock + Send + Sync + 'static> PortalEnrollmentHttpBackend for PortalEnrollmentBackend<C> {
    fn execute<'a>(
        &'a self,
        request: PortalEnrollmentHttpRequest<'a>,
    ) -> PortalEnrollmentHttpFuture<'a, Value> {
        Box::pin(async move { self.service.execute(request.command, request.host).await })
    }
}
