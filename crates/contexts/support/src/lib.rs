#![doc = "Support bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AddIncidentTimeline, AddTicketMessage, ApproveSupportSession, BroadcastIncident, GetIncident,
    GetSupportCapability, GetSupportSession, GetSupportTicket, IssueSupportCapability,
    ListIncidents, ListSupportTickets, OpenIncident, OpenSupportTicket, PlatformCaseCommandHandler,
    RequestSupportSession, RevokeSupportCapability, RevokeSupportSession, SupportApplicationError,
    SupportCommand, SupportCommandHandler, SupportMutationResult, SupportQuery,
    SupportQueryHandler, SupportQueryResponse, TransitionIncident, TransitionSupportTicket,
};
pub use domain::{
    BroadcastAudience, CapabilityEvidence, CapabilityId, CapabilityRevocationReason, Incident,
    IncidentBroadcast, IncidentId, IncidentStatus, IncidentTimelineEntry, MessageAudience,
    MessageId, PlatformCaseStatus, PlatformSupportCase, SessionId, SessionStatus, SupportAggregate,
    SupportCapability, SupportDomainError, SupportMode, SupportSession, SupportTicket,
    SupportTicketId, TicketMessage, TicketStatus, incident_transition_decision_ref,
    support_session_approval_decision_ref, ticket_transition_decision_ref,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, IncidentList, IncidentListFilter,
    IncidentView, PlatformCaseAuthorizationPort, PlatformCaseCommand, PlatformCaseQueryPort,
    PlatformCaseReceipt, PlatformCaseUnitOfWork, PlatformCaseView, PortError, ReadProjection,
    ReadRepository, StoredCommit, SupportAction, SupportAuthorityEffect, SupportCapabilityView,
    SupportCommit, SupportSessionView, SupportTicketList, SupportTicketListFilter,
    SupportTicketView, TicketReadAudience, WriteRepository,
};
