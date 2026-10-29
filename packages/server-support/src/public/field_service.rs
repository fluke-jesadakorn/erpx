//! Factual original ticket identity and intended deferred projection only.
pub use crate::adapters::field_service::{
    CurrentSupportServiceTicket, PreparedSupportServicePartsProjection,
    SupportCompletionProjection, SupportServicePartsAction, SupportServicePartsProjection,
    SupportServiceTicketFacts, SupportServiceTicketRequest, lock_service_ticket_sources,
    read_current_service_ticket,
};
