//! Field-managed Support history; no generic Support mutation authority.
pub const CONTRACT_VERSION: &str = "support.field-ticket-read.v1";
pub use crate::adapters::field_ticket_read::{
    CurrentSupportTicketRead, read_field_ticket, read_support_record_ticket,
};
