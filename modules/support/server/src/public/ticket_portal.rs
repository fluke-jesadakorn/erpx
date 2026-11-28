//! Native Support customer ticket-portal contract.
//!
//! Composition supplies the current staff/customer authority, purchase purpose,
//! workflow receipt/audit/outbox and Holding transaction. This port owns the
//! reviewed summary, independent review, version/CAS and acknowledgement rules.
pub const CONTRACT_VERSION: &str = "support.ticket-portal.v1";

pub use crate::adapters::ticket_portal_pg::PgTicketPortal;

pub use crate::adapters::ticket_portal::{
    acknowledged_at, acknowledgement_prior, apply_customer_acknowledgement, apply_plan,
    binding_for_staff, binding_image, current_response, customer_binding, decision_replay_proof,
    guard_binding_image, guard_customer_binding, lock_ticket_portal_sources, portal_installed,
    record_customer_projection, response_replay_proof, staff_workspace, ticket_for_staff,
    ticket_image,
};

pub use crate::application::ticket_portal::{
    PortalPurpose, PriorAcknowledgement, TicketPortalPorts, acknowledge, decide, prepare, respond,
};

pub use crate::domain::ticket_portal::{
    AcknowledgementInput, AcknowledgementResult, BindingInsert, BindingStatus, DecisionAction,
    DecisionInput, DecisionResult, EventInsert, MAX_ID, MAX_LIST, MAX_MESSAGE, MAX_NOTE,
    MAX_REASON, MAX_SUMMARY, PortalActor, PortalBindingRow, PortalBindingView, PortalCustomer,
    PortalPlan, PortalProjection, PortalProjectionItem, PortalResponse,
    PortalResponse as PortalPublishedResponse, PortalRights, PortalTicket, PortalWorkspace,
    PrepareInput, PrepareResult, ResponseInput, ResponseInsert, ResponseResult, binding_view,
    can_prepare, parse_acknowledgement, parse_decision, parse_prepare, parse_response,
    parse_selection, plan_acknowledgement, plan_decision, plan_prepare, plan_response,
    response_version, validate_command_key,
};
