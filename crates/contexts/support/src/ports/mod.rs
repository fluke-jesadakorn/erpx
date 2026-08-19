use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::{customer, identity_access, support};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    BroadcastAudience, CapabilityEvidence, CapabilityId, CapabilityRevocationReason, Incident,
    IncidentId, IncidentStatus, MessageAudience, MessageId, SessionId, SupportAggregate,
    SupportCapability, SupportMode, SupportSession, SupportTicket, SupportTicketId, TicketStatus,
};

mod platform_case;
pub use platform_case::{
    PlatformCaseAuthorizationPort, PlatformCaseCommand, PlatformCaseQueryPort, PlatformCaseReceipt,
    PlatformCaseUnitOfWork, PlatformCaseView,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportAction {
    OpenTicket,
    TransitionTicket,
    AddCustomerMessage,
    AddInternalMessage,
    ViewCustomerTicket,
    ViewInternalTicket,
    ListCustomerTickets,
    ListInternalTickets,
    RequestSession,
    ApproveSession,
    RevokeSession,
    ViewSession,
    IssueCapability,
    RevokeCapability,
    ViewCapability,
    OpenIncident,
    TransitionIncident,
    AddIncidentTimeline,
    BroadcastCustomerIncident,
    BroadcastInternalIncident,
    ViewIncident,
    ListIncidents,
}

impl SupportAction {
    pub const fn requires_write_entitlement(self) -> bool {
        !matches!(
            self,
            Self::ViewCustomerTicket
                | Self::ViewInternalTicket
                | Self::ListCustomerTickets
                | Self::ListInternalTickets
                | Self::ViewSession
                | Self::ViewCapability
                | Self::ViewIncident
                | Self::ListIncidents
        )
    }

    pub const fn requires_current_mfa(self) -> bool {
        matches!(
            self,
            Self::ApproveSession | Self::IssueCapability | Self::RevokeCapability
        )
    }

    pub const fn requires_segregation_of_duties(self) -> bool {
        matches!(self, Self::ApproveSession | Self::IssueCapability)
    }
}

pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: SupportAction,
    /// Capability issuance and use are denied unless the adapter rechecks the
    /// current Identity session/device state. This is never satisfied from a
    /// stale Support snapshot.
    pub session_id: Option<SessionId>,
    pub device_ref: Option<&'a str>,
    pub capability_evidence: Option<&'a CapabilityEvidence>,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_support_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    OpenTicket {
        ticket_id: SupportTicketId,
        customer: customer::CustomerProfileFact,
        case_code: String,
        category: String,
        severity: String,
        description: String,
    },
    TransitionTicket {
        ticket_id: SupportTicketId,
        next: TicketStatus,
    },
    AddTicketMessage {
        ticket_id: SupportTicketId,
        message_id: MessageId,
        audience: MessageAudience,
        body: String,
    },
    RequestSession {
        session_id: SessionId,
        ticket_id: SupportTicketId,
        agent: identity_access::PrincipalRef,
        represented_principal: Option<identity_access::PrincipalRef>,
        mode: SupportMode,
        permissions: Vec<String>,
        scope: AuthorityScope,
        module: String,
        reason: String,
        duration_seconds: i64,
    },
    ApproveSession {
        session_id: SessionId,
    },
    RevokeSession {
        session_id: SessionId,
        reason: String,
    },
    IssueCapability {
        capability_id: CapabilityId,
        session_id: SessionId,
        device_ref: String,
    },
    RevokeCapability {
        capability_id: CapabilityId,
        reason: CapabilityRevocationReason,
    },
    OpenIncident {
        incident_id: IncidentId,
        incident_code: String,
        severity: String,
        affected_services: Vec<String>,
        affected_customer_accounts: Vec<customer::CustomerAccountRef>,
    },
    TransitionIncident {
        incident_id: IncidentId,
        next: IncidentStatus,
    },
    AddIncidentTimeline {
        incident_id: IncidentId,
        message: String,
    },
    BroadcastIncident {
        incident_id: IncidentId,
        audience: BroadcastAudience,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_ticket(
        &self,
        authority_scope: &AuthorityScope,
        id: SupportTicketId,
    ) -> Result<Option<SupportTicket>, PortError>;
    async fn load_incident(
        &self,
        authority_scope: &AuthorityScope,
        id: IncidentId,
    ) -> Result<Option<Incident>, PortError>;
    async fn load_session(
        &self,
        authority_scope: &AuthorityScope,
        id: SessionId,
    ) -> Result<Option<SupportSession>, PortError>;
    async fn load_capability(
        &self,
        authority_scope: &AuthorityScope,
        id: CapabilityId,
    ) -> Result<Option<SupportCapability>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyVersion {
    pub aggregate_type: &'static str,
    pub aggregate_id: uuid::Uuid,
    pub expected_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    /// A create derived from another aggregate (session from ticket,
    /// capability from session) carries this optimistic guard in the same
    /// database transaction as snapshot/receipt/audit/outbox.
    pub dependency: Option<DependencyVersion>,
    pub aggregate: SupportAggregate,
    pub integration_events: Vec<support::Event>,
    /// Same-context revocation work emitted atomically with the owning change.
    pub authority_effects: Vec<SupportAuthorityEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum SupportAuthorityEffect {
    TerminateTicketAccess {
        ticket_id: SupportTicketId,
        effective_at: DateTime<Utc>,
    },
    TerminateSessionCapabilities {
        session_id: SessionId,
        effective_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: SupportAggregate,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "support".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("validated Support aggregate identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;
    /// Atomically persists aggregate snapshot, stable receipt, audit and
    /// published outbox events while checking expected/dependency versions.
    async fn commit(&self, request: SupportCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketReadAudience {
    Customer,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerTicketMessageView {
    pub message_id: MessageId,
    pub author_ref: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalTicketMessageView {
    pub message_id: MessageId,
    pub author_ref: String,
    pub audience: MessageAudience,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicketView {
    pub ticket_id: SupportTicketId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub customer: customer::CustomerRef,
    pub customer_account: customer::CustomerAccountRef,
    pub case_code: String,
    pub category: String,
    pub severity: String,
    pub description: String,
    pub status: TicketStatus,
    /// Always safe for the customer audience.
    pub customer_messages: Vec<CustomerTicketMessageView>,
    /// `None` is materially different from an empty list: it proves the read
    /// projection did not select internal rows for a customer request.
    pub internal_messages: Option<Vec<InternalTicketMessageView>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SupportTicketView {
    pub fn from_aggregate(value: &SupportTicket, audience: TicketReadAudience) -> Self {
        let customer_messages = value
            .messages()
            .iter()
            .filter(|message| message.audience() == MessageAudience::CustomerVisible)
            .map(|message| CustomerTicketMessageView {
                message_id: message.id(),
                author_ref: message.author_ref().into(),
                body: message.body().into(),
                created_at: message.created_at(),
            })
            .collect();
        let internal_messages = (audience == TicketReadAudience::Internal).then(|| {
            value
                .messages()
                .iter()
                .map(|message| InternalTicketMessageView {
                    message_id: message.id(),
                    author_ref: message.author_ref().into(),
                    audience: message.audience(),
                    body: message.body().into(),
                    created_at: message.created_at(),
                })
                .collect()
        });
        Self {
            ticket_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            customer: value.customer(),
            customer_account: value.customer_account(),
            case_code: value.case_code().into(),
            category: value.category().into(),
            severity: value.severity().into(),
            description: value.description().into(),
            status: value.status(),
            customer_messages,
            internal_messages,
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicketListFilter {
    pub status: Option<TicketStatus>,
    pub customer: Option<customer::CustomerRef>,
    pub audience: TicketReadAudience,
    pub page_size: u16,
    pub cursor: Option<String>,
}

impl Default for SupportTicketListFilter {
    fn default() -> Self {
        Self {
            status: None,
            customer: None,
            audience: TicketReadAudience::Customer,
            page_size: 50,
            cursor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicketList {
    pub tickets: Vec<SupportTicketView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentView {
    pub incident_id: IncidentId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub incident_code: String,
    pub severity: String,
    pub commander_ref: String,
    pub affected_services: Vec<String>,
    pub affected_customer_accounts: Vec<customer::CustomerAccountRef>,
    pub status: IncidentStatus,
    pub timeline: Vec<crate::domain::IncidentTimelineEntry>,
    pub broadcasts: Vec<crate::domain::IncidentBroadcast>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Incident> for IncidentView {
    fn from(value: &Incident) -> Self {
        Self {
            incident_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            incident_code: value.incident_code().into(),
            severity: value.severity().into(),
            commander_ref: value.commander_ref().into(),
            affected_services: value.affected_services().to_vec(),
            affected_customer_accounts: value.affected_customer_accounts().to_vec(),
            status: value.status(),
            timeline: value.timeline().to_vec(),
            broadcasts: value.broadcasts().to_vec(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IncidentListFilter {
    pub status: Option<IncidentStatus>,
    pub page_size: u16,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentList {
    pub incidents: Vec<IncidentView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportSessionView {
    pub session_id: SessionId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub ticket_id: SupportTicketId,
    pub customer: customer::CustomerRef,
    pub customer_account: customer::CustomerAccountRef,
    pub agent: identity_access::PrincipalRef,
    pub represented_principal: Option<identity_access::PrincipalRef>,
    pub mode: SupportMode,
    pub permissions: Vec<String>,
    pub module: String,
    pub reason: String,
    pub status: crate::domain::SessionStatus,
    pub requested_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
}

impl From<&SupportSession> for SupportSessionView {
    fn from(v: &SupportSession) -> Self {
        Self {
            session_id: v.id(),
            version: v.version(),
            scope: v.scope().clone(),
            ticket_id: v.ticket_id(),
            customer: v.customer(),
            customer_account: v.customer_account(),
            agent: v.agent(),
            represented_principal: v.represented_principal(),
            mode: v.mode(),
            permissions: v.permissions().to_vec(),
            module: v.module().into(),
            reason: v.reason().into(),
            status: v.status(),
            requested_at: v.requested_at(),
            expires_at: v.expires_at(),
            approved_by: v.approved_by().map(Into::into),
            approved_at: v.approved_at(),
            revoked_at: v.revoked_at(),
            revoked_reason: v.revoked_reason().map(Into::into),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportCapabilityView {
    pub capability_id: CapabilityId,
    pub version: u64,
    pub session_id: SessionId,
    pub ticket_id: SupportTicketId,
    pub customer: customer::CustomerRef,
    pub customer_account: customer::CustomerAccountRef,
    pub agent: identity_access::PrincipalRef,
    pub device_ref: String,
    pub mode: SupportMode,
    pub permissions: Vec<String>,
    pub scope: AuthorityScope,
    pub module: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revocation: Option<(CapabilityRevocationReason, DateTime<Utc>)>,
}

impl std::fmt::Debug for SupportCapabilityView {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SupportCapabilityView")
            .field("capability_id", &self.capability_id)
            .field("session_id", &self.session_id)
            .field(
                "status",
                &self.revocation.map(|_| "revoked").unwrap_or("active"),
            )
            .finish_non_exhaustive()
    }
}

impl From<&SupportCapability> for SupportCapabilityView {
    fn from(v: &SupportCapability) -> Self {
        Self {
            capability_id: v.id(),
            version: v.version(),
            session_id: v.session_id(),
            ticket_id: v.ticket_id(),
            customer: v.customer(),
            customer_account: v.customer_account(),
            agent: v.agent(),
            device_ref: v.device_ref().into(),
            mode: v.mode(),
            permissions: v.permissions().to_vec(),
            scope: v.scope().clone(),
            module: v.module().into(),
            issued_at: v.issued_at(),
            expires_at: v.expires_at(),
            revocation: v.revocation(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_ticket(
        &self,
        authority_scope: &AuthorityScope,
        id: SupportTicketId,
        audience: TicketReadAudience,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SupportTicketView>>, PortError>;
    async fn list_tickets(
        &self,
        authority_scope: &AuthorityScope,
        filter: &SupportTicketListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SupportTicketList>, PortError>;
    async fn get_incident(
        &self,
        authority_scope: &AuthorityScope,
        id: IncidentId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<IncidentView>>, PortError>;
    async fn list_incidents(
        &self,
        authority_scope: &AuthorityScope,
        filter: &IncidentListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<IncidentList>, PortError>;
    async fn get_session(
        &self,
        authority_scope: &AuthorityScope,
        id: SessionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SupportSessionView>>, PortError>;
    async fn get_capability(
        &self,
        authority_scope: &AuthorityScope,
        id: CapabilityId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SupportCapabilityView>>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Support operation is denied")]
    Denied,
    #[error("Support entitlement is not enabled")]
    EntitlementDenied,
    #[error("Support record was not found")]
    NotFound,
    #[error("Support aggregate version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("Support command identity was reused with a different fingerprint")]
    IdempotencyConflict,
    #[error("Support projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("Support storage is temporarily unavailable")]
    Unavailable,
    #[error("Support adapter failed")]
    Internal,
}
