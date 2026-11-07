//! HTTP-only parsed credential evidence and captured CAS, never an actor/grant.
pub use crate::adapters::account_inbound_http::{
    CapturedHoldingMutation, ParsedAccountRequest, holding_mutation_request,
    parse_inbound_account_request, unique_request_header,
};
