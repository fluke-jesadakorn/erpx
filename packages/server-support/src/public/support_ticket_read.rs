//! Native Support ticket workspace and detail read contract.
pub const CONTRACT_VERSION: &str = "support.ticket-read.v1";

pub use crate::adapters::support_ticket_read::{
    SupportOrderSource, SupportReadScope, SupportRecipient, SupportSlaPolicyChoice,
    SupportSourceReference, SupportTicketWorkspaceRow, parse_source_link,
    read_customer_contact_notes, read_order_recipients, read_order_sources, read_sla_policies,
    read_ticket_detail, read_ticket_history, read_ticket_incidents, read_ticket_recipients,
    read_ticket_workspace,
};
