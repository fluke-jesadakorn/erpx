//! Composition for the native Executive evidence-access command HTTP route.
//!
//! This module is intentionally standalone: the lead mounts the router built by
//! [`executive_evidence_router`] into the private gateway and injects the
//! backend built by [`ExecutiveEvidenceBackend::new`]. It resolves the internal
//! caller identity from the private caller's `x-erpx-context-user` header (the
//! public Holding route maps the session to that header upstream), checks it
//! against the body aggregate, opens one serializable Holding transaction per
//! command and injects the owner adapter. No SQL and no business rules live
//! here.
use erpx_server_runtime::public::executive_evidence::{
    EvidenceActor, ExecutiveEvidenceHttpBackend, ExecutiveEvidenceHttpFuture,
    ExecutiveEvidenceHttpKind, ExecutiveEvidenceHttpRequest, ExecutiveEvidenceHttpTarget,
    PgEvidenceAccessCommand, executive_evidence_http_router, record_evidence_access,
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
        "EXECUTIVE_EVIDENCE_UNAVAILABLE",
        "The native Executive evidence service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Composition-owned backend. `holding` is the Holding Postgres pool that owns
/// `executive_evidence_access_events`; `scope` is the Holding id written on the
/// receipt/audit/outbox rows; `clock` supplies the committed timestamp.
pub struct ExecutiveEvidenceBackend<C> {
    holding: DatabasePool,
    scope: String,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ExecutiveEvidenceBackend<C> {
    pub fn new(holding: DatabasePool, scope: impl Into<String>, clock: Arc<C>) -> Self {
        Self {
            holding,
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
                "EXECUTIVE_EVIDENCE_REQUEST_INVALID",
                "Use one bounded Executive evidence-access command.",
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
                    "Sign in before recording evidence access.",
                )
            })
    }
}

impl<C: Clock + Send + Sync + 'static> ExecutiveEvidenceHttpBackend
    for ExecutiveEvidenceBackend<C>
{
    fn command<'a>(
        &'a self,
        request: ExecutiveEvidenceHttpRequest<'a>,
    ) -> ExecutiveEvidenceHttpFuture<'a> {
        Box::pin(async move {
            let ExecutiveEvidenceHttpKind::Record {
                user_id,
                company_id,
                assignment_id,
            } = request.kind
            else {
                return Err(BusinessError::invalid(
                    "EXECUTIVE_EVIDENCE_REQUEST_INVALID",
                    "Use one bounded Executive evidence-access command.",
                ));
            };
            if Self::caller(request.headers)? != user_id {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "EVIDENCE_CONTEXT_CHANGED",
                    "Your account changed. Reload before recording evidence access.",
                ));
            }
            let actor = EvidenceActor {
                user_id: user_id.clone(),
                company_id,
                assignment_id,
            };
            let body = request.body.clone();
            let key = request.key.cloned();
            let scope = self.scope.clone();
            let clock = self.clock.clone();
            let holding = self.holding.clone();
            let data = PgUnitOfWork::new(holding)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let body = body.clone();
                    let key = key.clone();
                    let scope = scope.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let mut repository = PgEvidenceAccessCommand::new(&mut *connection, &scope);
                        let result = record_evidence_access(
                            &actor,
                            &body,
                            key.as_ref(),
                            &mut repository,
                            clock.now(),
                        )
                        .await?;
                        drop(repository);
                        Ok(serde_json::json!({ "data": result }))
                    })
                })
                .await
                .map_err(business)?;
            Ok(data)
        })
    }
}

/// Build the owner router bound to a backend. Lead mounts the returned Router.
pub fn executive_evidence_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn ExecutiveEvidenceHttpBackend>,
) -> Result<Router, BusinessError> {
    executive_evidence_http_router(vec![ExecutiveEvidenceHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
