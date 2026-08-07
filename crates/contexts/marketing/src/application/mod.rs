use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{Clock, CorrelationId, ErrorCategory, ErrorContext, KernelError, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{AudienceMemberId, CampaignId, LeadCandidate, LeadCandidateId, MarketingDomainError},
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, LeadCandidateList, LeadCandidateListFilter,
        LeadCandidateView, MarketingAction, MarketingCommit, PortError, ReadRepository,
        StoredCommit, WriteRepository,
    },
};

pub const MAX_MARKETING_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateLeadCandidate {
    pub lead_id: LeadCandidateId,
    pub campaign_id: CampaignId,
    pub audience_member_id: AudienceMemberId,
    pub source: String,
    pub existing_customer_id: Option<uuid::Uuid>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureEngagement {
    pub lead_id: LeadCandidateId,
    pub channel: String,
    pub action: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifyLeadCandidate {
    pub lead_id: LeadCandidateId,
    pub score_basis_points: u16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectLeadCandidate {
    pub lead_id: LeadCandidateId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum MarketingCommand {
    Create(CreateLeadCandidate),
    CaptureEngagement(CaptureEngagement),
    Qualify(QualifyLeadCandidate),
    Reject(RejectLeadCandidate),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetLeadCandidate {
    pub lead_id: LeadCandidateId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListLeadCandidates {
    pub filter: LeadCandidateListFilter,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum MarketingQuery {
    Get(GetLeadCandidate),
    List(ListLeadCandidates),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingMutationResult {
    pub candidate: LeadCandidateView,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum MarketingQueryResponse {
    Candidate(Option<LeadCandidateView>),
    Candidates(LeadCandidateList),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MarketingApplicationError {
    #[error(transparent)]
    Domain(#[from] MarketingDomainError),
    #[error("command scope does not contain the Marketing record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("Marketing aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Marketing page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("Marketing integration mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct MarketingCommandHandler<W, U, A, E, C> {
    write: W,
    uow: U,
    authorization: A,
    entitlement: E,
    clock: C,
}
impl<W, U, A, E, C> MarketingCommandHandler<W, U, A, E, C> {
    pub const fn new(write: W, uow: U, authorization: A, entitlement: E, clock: C) -> Self {
        Self {
            write,
            uow,
            authorization,
            entitlement,
            clock,
        }
    }
}

impl<W, U, A, E, C> MarketingCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &erpx_kernel::ActorAttribution,
        authority: &TenantScope,
        target: &TenantScope,
        action: MarketingAction,
    ) -> Result<(), MarketingApplicationError> {
        if !authority.contains(target) {
            return Err(MarketingApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_marketing_enabled(target, action.write())
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope: authority,
                target_scope: target,
                action,
            })
            .await?;
        Ok(())
    }

    async fn replay(
        &self,
        identity: &CommandIdentity,
        actor: &erpx_kernel::ActorAttribution,
        scope: &TenantScope,
        action: MarketingAction,
    ) -> Result<Option<StoredCommit>, MarketingApplicationError> {
        let Some(mut stored) = self.uow.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(actor, scope, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<MarketingCommand> for MarketingCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = MarketingMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<MarketingCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = match &envelope.command {
            MarketingCommand::Create(_) => MarketingAction::Create,
            MarketingCommand::CaptureEngagement(_) => MarketingAction::CaptureEngagement,
            MarketingCommand::Qualify(_) => MarketingAction::Qualify,
            MarketingCommand::Reject(_) => MarketingAction::Reject,
        };
        let fingerprint = match &envelope.command {
            MarketingCommand::Create(c) => CommandFingerprint::Create {
                lead_id: c.lead_id,
                campaign_id: c.campaign_id,
                audience_member_id: c.audience_member_id,
                source: c.source.clone(),
            },
            MarketingCommand::CaptureEngagement(c) => CommandFingerprint::CaptureEngagement {
                lead_id: c.lead_id,
                channel: c.channel.clone(),
                action: c.action.clone(),
            },
            MarketingCommand::Qualify(c) => CommandFingerprint::Qualify {
                lead_id: c.lead_id,
                score_basis_points: c.score_basis_points,
            },
            MarketingCommand::Reject(c) => CommandFingerprint::Reject { lead_id: c.lead_id },
        };
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint,
        };
        if let Some(stored) = self
            .replay(&identity, &envelope.actor, &envelope.tenant_scope, action)
            .await
            .map_err(|e| to_cqrs(e, correlation))?
        {
            let response = MarketingMutationResult {
                candidate: (&stored.aggregate).into(),
            };
            return Ok(stored.into_receipt(response));
        }

        let at = self.clock.now();
        let (mut aggregate, expected) = match envelope.command {
            MarketingCommand::Create(c) => {
                if envelope.expected_aggregate_version.is_some() {
                    return Err(to_cqrs(
                        MarketingApplicationError::UnexpectedCreateVersion,
                        correlation,
                    ));
                }
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    &envelope.tenant_scope,
                    action,
                )
                .await
                .map_err(|e| to_cqrs(e, correlation))?;
                (
                    LeadCandidate::create(
                        c.lead_id,
                        envelope.tenant_scope.clone(),
                        c.campaign_id,
                        c.audience_member_id,
                        c.source,
                        c.existing_customer_id,
                        at,
                    )
                    .map_err(|e| to_cqrs(e.into(), correlation))?,
                    None,
                )
            }
            MarketingCommand::CaptureEngagement(c) => {
                let mut value = self
                    .load_existing(
                        &envelope.tenant_scope,
                        c.lead_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .capture_engagement(c.channel, c.action, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, expected)
            }
            MarketingCommand::Qualify(c) => {
                let mut value = self
                    .load_existing(
                        &envelope.tenant_scope,
                        c.lead_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .qualify(c.score_basis_points, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, expected)
            }
            MarketingCommand::Reject(c) => {
                let mut value = self
                    .load_existing(
                        &envelope.tenant_scope,
                        c.lead_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .reject(at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (value, expected)
            }
        };
        let events = integration::published_events(&aggregate.take_uncommitted_events())
            .map_err(|_| to_cqrs(MarketingApplicationError::IntegrationMapping, correlation))?;
        let stored = self
            .uow
            .commit(MarketingCommit {
                identity,
                correlation_id: correlation,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version: expected,
                aggregate,
                integration_events: events,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        let response = MarketingMutationResult {
            candidate: (&stored.aggregate).into(),
        };
        Ok(stored.into_receipt(response))
    }
}

impl<W, U, A, E, C> MarketingCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn load_existing(
        &self,
        scope: &TenantScope,
        id: LeadCandidateId,
        expected: Option<u64>,
        actor: &erpx_kernel::ActorAttribution,
        action: MarketingAction,
        correlation: CorrelationId,
    ) -> Result<LeadCandidate, CqrsError> {
        let value = self
            .write
            .load(scope, id)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
        self.preflight(actor, scope, value.scope(), action)
            .await
            .map_err(|e| to_cqrs(e, correlation))?;
        let expected = expected.ok_or_else(|| {
            to_cqrs(
                MarketingApplicationError::MissingExpectedVersion,
                correlation,
            )
        })?;
        if expected != value.version() {
            return Err(to_cqrs(
                MarketingApplicationError::VersionConflict {
                    expected,
                    actual: value.version(),
                },
                correlation,
            ));
        }
        Ok(value)
    }
}

pub struct MarketingQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}
impl<R, A, E> MarketingQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<MarketingQuery> for MarketingQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = MarketingQueryResponse;
    async fn handle(
        &self,
        envelope: QueryEnvelope<MarketingQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        self.entitlement
            .ensure_marketing_enabled(&envelope.tenant_scope, false)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        let action = if matches!(&envelope.query, MarketingQuery::Get(_)) {
            MarketingAction::View
        } else {
            MarketingAction::List
        };
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.tenant_scope,
                target_scope: &envelope.tenant_scope,
                action,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        match envelope.query {
            MarketingQuery::Get(q) => {
                let projected = self
                    .read
                    .get(&envelope.tenant_scope, q.lead_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                Ok(QueryResult {
                    value: MarketingQueryResponse::Candidate(projected.value),
                    metadata: QueryMetadata {
                        correlation_id: correlation,
                        projection: projected.metadata,
                        consistency: envelope.consistency,
                    },
                })
            }
            MarketingQuery::List(q) => {
                if q.filter.page_size == 0 || q.filter.page_size > MAX_MARKETING_PAGE_SIZE {
                    return Err(to_cqrs(
                        MarketingApplicationError::InvalidPageSize,
                        correlation,
                    ));
                }
                let projected = self
                    .read
                    .list(&envelope.tenant_scope, &q.filter, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                Ok(QueryResult {
                    value: MarketingQueryResponse::Candidates(projected.value),
                    metadata: QueryMetadata {
                        correlation_id: correlation,
                        projection: projected.metadata,
                        consistency: envelope.consistency,
                    },
                })
            }
        }
    }
}

fn port_to_cqrs(error: PortError, correlation: CorrelationId) -> CqrsError {
    if let PortError::NotReady(value) = error {
        CqrsError::NotReady(value)
    } else {
        to_cqrs(error.into(), correlation)
    }
}
fn to_cqrs(error: MarketingApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, message, retryable) = match error {
        MarketingApplicationError::ScopeOutsideAuthority
        | MarketingApplicationError::Port(PortError::Denied | PortError::EntitlementDenied) => (
            "marketing.denied",
            ErrorCategory::Denied,
            "Marketing operation is not allowed.",
            false,
        ),
        MarketingApplicationError::Port(PortError::NotFound) => (
            "marketing.not_found",
            ErrorCategory::NotFound,
            "Marketing lead candidate was not found.",
            false,
        ),
        MarketingApplicationError::MissingExpectedVersion
        | MarketingApplicationError::VersionConflict { .. }
        | MarketingApplicationError::Port(
            PortError::Conflict { .. } | PortError::IdempotencyConflict,
        ) => (
            "marketing.conflict",
            ErrorCategory::Conflict,
            "Marketing record changed or command identity conflicts.",
            false,
        ),
        MarketingApplicationError::Port(PortError::Unavailable) => (
            "marketing.unavailable",
            ErrorCategory::Unavailable,
            "Marketing is temporarily unavailable.",
            true,
        ),
        MarketingApplicationError::Domain(_)
        | MarketingApplicationError::UnexpectedCreateVersion
        | MarketingApplicationError::InvalidPageSize => (
            "marketing.invalid",
            ErrorCategory::Invalid,
            "Marketing input is invalid.",
            false,
        ),
        MarketingApplicationError::IntegrationMapping
        | MarketingApplicationError::Port(PortError::Internal | PortError::NotReady(_)) => (
            "marketing.internal",
            ErrorCategory::Internal,
            "Marketing could not complete the operation.",
            false,
        ),
    };
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

#[cfg(test)]
mod tests;
