//! Native Support ticket workspace and detail read contract.
pub const CONTRACT_VERSION: &str = "support.ticket-read.v1";

pub use crate::adapters::support_ticket_read::{
    SupportOrderSource, SupportReadScope, SupportRecipient, SupportSlaPolicyChoice,
    SupportSlaPolicyRecord, SupportSlaTicketRecord, SupportSourceReference,
    SupportTicketWorkspaceRow, parse_source_link, read_customer_contact_notes,
    read_evidence_items, read_evidence_ticket, read_incident_candidates, read_incident_detail,
    read_incident_history, read_incident_links, read_incident_workspace,
    read_incident_workspace_tickets, read_order_recipients, read_order_sources,
    read_sla_alerts, read_sla_policies, read_sla_policy_records, read_sla_tickets,
    read_session_capabilities, read_session_capability_history, read_session_context,
    read_session_context_evidence, read_session_history, read_session_tickets,
    read_session_workspace, read_ticket_actor_eligible, read_ticket_detail,
    read_ticket_history, read_ticket_incidents, read_ticket_recipients, read_ticket_workspace,
};
