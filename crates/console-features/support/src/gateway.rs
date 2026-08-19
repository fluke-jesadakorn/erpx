use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_support::{
    BroadcastAudience, CapabilityRevocationReason, IncidentStatus, MessageAudience, SessionStatus,
    SupportCommand, SupportMode, SupportMutationResult, SupportQuery, SupportQueryResponse,
    TicketStatus,
    ports::{
        IncidentList, IncidentView, SupportCapabilityView, SupportSessionView, SupportTicketList,
        SupportTicketView,
    },
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{
        CapabilityMetadataItem, CustomerMessageItem, CustomerTicketItem, CustomerTicketItems,
        IncidentBroadcastItem, IncidentItem, IncidentItems, IncidentTimelineItem,
        InternalMessageItem, InternalTicketItem, InternalTicketItems, SessionItem,
    },
    presenter::SupportContent,
};

pub fn present_mutation(value: &SupportMutationResult) -> SupportContent {
    match value {
        SupportMutationResult::Ticket(value) => present_ticket(value.clone()),
        SupportMutationResult::Incident(value) => SupportContent::Incident(value.clone().into()),
        SupportMutationResult::Session(value) => SupportContent::Session(value.clone().into()),
        SupportMutationResult::Capability(value) => {
            SupportContent::CapabilityMetadata(value.clone().into())
        }
    }
}

pub fn present_query(
    value: QueryResult<SupportQueryResponse>,
) -> QueryResult<Option<SupportContent>> {
    let metadata = value.metadata;
    let value = match value.value {
        SupportQueryResponse::Ticket(value) => value.map(present_ticket),
        SupportQueryResponse::Tickets(value) => present_ticket_list(value),
        SupportQueryResponse::Incident(value) => {
            value.map(IncidentItem::from).map(SupportContent::Incident)
        }
        SupportQueryResponse::Incidents(value) => {
            let items = IncidentItems::from(value);
            (!items.incidents.is_empty())
                .then_some(items)
                .map(SupportContent::Incidents)
        }
        SupportQueryResponse::Session(value) => {
            value.map(SessionItem::from).map(SupportContent::Session)
        }
        SupportQueryResponse::Capability(value) => value
            .map(CapabilityMetadataItem::from)
            .map(SupportContent::CapabilityMetadata),
    };
    QueryResult { value, metadata }
}

fn present_ticket(value: SupportTicketView) -> SupportContent {
    if value.internal_messages.is_some() {
        SupportContent::InternalTicket(value.into())
    } else {
        SupportContent::CustomerTicket(value.into())
    }
}

fn present_ticket_list(value: SupportTicketList) -> Option<SupportContent> {
    if value.tickets.is_empty() {
        return None;
    }
    if value
        .tickets
        .first()
        .is_some_and(|ticket| ticket.internal_messages.is_some())
    {
        Some(SupportContent::InternalTickets(value.into()))
    } else {
        Some(SupportContent::CustomerTickets(value.into()))
    }
}

impl From<SupportTicketView> for CustomerTicketItem {
    fn from(value: SupportTicketView) -> Self {
        Self {
            ticket_id: value.ticket_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            customer_id: value.customer.as_uuid().to_string(),
            customer_account_id: value.customer_account.as_uuid().to_string(),
            case_code: value.case_code,
            category: value.category,
            severity: value.severity,
            description: value.description,
            status: ticket_status(value.status).into(),
            messages: value
                .customer_messages
                .into_iter()
                .map(|message| CustomerMessageItem {
                    message_id: message.message_id.as_uuid().to_string(),
                    author_ref: message.author_ref,
                    body: message.body,
                    created_at: message.created_at.to_rfc3339(),
                })
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<SupportTicketView> for InternalTicketItem {
    fn from(value: SupportTicketView) -> Self {
        Self {
            ticket_id: value.ticket_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            customer_id: value.customer.as_uuid().to_string(),
            customer_account_id: value.customer_account.as_uuid().to_string(),
            case_code: value.case_code,
            category: value.category,
            severity: value.severity,
            description: value.description,
            status: ticket_status(value.status).into(),
            messages: value
                .internal_messages
                .unwrap_or_default()
                .into_iter()
                .map(|message| InternalMessageItem {
                    message_id: message.message_id.as_uuid().to_string(),
                    author_ref: message.author_ref,
                    audience: message_audience(message.audience).into(),
                    body: message.body,
                    created_at: message.created_at.to_rfc3339(),
                })
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<SupportTicketList> for CustomerTicketItems {
    fn from(value: SupportTicketList) -> Self {
        Self {
            tickets: value
                .tickets
                .into_iter()
                .map(CustomerTicketItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<SupportTicketList> for InternalTicketItems {
    fn from(value: SupportTicketList) -> Self {
        Self {
            tickets: value
                .tickets
                .into_iter()
                .map(InternalTicketItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<IncidentView> for IncidentItem {
    fn from(value: IncidentView) -> Self {
        Self {
            incident_id: value.incident_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            incident_code: value.incident_code,
            severity: value.severity,
            commander_ref: value.commander_ref,
            affected_services: value.affected_services,
            affected_customer_accounts: value
                .affected_customer_accounts
                .into_iter()
                .map(|account| account.as_uuid().to_string())
                .collect(),
            status: incident_status(value.status).into(),
            timeline: value
                .timeline
                .into_iter()
                .map(|entry| IncidentTimelineItem {
                    author_ref: entry.actor_ref,
                    message: entry.message,
                    created_at: entry.at.to_rfc3339(),
                })
                .collect(),
            broadcasts: value
                .broadcasts
                .into_iter()
                .map(|entry| IncidentBroadcastItem {
                    author_ref: entry.actor_ref,
                    audience: broadcast_audience(entry.audience).into(),
                    message: entry.message,
                    created_at: entry.at.to_rfc3339(),
                })
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<IncidentList> for IncidentItems {
    fn from(value: IncidentList) -> Self {
        Self {
            incidents: value
                .incidents
                .into_iter()
                .map(IncidentItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<SupportSessionView> for SessionItem {
    fn from(value: SupportSessionView) -> Self {
        Self {
            session_id: value.session_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            ticket_id: value.ticket_id.as_uuid().to_string(),
            customer_id: value.customer.as_uuid().to_string(),
            customer_account_id: value.customer_account.as_uuid().to_string(),
            agent_principal_id: value.agent.as_uuid().to_string(),
            represented_principal_id: value
                .represented_principal
                .map(|principal| principal.as_uuid().to_string()),
            mode: support_mode(value.mode).into(),
            permissions: value.permissions,
            module: value.module,
            reason: value.reason,
            status: session_status(value.status).into(),
            requested_at: value.requested_at.to_rfc3339(),
            expires_at: value.expires_at.to_rfc3339(),
            approved_by: value.approved_by,
            approved_at: value.approved_at.map(|at| at.to_rfc3339()),
            revoked_at: value.revoked_at.map(|at| at.to_rfc3339()),
            revoked_reason: value.revoked_reason,
        }
    }
}

impl From<SupportCapabilityView> for CapabilityMetadataItem {
    fn from(value: SupportCapabilityView) -> Self {
        let (revocation_reason, revoked_at) = value
            .revocation
            .map(|(reason, at)| {
                (
                    Some(capability_revocation(reason).into()),
                    Some(at.to_rfc3339()),
                )
            })
            .unwrap_or((None, None));
        Self {
            capability_id: value.capability_id.as_uuid().to_string(),
            version: value.version,
            session_id: value.session_id.as_uuid().to_string(),
            ticket_id: value.ticket_id.as_uuid().to_string(),
            customer_id: value.customer.as_uuid().to_string(),
            customer_account_id: value.customer_account.as_uuid().to_string(),
            agent_principal_id: value.agent.as_uuid().to_string(),
            device_ref: value.device_ref,
            mode: support_mode(value.mode).into(),
            permissions: value.permissions,
            scope: format!("{:?}", value.scope),
            module: value.module,
            issued_at: value.issued_at.to_rfc3339(),
            expires_at: value.expires_at.to_rfc3339(),
            revocation_reason,
            revoked_at,
        }
    }
}

const fn ticket_status(value: TicketStatus) -> &'static str {
    match value {
        TicketStatus::New => "new",
        TicketStatus::Triaged => "triaged",
        TicketStatus::WaitingForCustomer => "waiting_for_customer",
        TicketStatus::AccessRequested => "access_requested",
        TicketStatus::Approved => "approved",
        TicketStatus::InProgress => "in_progress",
        TicketStatus::Resolved => "resolved",
        TicketStatus::CustomerConfirmed => "customer_confirmed",
        TicketStatus::Closed => "closed",
    }
}
const fn incident_status(value: IncidentStatus) -> &'static str {
    match value {
        IncidentStatus::Investigating => "investigating",
        IncidentStatus::Identified => "identified",
        IncidentStatus::Monitoring => "monitoring",
        IncidentStatus::Resolved => "resolved",
        IncidentStatus::PostmortemComplete => "postmortem_complete",
    }
}
const fn session_status(value: SessionStatus) -> &'static str {
    match value {
        SessionStatus::Requested => "requested",
        SessionStatus::Approved => "approved",
        SessionStatus::Revoked => "revoked",
    }
}
const fn support_mode(value: SupportMode) -> &'static str {
    match value {
        SupportMode::DiagnosticRead => "diagnostic_read",
        SupportMode::HoldingMutation => "holding_mutation",
        SupportMode::Impersonation => "impersonation",
    }
}
const fn message_audience(value: MessageAudience) -> &'static str {
    match value {
        MessageAudience::CustomerVisible => "customer_visible",
        MessageAudience::Internal => "internal",
    }
}
const fn broadcast_audience(value: BroadcastAudience) -> &'static str {
    match value {
        BroadcastAudience::CustomerVisible => "customer_visible",
        BroadcastAudience::Internal => "internal",
    }
}
const fn capability_revocation(value: CapabilityRevocationReason) -> &'static str {
    match value {
        CapabilityRevocationReason::CustomerRequested => "customer_requested",
        CapabilityRevocationReason::AgentDisabled => "agent_disabled",
        CapabilityRevocationReason::DeviceRevoked => "device_revoked",
        CapabilityRevocationReason::SessionRevoked => "session_revoked",
        CapabilityRevocationReason::TicketClosed => "ticket_closed",
        CapabilityRevocationReason::Administrative => "administrative",
    }
}

/// Generated protobuf and channel details stop behind this boundary. The
/// Console has no capability-issuance command constructor.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait SupportProtoTransport: Send + Sync {
    async fn command(
        &self,
        envelope: CommandEnvelope<SupportCommand>,
    ) -> Result<CommandReceipt<SupportMutationResult>, UiError>;
    async fn query(
        &self,
        envelope: QueryEnvelope<SupportQuery>,
    ) -> Result<QueryResult<SupportQueryResponse>, UiError>;
}

#[derive(Debug, Clone)]
pub struct SupportGateway<T> {
    transport: T,
}
impl<T> SupportGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: SupportProtoTransport> CommandGateway<SupportCommand> for SupportGateway<T> {
    type Response = SupportMutationResult;
    async fn execute(
        &self,
        envelope: CommandEnvelope<SupportCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.command(envelope).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: SupportProtoTransport> QueryGateway<SupportQuery> for SupportGateway<T> {
    type Response = SupportQueryResponse;
    async fn execute(
        &self,
        envelope: QueryEnvelope<SupportQuery>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.query(envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl SupportProtoTransport for Fake {
        async fn command(
            &self,
            _: CommandEnvelope<SupportCommand>,
        ) -> Result<CommandReceipt<SupportMutationResult>, UiError> {
            unreachable!()
        }
        async fn query(
            &self,
            _: QueryEnvelope<SupportQuery>,
        ) -> Result<QueryResult<SupportQueryResponse>, UiError> {
            unreachable!()
        }
    }

    #[test]
    fn gateway_is_typed_for_both_directions() {
        fn command<G: CommandGateway<SupportCommand, Response = SupportMutationResult>>() {}
        fn query<G: QueryGateway<SupportQuery, Response = SupportQueryResponse>>() {}
        command::<SupportGateway<Fake>>();
        query::<SupportGateway<Fake>>();
    }

    #[test]
    fn console_has_no_capability_issuance_surface() {
        let source = include_str!("gateway.rs");
        assert!(!source.contains(concat!("IssueSupport", "Capability")));
        assert!(!source.contains(concat!("Issue", "Capability")));
    }

    #[test]
    fn gateway_is_the_only_core_to_ui_mapping_boundary() {
        let source = include_str!("gateway.rs");
        assert!(source.contains(concat!("impl From<Support", "TicketView>")));
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("tonic", "::")));
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "support")));
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
