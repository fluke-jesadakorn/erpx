#![doc = "Support bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AddIncidentTimeline, AddTicketMessage, ApproveSupportSession, BroadcastIncident, GetIncident,
    GetSupportCapability, GetSupportSession, GetSupportTicket, IssueSupportCapability,
    ListIncidents, ListSupportTickets, OpenIncident, OpenSupportTicket, RequestSupportSession,
    RevokeSupportCapability, RevokeSupportSession, SupportApplicationError, SupportCommand,
    SupportCommandHandler, SupportMutationResult, SupportQuery, SupportQueryHandler,
    SupportQueryResponse, TransitionIncident, TransitionSupportTicket,
};
pub use domain::{
    BroadcastAudience, CapabilityEvidence, CapabilityId, CapabilityRevocationReason, Incident,
    IncidentId, IncidentStatus, MessageAudience, MessageId, SessionId, SessionStatus,
    SupportAggregate, SupportCapability, SupportDomainError, SupportMode, SupportSession,
    SupportTicket, SupportTicketId, TicketMessage, TicketStatus,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, IncidentList, IncidentListFilter,
    IncidentView, PortError, ReadProjection, ReadRepository, StoredCommit, SupportAction,
    SupportCapabilityView, SupportCommit, SupportSessionView, SupportTicketList,
    SupportTicketListFilter, SupportTicketView, TicketReadAudience, WriteRepository,
};
