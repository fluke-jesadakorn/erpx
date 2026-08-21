use async_trait::async_trait;
use chrono::Duration;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::{customer, identity_access};
use erpx_kernel::{
    AuthorityScope, Clock, CorrelationId, ErrorCategory, ErrorContext, HumanDecisionProof,
    KernelError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        BroadcastAudience, CapabilityEvidence, CapabilityId, CapabilityRevocationReason, Incident,
        IncidentId, IncidentStatus, MessageAudience, MessageId, SessionId, SupportAggregate,
        SupportCapability, SupportDomainError, SupportMode, SupportSession, SupportTicket,
        SupportTicketId, TicketMessage, TicketStatus, support_session_approval_decision_ref,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, DependencyVersion, EntitlementPort, IncidentList, IncidentListFilter,
        IncidentView, PortError, ReadRepository, StoredCommit, SupportAction,
        SupportAuthorityEffect, SupportCapabilityView, SupportCommit, SupportSessionView,
        SupportTicketList, SupportTicketListFilter, SupportTicketView, TicketReadAudience,
        WriteRepository,
    },
};

mod platform_case;
pub use platform_case::PlatformCaseCommandHandler;

pub const MAX_SUPPORT_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenSupportTicket {
    pub ticket_id: SupportTicketId,
    pub customer: customer::CustomerProfileFact,
    pub case_code: String,
    pub category: String,
    pub severity: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionSupportTicket {
    pub ticket_id: SupportTicketId,
    pub next: TicketStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddTicketMessage {
    pub ticket_id: SupportTicketId,
    pub message_id: MessageId,
    pub audience: MessageAudience,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestSupportSession {
    pub session_id: SessionId,
    pub ticket_id: SupportTicketId,
    pub agent: identity_access::PrincipalRef,
    pub represented_principal: Option<identity_access::PrincipalRef>,
    pub mode: SupportMode,
    pub permissions: Vec<String>,
    pub approved_scope: AuthorityScope,
    pub module: String,
    pub reason: String,
    pub duration_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApproveSupportSession {
    pub session_id: SessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeSupportSession {
    pub session_id: SessionId,
    pub reason: String,
}

/// Hash-only material resolved by the secure Server/agent boundary. Neither
/// this core command nor its receipt contains or returns raw credential data.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueSupportCapability {
    pub capability_id: CapabilityId,
    pub session_id: SessionId,
    pub evidence: CapabilityEvidence,
    pub secret_hash_sha256: [u8; 32],
}

impl std::fmt::Debug for IssueSupportCapability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IssueSupportCapability")
            .field("capability_id", &self.capability_id)
            .field("session_id", &self.session_id)
            .field("evidence", &self.evidence)
            .field("secret_hash_sha256", &"REDACTED")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeSupportCapability {
    pub capability_id: CapabilityId,
    pub reason: CapabilityRevocationReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenIncident {
    pub incident_id: IncidentId,
    pub incident_code: String,
    pub severity: String,
    pub affected_services: Vec<String>,
    pub affected_customer_accounts: Vec<customer::CustomerAccountRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionIncident {
    pub incident_id: IncidentId,
    pub next: IncidentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddIncidentTimeline {
    pub incident_id: IncidentId,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BroadcastIncident {
    pub incident_id: IncidentId,
    pub audience: BroadcastAudience,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum SupportCommand {
    OpenTicket(OpenSupportTicket),
    TransitionTicket(TransitionSupportTicket),
    AddTicketMessage(AddTicketMessage),
    RequestSession(RequestSupportSession),
    ApproveSession(ApproveSupportSession),
    RevokeSession(RevokeSupportSession),
    IssueCapability(IssueSupportCapability),
    RevokeCapability(RevokeSupportCapability),
    OpenIncident(OpenIncident),
    TransitionIncident(TransitionIncident),
    AddIncidentTimeline(AddIncidentTimeline),
    BroadcastIncident(BroadcastIncident),
}

impl SupportCommand {
    fn action(&self) -> SupportAction {
        match self {
            Self::OpenTicket(_) => SupportAction::OpenTicket,
            Self::TransitionTicket(_) => SupportAction::TransitionTicket,
            Self::AddTicketMessage(value) => match value.audience {
                MessageAudience::CustomerVisible => SupportAction::AddCustomerMessage,
                MessageAudience::Internal => SupportAction::AddInternalMessage,
            },
            Self::RequestSession(_) => SupportAction::RequestSession,
            Self::ApproveSession(_) => SupportAction::ApproveSession,
            Self::RevokeSession(_) => SupportAction::RevokeSession,
            Self::IssueCapability(_) => SupportAction::IssueCapability,
            Self::RevokeCapability(_) => SupportAction::RevokeCapability,
            Self::OpenIncident(_) => SupportAction::OpenIncident,
            Self::TransitionIncident(_) => SupportAction::TransitionIncident,
            Self::AddIncidentTimeline(_) => SupportAction::AddIncidentTimeline,
            Self::BroadcastIncident(value) => match value.audience {
                BroadcastAudience::CustomerVisible => SupportAction::BroadcastCustomerIncident,
                BroadcastAudience::Internal => SupportAction::BroadcastInternalIncident,
            },
        }
    }

    fn fingerprint(&self) -> CommandFingerprint {
        match self {
            Self::OpenTicket(v) => CommandFingerprint::OpenTicket {
                ticket_id: v.ticket_id,
                customer: v.customer.clone(),
                case_code: v.case_code.clone(),
                category: v.category.clone(),
                severity: v.severity.clone(),
                description: v.description.clone(),
            },
            Self::TransitionTicket(v) => CommandFingerprint::TransitionTicket {
                ticket_id: v.ticket_id,
                next: v.next,
            },
            Self::AddTicketMessage(v) => CommandFingerprint::AddTicketMessage {
                ticket_id: v.ticket_id,
                message_id: v.message_id,
                audience: v.audience,
                body: v.body.clone(),
            },
            Self::RequestSession(v) => CommandFingerprint::RequestSession {
                session_id: v.session_id,
                ticket_id: v.ticket_id,
                agent: v.agent,
                represented_principal: v.represented_principal,
                mode: v.mode,
                permissions: v.permissions.clone(),
                scope: v.approved_scope.clone(),
                module: v.module.clone(),
                reason: v.reason.clone(),
                duration_seconds: v.duration_seconds,
            },
            Self::ApproveSession(v) => CommandFingerprint::ApproveSession {
                session_id: v.session_id,
            },
            Self::RevokeSession(v) => CommandFingerprint::RevokeSession {
                session_id: v.session_id,
                reason: v.reason.clone(),
            },
            Self::IssueCapability(v) => CommandFingerprint::IssueCapability {
                capability_id: v.capability_id,
                session_id: v.session_id,
                device_ref: v.evidence.device_ref.clone(),
            },
            Self::RevokeCapability(v) => CommandFingerprint::RevokeCapability {
                capability_id: v.capability_id,
                reason: v.reason,
            },
            Self::OpenIncident(v) => CommandFingerprint::OpenIncident {
                incident_id: v.incident_id,
                incident_code: v.incident_code.clone(),
                severity: v.severity.clone(),
                affected_services: v.affected_services.clone(),
                affected_customer_accounts: v.affected_customer_accounts.clone(),
            },
            Self::TransitionIncident(v) => CommandFingerprint::TransitionIncident {
                incident_id: v.incident_id,
                next: v.next,
            },
            Self::AddIncidentTimeline(v) => CommandFingerprint::AddIncidentTimeline {
                incident_id: v.incident_id,
                message: v.message.clone(),
            },
            Self::BroadcastIncident(v) => CommandFingerprint::BroadcastIncident {
                incident_id: v.incident_id,
                audience: v.audience,
                message: v.message.clone(),
            },
        }
    }

    const fn is_create(&self) -> bool {
        matches!(
            self,
            Self::OpenTicket(_)
                | Self::RequestSession(_)
                | Self::IssueCapability(_)
                | Self::OpenIncident(_)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSupportTicket {
    pub ticket_id: SupportTicketId,
    pub audience: TicketReadAudience,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSupportTickets {
    pub filter: SupportTicketListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetIncident {
    pub incident_id: IncidentId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListIncidents {
    pub filter: IncidentListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSupportSession {
    pub session_id: SessionId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSupportCapability {
    pub capability_id: CapabilityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum SupportQuery {
    GetTicket(GetSupportTicket),
    ListTickets(ListSupportTickets),
    GetIncident(GetIncident),
    ListIncidents(ListIncidents),
    GetSession(GetSupportSession),
    GetCapability(GetSupportCapability),
}

impl SupportQuery {
    fn action(&self) -> SupportAction {
        match self {
            Self::GetTicket(v) => match v.audience {
                TicketReadAudience::Customer => SupportAction::ViewCustomerTicket,
                TicketReadAudience::Internal => SupportAction::ViewInternalTicket,
            },
            Self::ListTickets(v) => match v.filter.audience {
                TicketReadAudience::Customer => SupportAction::ListCustomerTickets,
                TicketReadAudience::Internal => SupportAction::ListInternalTickets,
            },
            Self::GetIncident(_) => SupportAction::ViewIncident,
            Self::ListIncidents(_) => SupportAction::ListIncidents,
            Self::GetSession(_) => SupportAction::ViewSession,
            Self::GetCapability(_) => SupportAction::ViewCapability,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum SupportMutationResult {
    Ticket(SupportTicketView),
    Incident(IncidentView),
    Session(SupportSessionView),
    Capability(SupportCapabilityView),
}

impl SupportMutationResult {
    fn from_aggregate(value: &SupportAggregate) -> Self {
        match value {
            SupportAggregate::Ticket(v) => Self::Ticket(SupportTicketView::from_aggregate(
                v,
                TicketReadAudience::Internal,
            )),
            SupportAggregate::Incident(v) => Self::Incident(v.into()),
            SupportAggregate::Session(v) => Self::Session(v.into()),
            SupportAggregate::Capability(v) => Self::Capability(v.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum SupportQueryResponse {
    Ticket(Option<SupportTicketView>),
    Tickets(SupportTicketList),
    Incident(Option<IncidentView>),
    Incidents(IncidentList),
    Session(Option<SupportSessionView>),
    Capability(Option<SupportCapabilityView>),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SupportApplicationError {
    #[error(transparent)]
    Domain(#[from] SupportDomainError),
    #[error("command scope does not contain the Support record scope")]
    ScopeOutsideAuthority,
    #[error("Support session approval requires a directly authenticated human")]
    HumanApprovalRequired,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("Support aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Support page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("Support integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct SupportCommandHandler<W, U, A, E, C> {
    write: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}
impl<W, U, A, E, C> SupportCommandHandler<W, U, A, E, C> {
    pub const fn new(
        write: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        clock: C,
    ) -> Self {
        Self {
            write,
            unit_of_work,
            authorization,
            entitlement,
            clock,
        }
    }
}

impl<W, U, A, E, C> SupportCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn preflight(
        &self,
        envelope: &CommandEnvelope<SupportCommand>,
        scope: &AuthorityScope,
        action: SupportAction,
        session_id: Option<SessionId>,
        device_ref: Option<&str>,
        capability_evidence: Option<&CapabilityEvidence>,
    ) -> Result<(), SupportApplicationError> {
        if !envelope.active_scope.contains(scope) {
            return Err(SupportApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_support_enabled(scope, action.requires_write_entitlement())
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: scope,
                action,
                session_id,
                device_ref,
                capability_evidence,
            })
            .await?;
        Ok(())
    }

    async fn replay(
        &self,
        envelope: &CommandEnvelope<SupportCommand>,
        identity: &CommandIdentity,
        action: SupportAction,
    ) -> Result<Option<StoredCommit>, SupportApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_receipt(identity).await? else {
            return Ok(None);
        };
        let (session_id, device_ref) = match &stored.aggregate {
            SupportAggregate::Capability(v) => (Some(v.session_id()), Some(v.device_ref())),
            SupportAggregate::Session(v) => (Some(v.id()), None),
            _ => (None, None),
        };
        self.preflight(
            envelope,
            stored.aggregate.scope(),
            action,
            session_id,
            device_ref,
            None,
        )
        .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    #[allow(clippy::result_large_err)]
    fn expected(expected: Option<u64>, actual: u64) -> Result<u64, SupportApplicationError> {
        let expected = expected.ok_or(SupportApplicationError::MissingExpectedVersion)?;
        if expected != actual {
            Err(SupportApplicationError::VersionConflict { expected, actual })
        } else {
            Ok(expected)
        }
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<SupportCommand> for SupportCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = SupportMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<SupportCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        if matches!(&envelope.command, SupportCommand::ApproveSession(_))
            && (!envelope.actor.is_human()
                || envelope.actor.session_id.is_none()
                || envelope.actor.represented_principal_ref.is_some()
                || envelope.actor.support_capability_id.is_some())
        {
            return Err(to_cqrs(
                SupportApplicationError::HumanApprovalRequired,
                correlation,
            ));
        }
        let action = envelope.command.action();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: envelope.command.fingerprint(),
        };
        let decision_at = matches!(
            envelope.command,
            SupportCommand::TransitionTicket(_) | SupportCommand::TransitionIncident(_)
        )
        .then(|| self.clock.now());
        let mut human_decision = match (&envelope.command, decision_at) {
            (SupportCommand::TransitionTicket(value), Some(at)) => Some(
                HumanDecisionProof::try_new(
                    envelope.actor.clone(),
                    envelope.active_scope.clone(),
                    crate::domain::ticket_transition_decision_ref(
                        envelope.command_id,
                        value.ticket_id,
                        envelope.expected_aggregate_version.unwrap_or(0),
                        value.next,
                    ),
                    at,
                )
                .map_err(|_| {
                    to_cqrs(SupportApplicationError::HumanApprovalRequired, correlation)
                })?,
            ),
            (SupportCommand::TransitionIncident(value), Some(at)) => Some(
                HumanDecisionProof::try_new(
                    envelope.actor.clone(),
                    envelope.active_scope.clone(),
                    crate::domain::incident_transition_decision_ref(
                        envelope.command_id,
                        value.incident_id,
                        envelope.expected_aggregate_version.unwrap_or(0),
                        value.next,
                    ),
                    at,
                )
                .map_err(|_| {
                    to_cqrs(SupportApplicationError::HumanApprovalRequired, correlation)
                })?,
            ),
            _ => None,
        };
        if let Some(stored) = self
            .replay(&envelope, &identity, action)
            .await
            .map_err(|e| to_cqrs(e, correlation))?
        {
            let response = SupportMutationResult::from_aggregate(&stored.aggregate);
            return Ok(stored.into_receipt(response));
        }
        if envelope.command.is_create() && envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                SupportApplicationError::UnexpectedCreateVersion,
                correlation,
            ));
        }
        let at = decision_at.unwrap_or_else(|| self.clock.now());
        let actor_ref = envelope.actor.principal_ref.clone();
        let (mut aggregate, expected_version, dependency, session_id, device_ref) = match &envelope
            .command
        {
            SupportCommand::OpenTicket(v) => (
                SupportAggregate::Ticket(
                    SupportTicket::open(
                        v.ticket_id,
                        envelope.active_scope.clone(),
                        v.customer.clone(),
                        v.case_code.clone(),
                        v.category.clone(),
                        v.severity.clone(),
                        v.description.clone(),
                        at,
                    )
                    .map_err(|e| to_cqrs(e.into(), correlation))?,
                ),
                None,
                None,
                None,
                None,
            ),
            SupportCommand::TransitionTicket(v) => {
                let mut value = self
                    .write
                    .load_ticket(&envelope.active_scope, v.ticket_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let expected = Self::expected(envelope.expected_aggregate_version, value.version())
                    .map_err(|e| to_cqrs(e, correlation))?;
                let decision = human_decision.as_ref().ok_or_else(|| {
                    to_cqrs(SupportApplicationError::HumanApprovalRequired, correlation)
                })?;
                value
                    .transition(decision, identity.command_id, expected, v.next, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    SupportAggregate::Ticket(value),
                    Some(expected),
                    None,
                    None,
                    None,
                )
            }
            SupportCommand::AddTicketMessage(v) => {
                let mut value = self
                    .write
                    .load_ticket(&envelope.active_scope, v.ticket_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let expected = Self::expected(envelope.expected_aggregate_version, value.version())
                    .map_err(|e| to_cqrs(e, correlation))?;
                let message = TicketMessage::new(
                    v.message_id,
                    actor_ref.clone(),
                    v.audience,
                    v.body.clone(),
                    at,
                )
                .map_err(|e| to_cqrs(e.into(), correlation))?;
                value
                    .add_message(message, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    SupportAggregate::Ticket(value),
                    Some(expected),
                    None,
                    None,
                    None,
                )
            }
            SupportCommand::RequestSession(v) => {
                if actor_ref != v.agent.as_uuid().to_string() {
                    return Err(to_cqrs(
                        SupportDomainError::InvalidCapabilityBinding.into(),
                        correlation,
                    ));
                }
                let ticket = self
                    .write
                    .load_ticket(&envelope.active_scope, v.ticket_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let session = SupportSession::request(
                    v.session_id,
                    &ticket,
                    v.agent,
                    v.represented_principal,
                    v.mode,
                    v.permissions.clone(),
                    v.approved_scope.clone(),
                    v.module.clone(),
                    v.reason.clone(),
                    Duration::seconds(v.duration_seconds),
                    at,
                )
                .map_err(|e| to_cqrs(e.into(), correlation))?;
                let guard = DependencyVersion {
                    aggregate_type: "support_ticket",
                    aggregate_id: ticket.id().as_uuid(),
                    expected_version: ticket.version(),
                };
                (
                    SupportAggregate::Session(session),
                    None,
                    Some(guard),
                    Some(v.session_id),
                    None,
                )
            }
            SupportCommand::ApproveSession(v) => {
                let mut value = self
                    .write
                    .load_session(&envelope.active_scope, v.session_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let expected = Self::expected(envelope.expected_aggregate_version, value.version())
                    .map_err(|e| to_cqrs(e, correlation))?;
                let decision = HumanDecisionProof::try_new(
                    envelope.actor.clone(),
                    value.scope().clone(),
                    support_session_approval_decision_ref(v.session_id),
                    at,
                )
                .map_err(|_| {
                    to_cqrs(SupportApplicationError::HumanApprovalRequired, correlation)
                })?;
                value
                    .approve(&decision)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                human_decision = Some(decision);
                (
                    SupportAggregate::Session(value),
                    Some(expected),
                    None,
                    Some(v.session_id),
                    None,
                )
            }
            SupportCommand::RevokeSession(v) => {
                let mut value = self
                    .write
                    .load_session(&envelope.active_scope, v.session_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let expected = Self::expected(envelope.expected_aggregate_version, value.version())
                    .map_err(|e| to_cqrs(e, correlation))?;
                value
                    .revoke(v.reason.clone(), at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    SupportAggregate::Session(value),
                    Some(expected),
                    None,
                    Some(v.session_id),
                    None,
                )
            }
            SupportCommand::IssueCapability(v) => {
                let session = self
                    .write
                    .load_session(&envelope.active_scope, v.session_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let capability = SupportCapability::issue(
                    v.capability_id,
                    &session,
                    v.evidence.clone(),
                    v.secret_hash_sha256,
                    at,
                )
                .map_err(|e| to_cqrs(e.into(), correlation))?;
                let guard = DependencyVersion {
                    aggregate_type: "support_session",
                    aggregate_id: session.id().as_uuid(),
                    expected_version: session.version(),
                };
                (
                    SupportAggregate::Capability(capability),
                    None,
                    Some(guard),
                    Some(v.session_id),
                    Some(v.evidence.device_ref.clone()),
                )
            }
            SupportCommand::RevokeCapability(v) => {
                let mut value = self
                    .write
                    .load_capability(&envelope.active_scope, v.capability_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let expected = Self::expected(envelope.expected_aggregate_version, value.version())
                    .map_err(|e| to_cqrs(e, correlation))?;
                value
                    .revoke(v.reason, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                let sid = value.session_id();
                let device = value.device_ref().to_owned();
                (
                    SupportAggregate::Capability(value),
                    Some(expected),
                    None,
                    Some(sid),
                    Some(device),
                )
            }
            SupportCommand::OpenIncident(v) => (
                SupportAggregate::Incident(
                    Incident::open(
                        v.incident_id,
                        envelope.active_scope.clone(),
                        v.incident_code.clone(),
                        v.severity.clone(),
                        actor_ref.clone(),
                        v.affected_services.clone(),
                        v.affected_customer_accounts.clone(),
                        at,
                    )
                    .map_err(|e| to_cqrs(e.into(), correlation))?,
                ),
                None,
                None,
                None,
                None,
            ),
            SupportCommand::TransitionIncident(v) => {
                let mut value = self
                    .write
                    .load_incident(&envelope.active_scope, v.incident_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let expected = Self::expected(envelope.expected_aggregate_version, value.version())
                    .map_err(|e| to_cqrs(e, correlation))?;
                let decision = human_decision.as_ref().ok_or_else(|| {
                    to_cqrs(SupportApplicationError::HumanApprovalRequired, correlation)
                })?;
                value
                    .transition(decision, identity.command_id, expected, v.next, at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    SupportAggregate::Incident(value),
                    Some(expected),
                    None,
                    None,
                    None,
                )
            }
            SupportCommand::AddIncidentTimeline(v) => {
                let mut value = self
                    .write
                    .load_incident(&envelope.active_scope, v.incident_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let expected = Self::expected(envelope.expected_aggregate_version, value.version())
                    .map_err(|e| to_cqrs(e, correlation))?;
                value
                    .add_timeline(actor_ref.clone(), v.message.clone(), at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    SupportAggregate::Incident(value),
                    Some(expected),
                    None,
                    None,
                    None,
                )
            }
            SupportCommand::BroadcastIncident(v) => {
                let mut value = self
                    .write
                    .load_incident(&envelope.active_scope, v.incident_id)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?
                    .ok_or_else(|| port_to_cqrs(PortError::NotFound, correlation))?;
                let expected = Self::expected(envelope.expected_aggregate_version, value.version())
                    .map_err(|e| to_cqrs(e, correlation))?;
                value
                    .broadcast(actor_ref.clone(), v.audience, v.message.clone(), at)
                    .map_err(|e| to_cqrs(e.into(), correlation))?;
                (
                    SupportAggregate::Incident(value),
                    Some(expected),
                    None,
                    None,
                    None,
                )
            }
        };
        self.preflight(
            &envelope,
            aggregate.scope(),
            action,
            session_id,
            device_ref.as_deref(),
            match &envelope.command {
                SupportCommand::IssueCapability(value) => Some(&value.evidence),
                _ => None,
            },
        )
        .await
        .map_err(|e| to_cqrs(e, correlation))?;
        let authority_effects = authority_effects(&envelope.command, at);
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events())
                .map_err(|_| to_cqrs(SupportApplicationError::IntegrationMapping, correlation))?;
        let stored = self
            .unit_of_work
            .commit(SupportCommit {
                identity,
                correlation_id: correlation,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.active_scope,
                audit: envelope.audit,
                human_decision,
                expected_version,
                dependency,
                aggregate,
                integration_events,
                authority_effects,
            })
            .await
            .map_err(|e| port_to_cqrs(e, correlation))?;
        let response = SupportMutationResult::from_aggregate(&stored.aggregate);
        Ok(stored.into_receipt(response))
    }
}

fn authority_effects(
    command: &SupportCommand,
    at: chrono::DateTime<chrono::Utc>,
) -> Vec<SupportAuthorityEffect> {
    match command {
        SupportCommand::TransitionTicket(value) if value.next == TicketStatus::Closed => {
            vec![SupportAuthorityEffect::TerminateTicketAccess {
                ticket_id: value.ticket_id,
                effective_at: at,
            }]
        }
        SupportCommand::RevokeSession(value) => {
            vec![SupportAuthorityEffect::TerminateSessionCapabilities {
                session_id: value.session_id,
                effective_at: at,
            }]
        }
        _ => Vec::new(),
    }
}

pub struct SupportQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}
impl<R, A, E> SupportQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<SupportQuery> for SupportQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = SupportQueryResponse;
    async fn handle(
        &self,
        envelope: QueryEnvelope<SupportQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.query.action();
        self.entitlement
            .ensure_support_enabled(&envelope.active_scope, false)
            .await
            .map_err(|e| port_to_cqrs(e, correlation))?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action,
                session_id: None,
                device_ref: None,
                capability_evidence: None,
            })
            .await
            .map_err(|e| port_to_cqrs(e, correlation))?;
        let (value, projection) = match &envelope.query {
            SupportQuery::GetTicket(v) => {
                let p = self
                    .read
                    .get_ticket(
                        &envelope.active_scope,
                        v.ticket_id,
                        v.audience,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (SupportQueryResponse::Ticket(p.value), p.metadata)
            }
            SupportQuery::ListTickets(v) => {
                validate_page(v.filter.page_size).map_err(|e| to_cqrs(e, correlation))?;
                let p = self
                    .read
                    .list_tickets(&envelope.active_scope, &v.filter, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (SupportQueryResponse::Tickets(p.value), p.metadata)
            }
            SupportQuery::GetIncident(v) => {
                let p = self
                    .read
                    .get_incident(&envelope.active_scope, v.incident_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (SupportQueryResponse::Incident(p.value), p.metadata)
            }
            SupportQuery::ListIncidents(v) => {
                validate_page(v.filter.page_size).map_err(|e| to_cqrs(e, correlation))?;
                let p = self
                    .read
                    .list_incidents(&envelope.active_scope, &v.filter, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (SupportQueryResponse::Incidents(p.value), p.metadata)
            }
            SupportQuery::GetSession(v) => {
                let p = self
                    .read
                    .get_session(&envelope.active_scope, v.session_id, &envelope.consistency)
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (SupportQueryResponse::Session(p.value), p.metadata)
            }
            SupportQuery::GetCapability(v) => {
                let p = self
                    .read
                    .get_capability(
                        &envelope.active_scope,
                        v.capability_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|e| port_to_cqrs(e, correlation))?;
                (SupportQueryResponse::Capability(p.value), p.metadata)
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

#[allow(clippy::result_large_err)]
fn validate_page(value: u16) -> Result<(), SupportApplicationError> {
    if value == 0 || value > MAX_SUPPORT_PAGE_SIZE {
        Err(SupportApplicationError::InvalidPageSize)
    } else {
        Ok(())
    }
}
fn port_to_cqrs(error: PortError, correlation: CorrelationId) -> CqrsError {
    if let PortError::NotReady(v) = error {
        CqrsError::NotReady(v)
    } else {
        to_cqrs(error.into(), correlation)
    }
}
fn to_cqrs(error: SupportApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, message, retryable) = match error {
        SupportApplicationError::ScopeOutsideAuthority
        | SupportApplicationError::HumanApprovalRequired
        | SupportApplicationError::Domain(SupportDomainError::HumanApprovalRequired)
        | SupportApplicationError::Port(PortError::Denied | PortError::EntitlementDenied) => (
            "support.denied",
            ErrorCategory::Denied,
            "Support operation is not allowed.",
            false,
        ),
        SupportApplicationError::Port(PortError::NotFound) => (
            "support.not_found",
            ErrorCategory::NotFound,
            "Support record was not found.",
            false,
        ),
        SupportApplicationError::MissingExpectedVersion
        | SupportApplicationError::VersionConflict { .. }
        | SupportApplicationError::Port(
            PortError::Conflict { .. } | PortError::IdempotencyConflict,
        ) => (
            "support.conflict",
            ErrorCategory::Conflict,
            "Support record changed or command identity conflicts.",
            false,
        ),
        SupportApplicationError::Port(PortError::Unavailable) => (
            "support.unavailable",
            ErrorCategory::Unavailable,
            "Support is temporarily unavailable.",
            true,
        ),
        SupportApplicationError::Domain(_)
        | SupportApplicationError::UnexpectedCreateVersion
        | SupportApplicationError::InvalidPageSize => (
            "support.invalid",
            ErrorCategory::Invalid,
            "Support input is invalid.",
            false,
        ),
        SupportApplicationError::IntegrationMapping
        | SupportApplicationError::Port(PortError::Internal | PortError::NotReady(_)) => (
            "support.internal",
            ErrorCategory::Internal,
            "Support could not complete the operation.",
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
