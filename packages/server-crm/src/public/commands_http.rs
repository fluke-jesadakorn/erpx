//! Shared owner-local CRM command HTTP transport facts.
pub use crate::adapters::commands_http::{
    COMMAND_BODY_LIMIT, CommandHttpFuture, CrmCommandHttpReply, HeaderRejected, bounded, configure,
    error_response, missing, parse_command, status, target, unique_header, valid_origin,
    valid_slug,
};
