//! Composition for the native Agent AI-quota command HTTP route.
//!
//! This module is intentionally standalone: the lead mounts
//! [`agent_quota_router`] into the private gateway and injects the backend
//! built by [`AgentQuotaBackend::new`]. It resolves the caller-bound Holding,
//! opens one serializable Holding transaction per command and injects the
//! owner `PgCompanyQuota` adapter. No SQL and no business rules live here.
use erpx_server_agent::public::owner_write::{PgOwnerWrite, execute_owner_write};
use erpx_server_agent::public::owner_write_http::{
    OwnerWriteFuture, OwnerWriteHttpBackend, OwnerWriteHttpRequest, OwnerWriteHttpTarget,
    owner_write_router,
};
use erpx_server_agent::public::quota::{
    PgCompanyQuota, finish_company_quota, reserve_company_quota,
};
use erpx_server_agent::public::quota_http::{
    AgentQuotaHttpBackend, AgentQuotaHttpFuture, AgentQuotaHttpKind, AgentQuotaHttpRequest,
    AgentQuotaHttpResponse, AgentQuotaHttpTarget, agent_quota_http_router,
};
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;

type Error = PgTransactionError<BusinessError>;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "NATIVE_AGENT_QUOTA_UNAVAILABLE",
        "The native Agent quota service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Composition-owned backend. `holdings` is the Holding write pool registry;
/// the router's exact (Host, Holding) target selects the one pool for a
/// command. The caller's context-company header, when present, must match the
/// aggregate company id carried in the bounded body.
pub struct AgentQuotaBackend {
    holdings: std::collections::BTreeMap<String, DatabasePool>,
}

impl AgentQuotaBackend {
    pub fn new(holdings: impl IntoIterator<Item = (String, DatabasePool)>) -> Self {
        Self {
            holdings: holdings.into_iter().collect(),
        }
    }

    fn company(headers: &HeaderMap, expected: &str) -> Result<(), BusinessError> {
        let mut values = headers.get_all(CONTEXT_COMPANY).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "AI_QUOTA_REQUEST_INVALID",
                "Use one bounded AI quota command.",
            ));
        }
        if let Some(value) = first
            && value.to_str().map(|text| text != expected).unwrap_or(true)
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Forbidden,
                "AI_QUOTA_CONTEXT_CHANGED",
                "The Company scope changed. Reload before continuing.",
            ));
        }
        Ok(())
    }
}

impl AgentQuotaHttpBackend for AgentQuotaBackend {
    fn command<'a>(
        &'a self,
        request: AgentQuotaHttpRequest<'a>,
    ) -> AgentQuotaHttpFuture<'a, AgentQuotaHttpResponse> {
        Box::pin(async move {
            let pool = self
                .holdings
                .get(request.holding_slug)
                .ok_or_else(unavailable)?
                .clone();
            let kind = request.kind;
            let expected_company = match &kind {
                AgentQuotaHttpKind::Reserve(reservation) => reservation.company_id.clone(),
                AgentQuotaHttpKind::Finish(finish) => finish.company_id.clone(),
            };
            Self::company(request.headers, &expected_company)?;
            let data = PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let mut repository = PgCompanyQuota::new(&mut *connection);
                        let data = match kind {
                            AgentQuotaHttpKind::Reserve(reservation) => {
                                reserve_company_quota(&mut repository, &reservation).await?
                            }
                            AgentQuotaHttpKind::Finish(finish) => {
                                finish_company_quota(&mut repository, &finish).await?
                            }
                        };
                        Ok(data)
                    })
                })
                .await
                .map_err(business)?;
            Ok(AgentQuotaHttpResponse { data })
        })
    }
}

/// Build the owner router bound to a backend. Lead mounts the returned Router.
pub fn agent_quota_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: std::sync::Arc<dyn AgentQuotaHttpBackend>,
) -> Result<Router, BusinessError> {
    agent_quota_http_router(vec![AgentQuotaHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}

/// Executor for the owner-wide Manage ERPX command router. It selects the
/// exact registered Holding pool or the Control pool from the route target,
/// opens one transaction and injects `PgOwnerWrite`. No SQL lives here.
pub struct AgentOwnerWriteBackend {
    holdings: std::collections::BTreeMap<String, DatabasePool>,
    control: DatabasePool,
}

impl AgentOwnerWriteBackend {
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

impl OwnerWriteHttpBackend for AgentOwnerWriteBackend {
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
                    Box::pin(async move {
                        let mut repository = PgOwnerWrite::new(&mut *connection);
                        let command = erpx_server_agent::public::owner_write::OwnerWriteCommand {
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
pub fn agent_owner_write_router(
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
