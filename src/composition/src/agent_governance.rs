//! Native Agent governance composition.
//!
//! Account session, current IAM authority and the serializable Holding unit are
//! resolved here; catalogue/pool/assignment/session rules stay in the Agent
//! owner crate. Provider credentials and quota runtime remain A1-deferred and
//! are mounted separately.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_agent::public::governance::{
    AgentActor, AgentGovernanceHttpBackend, AgentGovernanceHttpCommandRequest,
    AgentGovernanceHttpFuture, AgentGovernanceHttpReadRequest, AgentGovernanceHttpTarget,
    AgentGovernancePorts, PgAgentGovernance, RecordKind, change_agent_pool,
    change_agent_record, change_private_session, create_agent, create_agent_pool,
    create_agent_version, start_agent_assignment,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "AGENT_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AGENT_UNAVAILABLE",
        "The Agent service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

#[derive(Clone, Copy)]
struct AgentSources;

impl HoldingWorkSourcePreparation for AgentSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct AgentGovernanceConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl AgentGovernanceConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn agent_actor(
    current: &AuthorizedWorkContext,
    holding: &HoldingId,
) -> Result<AgentActor, BusinessError> {
    Ok(AgentActor::new(
        holding.as_str(),
        crate::finance_journal_actor::command_actor(current)?,
        current.modules().to_vec(),
        current.command_iam_metadata().permissions.clone(),
    ))
}

fn record_kind(entity: &str) -> Option<RecordKind> {
    match entity {
        "agent" => Some(RecordKind::Agent),
        "version" => Some(RecordKind::Version),
        "assignment" => Some(RecordKind::Assignment),
        _ => None,
    }
}

pub struct AgentGovernanceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: AgentGovernanceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> AgentGovernanceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: AgentGovernanceConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        key: String,
        body: Value,
    ) -> Result<Value, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "ai.manage".into(),
                    module: Some("agent".into()),
                    destination: "agents".into(),
                    api: true,
                },
                AgentSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = agent_actor(current, &holding)?;
                        let now = clock.now().get();
                        let mut ports = PgAgentGovernance::new(connection, &actor, now);
                        let id = body.get("id").and_then(Value::as_str);
                        let entity = body
                            .get("entity")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                invalid(
                                    "AGENT_ENTITY_REQUIRED",
                                    "Choose an Agent governance command.",
                                )
                            })?;
                        let data = match (id, entity) {
                            (Some(id), "pool") => {
                                change_agent_pool(&mut ports, id, &body, &key).await?
                            }
                            (Some(id), "session") => {
                                change_private_session(&mut ports, id, &body, &key).await?
                            }
                            (Some(id), entity) => {
                                let kind = record_kind(entity).ok_or_else(|| {
                                    invalid(
                                        "AGENT_ENTITY_REQUIRED",
                                        "Choose an Agent governance command.",
                                    )
                                })?;
                                change_agent_record(&mut ports, kind, id, &body, &key).await?
                            }
                            (None, "agent") => create_agent(&mut ports, &body, &key).await?,
                            (None, "version") => {
                                create_agent_version(&mut ports, &body, &key).await?
                            }
                            (None, "pool") => create_agent_pool(&mut ports, &body, &key).await?,
                            (None, "assignment") => {
                                start_agent_assignment(&mut ports, &body, &key).await?
                            }
                            (None, _) => {
                                return Err(invalid(
                                    "AGENT_ENTITY_REQUIRED",
                                    "Choose an Agent governance command.",
                                )
                                .into())
                            }
                        };
                        let _ = ports.actor();
                        Ok(data)
                    })
                },
            )
            .await
    }
}

pub struct AgentGovernanceBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<AgentGovernanceService<C>>,
}

impl<C: Clock + Send + Sync + 'static> AgentGovernanceBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<AgentGovernanceService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> AgentGovernanceHttpBackend for AgentGovernanceBackend<C> {
    fn read<'a>(
        &'a self,
        _request: AgentGovernanceHttpReadRequest<'a>,
    ) -> AgentGovernanceHttpFuture<'a, Value> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "AGENT_WORKSPACE_READ_RETAINED",
                "The Agent workspace read is served by the retained reader.",
            ))
        })
    }

    fn command<'a>(
        &'a self,
        request: AgentGovernanceHttpCommandRequest<'a>,
    ) -> AgentGovernanceHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.key.to_owned(),
                    request.body,
                )
                .await
                .map_err(business)
        })
    }
}

pub fn agent_target(
    host: String,
    holding_slug: String,
    origin: String,
    backend: Arc<dyn AgentGovernanceHttpBackend>,
) -> AgentGovernanceHttpTarget {
    AgentGovernanceHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}
