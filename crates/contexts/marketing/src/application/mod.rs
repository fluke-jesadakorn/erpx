use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{
    AuthorityScope, Clock, CorrelationId, ErrorCategory, ErrorContext, HumanDecisionProof,
    KernelError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        AudienceMemberId, BudgetAllocation, BudgetAllocationId, CampaignBudget, CampaignBudgetId,
        CampaignId, LeadCandidate, LeadCandidateId, MarketingAggregate, MarketingDomainError,
        SpendRequestId,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CampaignBudgetView, CommandFingerprint,
        CommandIdentity, CommandUnitOfWork, EntitlementPort, LeadCandidateList,
        LeadCandidateListFilter, LeadCandidateView, MarketingAction, MarketingCommit, PortError,
        ReadRepository, StoredCommit, WriteRepository,
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
pub struct CreateCampaignBudget {
    pub budget_id: CampaignBudgetId,
    pub campaign_id: CampaignId,
    pub code: String,
    pub name: String,
    pub period: erpx_integration_contracts::marketing::CampaignBudgetPeriod,
    pub requested_amount: erpx_integration_contracts::MoneyAmount,
    pub dimensions: erpx_integration_contracts::DimensionSet,
    pub allocations: Vec<BudgetAllocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitCampaignBudget {
    pub budget_id: CampaignBudgetId,
    pub approval: erpx_integration_contracts::operations::ApprovalRequestFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyCampaignBudgetDecision {
    pub budget_id: CampaignBudgetId,
    pub decision: erpx_integration_contracts::operations::ApprovalDecisionFact,
}

macro_rules! budget_command {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            pub budget_id: CampaignBudgetId,
        }
    };
}

budget_command!(StartCampaignBudget);
budget_command!(CompleteCampaignBudget);
budget_command!(CancelCampaignBudget);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReserveCampaignSpend {
    pub budget_id: CampaignBudgetId,
    pub request_id: SpendRequestId,
    pub allocation_id: BudgetAllocationId,
    pub amount: erpx_integration_contracts::MoneyAmount,
    pub expected_outcome: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseCampaignSpend {
    pub budget_id: CampaignBudgetId,
    pub request_id: SpendRequestId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum MarketingCommand {
    Create(CreateLeadCandidate),
    CaptureEngagement(CaptureEngagement),
    Qualify(QualifyLeadCandidate),
    Reject(RejectLeadCandidate),
    CreateBudget(CreateCampaignBudget),
    SubmitBudget(SubmitCampaignBudget),
    ApplyBudgetDecision(ApplyCampaignBudgetDecision),
    StartBudget(StartCampaignBudget),
    CompleteBudget(CompleteCampaignBudget),
    CancelBudget(CancelCampaignBudget),
    ReserveSpend(ReserveCampaignSpend),
    ReleaseSpend(ReleaseCampaignSpend),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetLeadCandidate {
    pub lead_id: LeadCandidateId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListLeadCandidates {
    pub filter: LeadCandidateListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCampaignBudget {
    pub budget_id: CampaignBudgetId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum MarketingQuery {
    Get(GetLeadCandidate),
    List(ListLeadCandidates),
    GetBudget(GetCampaignBudget),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum MarketingMutationResult {
    Candidate(LeadCandidateView),
    CampaignBudget(Box<CampaignBudgetView>),
}
impl MarketingMutationResult {
    fn from_aggregate(value: &MarketingAggregate) -> Result<Self, MarketingApplicationError> {
        match value {
            MarketingAggregate::LeadCandidate(value) => Ok(Self::Candidate(value.as_ref().into())),
            MarketingAggregate::CampaignBudget(value) => Ok(Self::CampaignBudget(Box::new(
                CampaignBudgetView::try_from(value.as_ref())?,
            ))),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum MarketingQueryResponse {
    Candidate(Option<LeadCandidateView>),
    Candidates(LeadCandidateList),
    CampaignBudget(Option<Box<CampaignBudgetView>>),
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
    #[error("lead rejection requires an authenticated human decision bound to this command")]
    HumanDecisionRequired,
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
        authority: &AuthorityScope,
        target: &AuthorityScope,
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
        scope: &AuthorityScope,
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
        let (action, fingerprint) = match &envelope.command {
            MarketingCommand::Create(c) => (
                MarketingAction::CreateLead,
                CommandFingerprint::CreateLead {
                    lead_id: c.lead_id,
                    campaign_id: c.campaign_id,
                    audience_member_id: c.audience_member_id,
                    source: c.source.clone(),
                },
            ),
            MarketingCommand::CaptureEngagement(c) => (
                MarketingAction::CaptureEngagement,
                CommandFingerprint::CaptureEngagement {
                    lead_id: c.lead_id,
                    channel: c.channel.clone(),
                    action: c.action.clone(),
                },
            ),
            MarketingCommand::Qualify(c) => (
                MarketingAction::Qualify,
                CommandFingerprint::Qualify {
                    lead_id: c.lead_id,
                    score_basis_points: c.score_basis_points,
                },
            ),
            MarketingCommand::Reject(c) => (
                MarketingAction::Reject,
                CommandFingerprint::Reject { lead_id: c.lead_id },
            ),
            MarketingCommand::CreateBudget(c) => (
                MarketingAction::CreateBudget,
                CommandFingerprint::CreateBudget {
                    budget_id: c.budget_id,
                    campaign_id: c.campaign_id,
                    code: c.code.clone(),
                    name: c.name.clone(),
                    period: c.period,
                    requested_amount: c.requested_amount,
                    dimensions: c.dimensions.clone(),
                    allocations: c.allocations.clone(),
                },
            ),
            MarketingCommand::SubmitBudget(c) => (
                MarketingAction::SubmitBudget,
                CommandFingerprint::SubmitBudget {
                    budget_id: c.budget_id,
                    approval: c.approval.clone(),
                },
            ),
            MarketingCommand::ApplyBudgetDecision(c) => (
                MarketingAction::ApplyBudgetDecision,
                CommandFingerprint::ApplyBudgetDecision {
                    budget_id: c.budget_id,
                    decision: c.decision.clone(),
                },
            ),
            MarketingCommand::StartBudget(c) => (
                MarketingAction::StartBudget,
                CommandFingerprint::StartBudget {
                    budget_id: c.budget_id,
                },
            ),
            MarketingCommand::CompleteBudget(c) => (
                MarketingAction::CompleteBudget,
                CommandFingerprint::CompleteBudget {
                    budget_id: c.budget_id,
                },
            ),
            MarketingCommand::CancelBudget(c) => (
                MarketingAction::CancelBudget,
                CommandFingerprint::CancelBudget {
                    budget_id: c.budget_id,
                },
            ),
            MarketingCommand::ReserveSpend(c) => (
                MarketingAction::ReserveSpend,
                CommandFingerprint::ReserveSpend {
                    budget_id: c.budget_id,
                    request_id: c.request_id,
                    allocation_id: c.allocation_id,
                    amount: c.amount,
                    expected_outcome: c.expected_outcome.clone(),
                },
            ),
            MarketingCommand::ReleaseSpend(c) => (
                MarketingAction::ReleaseSpend,
                CommandFingerprint::ReleaseSpend {
                    budget_id: c.budget_id,
                    request_id: c.request_id,
                },
            ),
        };
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint,
        };
        if matches!(envelope.command, MarketingCommand::Reject(_))
            && envelope.expected_aggregate_version.is_none()
        {
            return Err(to_cqrs(
                MarketingApplicationError::MissingExpectedVersion,
                correlation,
            ));
        }
        // Construct human-only evidence before receipt lookup so replay cannot
        // authorize a non-human, sessionless, represented, or support actor.
        let decision_at =
            matches!(envelope.command, MarketingCommand::Reject(_)).then(|| self.clock.now());
        let human_decision = match (&envelope.command, decision_at) {
            (MarketingCommand::Reject(command), Some(at)) => Some(
                HumanDecisionProof::try_new(
                    envelope.actor.clone(),
                    envelope.active_scope.clone(),
                    crate::domain::lead_rejection_decision_ref(
                        envelope.command_id,
                        command.lead_id,
                        envelope.expected_aggregate_version.unwrap_or(0),
                    ),
                    at,
                )
                .map_err(|_| {
                    to_cqrs(
                        MarketingApplicationError::HumanDecisionRequired,
                        correlation,
                    )
                })?,
            ),
            _ => None,
        };
        if let Some(stored) = self
            .replay(&identity, &envelope.actor, &envelope.active_scope, action)
            .await
            .map_err(|e| to_cqrs(e, correlation))?
        {
            let response = MarketingMutationResult::from_aggregate(&stored.aggregate)
                .map_err(|e| to_cqrs(e, correlation))?;
            return Ok(stored.into_receipt(response));
        }

        let at = decision_at.unwrap_or_else(|| self.clock.now());
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
                    &envelope.active_scope,
                    &envelope.active_scope,
                    action,
                )
                .await
                .map_err(|e| to_cqrs(e, correlation))?;
                (
                    MarketingAggregate::LeadCandidate(Box::new(
                        LeadCandidate::create(
                            c.lead_id,
                            envelope.active_scope.clone(),
                            c.campaign_id,
                            c.audience_member_id,
                            c.source,
                            c.existing_customer_id,
                            at,
                        )
                        .map_err(|e| to_cqrs(e.into(), correlation))?,
                    )),
                    None,
                )
            }
            MarketingCommand::CaptureEngagement(c) => {
                let mut value = self
                    .load_existing(
                        &envelope.active_scope,
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
                (MarketingAggregate::LeadCandidate(Box::new(value)), expected)
            }
            MarketingCommand::Qualify(c) => {
                let mut value = self
                    .load_existing(
                        &envelope.active_scope,
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
                (MarketingAggregate::LeadCandidate(Box::new(value)), expected)
            }
            MarketingCommand::Reject(c) => {
                let mut value = self
                    .load_existing(
                        &envelope.active_scope,
                        c.lead_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected_version = value.version();
                let decision = human_decision.as_ref().ok_or_else(|| {
                    to_cqrs(
                        MarketingApplicationError::HumanDecisionRequired,
                        correlation,
                    )
                })?;
                value
                    .reject(decision, envelope.command_id, expected_version, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    MarketingAggregate::LeadCandidate(Box::new(value)),
                    Some(expected_version),
                )
            }
            MarketingCommand::CreateBudget(c) => {
                if envelope.expected_aggregate_version.is_some() {
                    return Err(to_cqrs(
                        MarketingApplicationError::UnexpectedCreateVersion,
                        correlation,
                    ));
                }
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &envelope.active_scope,
                    action,
                )
                .await
                .map_err(|e| to_cqrs(e, correlation))?;
                let budget = CampaignBudget::create(
                    c.budget_id,
                    envelope.active_scope.clone(),
                    c.campaign_id,
                    c.code,
                    c.name,
                    c.period,
                    c.requested_amount,
                    c.dimensions,
                    c.allocations,
                    at,
                )
                .map_err(|e| to_cqrs(e.into(), correlation))?;
                (MarketingAggregate::CampaignBudget(Box::new(budget)), None)
            }
            MarketingCommand::SubmitBudget(c) => {
                let mut value = self
                    .load_budget(
                        &envelope.active_scope,
                        c.budget_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .submit_for_review(c.approval, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    MarketingAggregate::CampaignBudget(Box::new(value)),
                    expected,
                )
            }
            MarketingCommand::ApplyBudgetDecision(c) => {
                let mut value = self
                    .load_budget(
                        &envelope.active_scope,
                        c.budget_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .apply_approval_decision(c.decision, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    MarketingAggregate::CampaignBudget(Box::new(value)),
                    expected,
                )
            }
            MarketingCommand::StartBudget(c) => {
                let mut value = self
                    .load_budget(
                        &envelope.active_scope,
                        c.budget_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .start(at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    MarketingAggregate::CampaignBudget(Box::new(value)),
                    expected,
                )
            }
            MarketingCommand::CompleteBudget(c) => {
                let mut value = self
                    .load_budget(
                        &envelope.active_scope,
                        c.budget_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .complete(at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    MarketingAggregate::CampaignBudget(Box::new(value)),
                    expected,
                )
            }
            MarketingCommand::CancelBudget(c) => {
                let mut value = self
                    .load_budget(
                        &envelope.active_scope,
                        c.budget_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .cancel(at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    MarketingAggregate::CampaignBudget(Box::new(value)),
                    expected,
                )
            }
            MarketingCommand::ReserveSpend(c) => {
                let mut value = self
                    .load_budget(
                        &envelope.active_scope,
                        c.budget_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .reserve_spend(
                        c.request_id,
                        c.allocation_id,
                        c.amount,
                        c.expected_outcome,
                        at,
                    )
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    MarketingAggregate::CampaignBudget(Box::new(value)),
                    expected,
                )
            }
            MarketingCommand::ReleaseSpend(c) => {
                let mut value = self
                    .load_budget(
                        &envelope.active_scope,
                        c.budget_id,
                        envelope.expected_aggregate_version,
                        &envelope.actor,
                        action,
                        correlation,
                    )
                    .await?;
                let expected = Some(value.version());
                value
                    .release_spend(c.request_id, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    MarketingAggregate::CampaignBudget(Box::new(value)),
                    expected,
                )
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
                authority_scope: envelope.active_scope,
                audit: envelope.audit,
                expected_version: expected,
                aggregate,
                integration_events: events,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        let response = MarketingMutationResult::from_aggregate(&stored.aggregate)
            .map_err(|e| to_cqrs(e, correlation))?;
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
        scope: &AuthorityScope,
        id: LeadCandidateId,
        expected: Option<u64>,
        actor: &erpx_kernel::ActorAttribution,
        action: MarketingAction,
        correlation: CorrelationId,
    ) -> Result<LeadCandidate, CqrsError> {
        let value = self
            .write
            .load_lead(scope, id)
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

    async fn load_budget(
        &self,
        scope: &AuthorityScope,
        id: CampaignBudgetId,
        expected: Option<u64>,
        actor: &erpx_kernel::ActorAttribution,
        action: MarketingAction,
        correlation: CorrelationId,
    ) -> Result<CampaignBudget, CqrsError> {
        let value = self
            .write
            .load_budget(scope, id)
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
            .ensure_marketing_enabled(&envelope.active_scope, false)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        let action = match &envelope.query {
            MarketingQuery::Get(_) => MarketingAction::ViewLead,
            MarketingQuery::List(_) => MarketingAction::ListLeads,
            MarketingQuery::GetBudget(_) => MarketingAction::ViewBudget,
        };
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action,
            })
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        match envelope.query {
            MarketingQuery::Get(q) => {
                let projected = self
                    .read
                    .get_lead(&envelope.active_scope, q.lead_id, &envelope.consistency)
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
                    .list_leads(&envelope.active_scope, &q.filter, &envelope.consistency)
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
            MarketingQuery::GetBudget(q) => {
                let projected = self
                    .read
                    .get_budget(&envelope.active_scope, q.budget_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                Ok(QueryResult {
                    value: MarketingQueryResponse::CampaignBudget(projected.value.map(Box::new)),
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
        | MarketingApplicationError::HumanDecisionRequired
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

mod records;
#[cfg(test)]
mod tests;

pub use records::{MarketingRecordApplicationError, MarketingRecordCommandHandler};
