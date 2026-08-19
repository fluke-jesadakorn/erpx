use async_trait::async_trait;
use chrono::NaiveDate;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::{crm, marketing, marketing::QualifiedLeadFact};
use erpx_kernel::{AuthorityScope, Clock, CorrelationId, ErrorCategory, ErrorContext, KernelError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        CrmAggregate, CrmDomainError, CrmLead, CrmLeadId, CrmOpportunity, OpportunityId,
        OpportunityStage,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, CrmAction, CrmCommit, CrmLeadView, EntitlementPort, OpportunityList,
        OpportunityListFilter, OpportunityView, PortError, QualifiedLeadEvidence,
        QualifiedLeadResolveRequest, QualifiedLeadResolverPort, ReadRepository, StoredCommit,
        WriteRepository,
    },
};

pub const MAX_CRM_PAGE_SIZE: u16 = 100;

/// Marketing handoff accepts only Marketing's published fact, never its aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptMarketingLead {
    pub lead_id: CrmLeadId,
    pub qualified: QualifiedLeadFact,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateOpportunity {
    pub opportunity_id: OpportunityId,
    pub accepted_lead_id: CrmLeadId,
    pub customer_id: uuid::Uuid,
    pub name: String,
    pub expected_value_minor: i64,
    pub currency: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionOpportunity {
    pub opportunity_id: OpportunityId,
    pub stage: OpportunityStage,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateOpportunityForecast {
    pub opportunity_id: OpportunityId,
    pub account: crm::CrmAccountRef,
    pub owner: String,
    pub category: crm::PipelineCategory,
    pub probability_basis_points: u16,
    pub expected_value_minor: i64,
    pub expected_close_on: NaiveDate,
    pub originating_campaign: Option<marketing::CampaignRef>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum CrmCommand {
    AcceptMarketingLead(AcceptMarketingLead),
    CreateOpportunity(CreateOpportunity),
    TransitionOpportunity(TransitionOpportunity),
    UpdateOpportunityForecast(UpdateOpportunityForecast),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCrmLead {
    pub lead_id: CrmLeadId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOpportunity {
    pub opportunity_id: OpportunityId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListOpportunities {
    pub filter: OpportunityListFilter,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum CrmQuery {
    GetLead(GetCrmLead),
    GetOpportunity(GetOpportunity),
    ListOpportunities(ListOpportunities),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum CrmMutationResult {
    Lead(CrmLeadView),
    Opportunity(OpportunityView),
}
impl CrmMutationResult {
    fn from_aggregate(value: &CrmAggregate) -> Self {
        match value {
            CrmAggregate::Lead(v) => Self::Lead(v.into()),
            CrmAggregate::Opportunity(v) => Self::Opportunity(v.into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum CrmQueryResponse {
    Lead(Option<CrmLeadView>),
    Opportunity(Option<OpportunityView>),
    Opportunities(OpportunityList),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CrmApplicationError {
    #[error(transparent)]
    Domain(#[from] CrmDomainError),
    #[error("command scope does not contain the CRM record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("CRM aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("the accepted lead is not linked to the opportunity customer")]
    CustomerMismatch,
    #[error("CRM page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("CRM integration mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableQualifiedLeadResolver;

#[async_trait]
impl QualifiedLeadResolverPort for UnavailableQualifiedLeadResolver {
    async fn resolve_qualified_lead(
        &self,
        _request: QualifiedLeadResolveRequest<'_>,
    ) -> Result<QualifiedLeadEvidence, PortError> {
        Err(PortError::Unavailable)
    }
}

pub struct CrmCommandHandler<W, U, A, E, C, M = UnavailableQualifiedLeadResolver> {
    write: W,
    uow: U,
    authorization: A,
    entitlement: E,
    clock: C,
    qualified_leads: M,
}
impl<W, U, A, E, C> CrmCommandHandler<W, U, A, E, C, UnavailableQualifiedLeadResolver> {
    pub const fn new(write: W, uow: U, authorization: A, entitlement: E, clock: C) -> Self {
        Self {
            write,
            uow,
            authorization,
            entitlement,
            clock,
            qualified_leads: UnavailableQualifiedLeadResolver,
        }
    }
}

impl<W, U, A, E, C, M> CrmCommandHandler<W, U, A, E, C, M> {
    pub fn with_qualified_lead_resolver<N>(
        self,
        qualified_leads: N,
    ) -> CrmCommandHandler<W, U, A, E, C, N> {
        CrmCommandHandler {
            write: self.write,
            uow: self.uow,
            authorization: self.authorization,
            entitlement: self.entitlement,
            clock: self.clock,
            qualified_leads,
        }
    }
}

impl<W, U, A, E, C, M> CrmCommandHandler<W, U, A, E, C, M>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
    M: QualifiedLeadResolverPort,
{
    async fn preflight(
        &self,
        actor: &erpx_kernel::ActorAttribution,
        authority: &AuthorityScope,
        target: &AuthorityScope,
        action: CrmAction,
    ) -> Result<(), CrmApplicationError> {
        if !authority.contains(target) {
            return Err(CrmApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_crm_enabled(target, action.write())
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
        action: CrmAction,
    ) -> Result<Option<StoredCommit>, CrmApplicationError> {
        let Some(mut stored) = self.uow.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(actor, scope, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }
    // Preserve the bounded application error taxonomy at the private version fence.
    #[allow(clippy::result_large_err)]
    fn expected(expected: Option<u64>, actual: u64) -> Result<u64, CrmApplicationError> {
        let expected = expected.ok_or(CrmApplicationError::MissingExpectedVersion)?;
        if expected != actual {
            return Err(CrmApplicationError::VersionConflict { expected, actual });
        }
        Ok(expected)
    }
}

#[async_trait]
impl<W, U, A, E, C, M> CommandHandler<CrmCommand> for CrmCommandHandler<W, U, A, E, C, M>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
    M: QualifiedLeadResolverPort,
{
    type Response = CrmMutationResult;
    async fn handle(
        &self,
        envelope: CommandEnvelope<CrmCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let (action, fingerprint) = match &envelope.command {
            CrmCommand::AcceptMarketingLead(c) => (
                CrmAction::AcceptLead,
                CommandFingerprint::AcceptLead {
                    lead_id: c.lead_id,
                    marketing_candidate_id: c.qualified.candidate.as_uuid(),
                },
            ),
            CrmCommand::CreateOpportunity(c) => (
                CrmAction::CreateOpportunity,
                CommandFingerprint::CreateOpportunity {
                    opportunity_id: c.opportunity_id,
                    accepted_lead_id: c.accepted_lead_id,
                    customer_id: c.customer_id,
                    expected_value_minor: c.expected_value_minor,
                    currency: c.currency.clone(),
                },
            ),
            CrmCommand::TransitionOpportunity(c) => (
                CrmAction::TransitionOpportunity,
                CommandFingerprint::TransitionOpportunity {
                    opportunity_id: c.opportunity_id,
                    stage: c.stage,
                },
            ),
            CrmCommand::UpdateOpportunityForecast(c) => (
                CrmAction::UpdateOpportunityForecast,
                CommandFingerprint::UpdateOpportunityForecast {
                    opportunity_id: c.opportunity_id,
                    account: c.account,
                    owner: c.owner.clone(),
                    category: c.category,
                    probability_basis_points: c.probability_basis_points,
                    expected_value_minor: c.expected_value_minor,
                    expected_close_on: c.expected_close_on,
                    originating_campaign: c.originating_campaign,
                },
            ),
        };
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint,
        };
        if let Some(stored) = self
            .replay(&identity, &envelope.actor, &envelope.active_scope, action)
            .await
            .map_err(|e| to_cqrs(e, correlation))?
        {
            let response = CrmMutationResult::from_aggregate(&stored.aggregate);
            return Ok(stored.into_receipt(response));
        }
        let at = self.clock.now();
        let (mut aggregate, expected) = match envelope.command {
            CrmCommand::AcceptMarketingLead(c) => {
                if envelope.expected_aggregate_version.is_some() {
                    return Err(to_cqrs(
                        CrmApplicationError::UnexpectedCreateVersion,
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
                let requested_candidate = c.qualified.candidate;
                let evidence = self
                    .qualified_leads
                    .resolve_qualified_lead(QualifiedLeadResolveRequest {
                        actor: &envelope.actor,
                        authority_scope: &envelope.active_scope,
                        candidate: requested_candidate,
                    })
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                if evidence.fact.candidate != requested_candidate || evidence.source_version == 0 {
                    return Err(to_cqrs(PortError::Denied.into(), correlation));
                }
                let q = evidence.fact;
                let lead = CrmLead::accept(
                    c.lead_id,
                    envelope.active_scope.clone(),
                    q.candidate.as_uuid(),
                    q.campaign.as_uuid(),
                    q.audience_member.as_uuid(),
                    evidence.source_version,
                    q.existing_customer.map(|v| v.as_uuid()),
                    q.source.into_inner(),
                    q.qualification_score_basis_points,
                    at,
                )
                .map_err(|e| to_cqrs(e.into(), correlation))?;
                (CrmAggregate::Lead(lead), None)
            }
            CrmCommand::CreateOpportunity(c) => {
                if envelope.expected_aggregate_version.is_some() {
                    return Err(to_cqrs(
                        CrmApplicationError::UnexpectedCreateVersion,
                        correlation,
                    ));
                }
                let lead = self
                    .write
                    .load_lead(&envelope.active_scope, c.accepted_lead_id)
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    lead.scope(),
                    action,
                )
                .await
                .map_err(|e| to_cqrs(e, correlation))?;
                if lead.customer_id().is_some_and(|id| id != c.customer_id) {
                    return Err(to_cqrs(CrmApplicationError::CustomerMismatch, correlation));
                }
                let opportunity = CrmOpportunity::create(
                    c.opportunity_id,
                    lead.scope().clone(),
                    c.accepted_lead_id,
                    c.customer_id,
                    c.name,
                    c.expected_value_minor,
                    c.currency,
                    at,
                )
                .map_err(|e| to_cqrs(e.into(), correlation))?;
                (CrmAggregate::Opportunity(opportunity), None)
            }
            CrmCommand::TransitionOpportunity(c) => {
                let mut opportunity = self
                    .write
                    .load_opportunity(&envelope.active_scope, c.opportunity_id)
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    opportunity.scope(),
                    action,
                )
                .await
                .map_err(|e| to_cqrs(e, correlation))?;
                let expected =
                    Self::expected(envelope.expected_aggregate_version, opportunity.version())
                        .map_err(|e| to_cqrs(e, correlation))?;
                opportunity
                    .transition(c.stage, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (CrmAggregate::Opportunity(opportunity), Some(expected))
            }
            CrmCommand::UpdateOpportunityForecast(c) => {
                let mut opportunity = self
                    .write
                    .load_opportunity(&envelope.active_scope, c.opportunity_id)
                    .await
                    .map_err(|e| to_cqrs(e.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    opportunity.scope(),
                    action,
                )
                .await
                .map_err(|e| to_cqrs(e, correlation))?;
                let expected =
                    Self::expected(envelope.expected_aggregate_version, opportunity.version())
                        .map_err(|e| to_cqrs(e, correlation))?;
                opportunity
                    .update_forecast(
                        c.account,
                        c.owner,
                        c.category,
                        c.probability_basis_points,
                        c.expected_value_minor,
                        c.expected_close_on,
                        c.originating_campaign,
                        at,
                    )
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (CrmAggregate::Opportunity(opportunity), Some(expected))
            }
        };
        let events = integration::published_events(&aggregate.take_uncommitted_events())
            .map_err(|_| to_cqrs(CrmApplicationError::IntegrationMapping, correlation))?;
        let stored = self
            .uow
            .commit(CrmCommit {
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
        let response = CrmMutationResult::from_aggregate(&stored.aggregate);
        Ok(stored.into_receipt(response))
    }
}

pub struct CrmQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}
impl<R, A, E> CrmQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}
#[async_trait]
impl<R, A, E> QueryHandler<CrmQuery> for CrmQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = CrmQueryResponse;
    async fn handle(
        &self,
        envelope: QueryEnvelope<CrmQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        self.entitlement
            .ensure_crm_enabled(&envelope.active_scope, false)
            .await
            .map_err(|e| to_cqrs(e.into(), correlation))?;
        let action = match &envelope.query {
            CrmQuery::GetLead(_) => CrmAction::ViewLead,
            CrmQuery::GetOpportunity(_) => CrmAction::ViewOpportunity,
            CrmQuery::ListOpportunities(_) => CrmAction::ListOpportunities,
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
        let (value, projection) = match envelope.query {
            CrmQuery::GetLead(q) => {
                let p = self
                    .read
                    .get_lead(&envelope.active_scope, q.lead_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (CrmQueryResponse::Lead(p.value), p.metadata)
            }
            CrmQuery::GetOpportunity(q) => {
                let p = self
                    .read
                    .get_opportunity(
                        &envelope.active_scope,
                        q.opportunity_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (CrmQueryResponse::Opportunity(p.value), p.metadata)
            }
            CrmQuery::ListOpportunities(q) => {
                if q.filter.page_size == 0 || q.filter.page_size > MAX_CRM_PAGE_SIZE {
                    return Err(to_cqrs(CrmApplicationError::InvalidPageSize, correlation));
                }
                let p = self
                    .read
                    .list_opportunities(&envelope.active_scope, &q.filter, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (CrmQueryResponse::Opportunities(p.value), p.metadata)
            }
        };
        Ok(QueryResult {
            value,
            metadata: QueryMetadata {
                correlation_id: correlation,
                projection,
                consistency: envelope.consistency,
            },
        })
    }
}
fn port_to_cqrs(error: PortError, correlation: CorrelationId) -> CqrsError {
    if let PortError::NotReady(value) = error {
        CqrsError::NotReady(value)
    } else {
        to_cqrs(error.into(), correlation)
    }
}
fn to_cqrs(error: CrmApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, message, retryable) = match error {
        CrmApplicationError::ScopeOutsideAuthority
        | CrmApplicationError::Port(PortError::Denied | PortError::EntitlementDenied) => (
            "crm.denied",
            ErrorCategory::Denied,
            "CRM operation is not allowed.",
            false,
        ),
        CrmApplicationError::Port(PortError::NotFound) => (
            "crm.not_found",
            ErrorCategory::NotFound,
            "CRM record was not found.",
            false,
        ),
        CrmApplicationError::MissingExpectedVersion
        | CrmApplicationError::VersionConflict { .. }
        | CrmApplicationError::Port(PortError::Conflict { .. } | PortError::IdempotencyConflict) => {
            (
                "crm.conflict",
                ErrorCategory::Conflict,
                "CRM record changed or command identity conflicts.",
                false,
            )
        }
        CrmApplicationError::Port(PortError::Unavailable) => (
            "crm.unavailable",
            ErrorCategory::Unavailable,
            "CRM is temporarily unavailable.",
            true,
        ),
        CrmApplicationError::Domain(_)
        | CrmApplicationError::UnexpectedCreateVersion
        | CrmApplicationError::CustomerMismatch
        | CrmApplicationError::InvalidPageSize => (
            "crm.invalid",
            ErrorCategory::Invalid,
            "CRM input is invalid.",
            false,
        ),
        CrmApplicationError::IntegrationMapping
        | CrmApplicationError::Port(PortError::Internal | PortError::NotReady(_)) => (
            "crm.internal",
            ErrorCategory::Internal,
            "CRM could not complete the operation.",
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
pub use records::{CrmRecordApplicationError, CrmRecordCommandHandler};
