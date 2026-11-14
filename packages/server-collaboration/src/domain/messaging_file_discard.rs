//! Unsent Messaging file discard policy.
//!
//! Only the original uploader may discard their still-unbound message file
//! evidence. Conversation membership, the Files link lifecycle and the Files
//! withdrawal audit stay with their owners; this module only sequences the
//! Messaging-owned checks and never grants file or room authority.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const MIN_DISCARD_REASON_UNITS: usize = 3;
pub const MAX_DISCARD_REASON_UNITS: usize = 1000;
pub const MAX_DISCARD_ID_BYTES: usize = 180;

/// JavaScript `String.prototype.trim` whitespace and line terminators.
fn is_js_whitespace(value: char) -> bool {
    matches!(
        value,
        '\u{0009}'..='\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// Match the retained TypeScript `reason.trim()` and UTF-16 length policy so a
/// native retry stores the same audit payload for the same input.
pub fn normalize_discard_reason(raw: &str) -> Result<String, BusinessError> {
    let text = raw.trim_matches(is_js_whitespace);
    let units = text.encode_utf16().count();
    if !(MIN_DISCARD_REASON_UNITS..=MAX_DISCARD_REASON_UNITS).contains(&units) {
        return Err(BusinessError::invalid(
            "MESSAGE_FILE_DISCARD_REASON_REQUIRED",
            "Explain why this unsent attachment is being discarded.",
        ));
    }
    Ok(text.to_owned())
}

pub fn valid_file_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DISCARD_ID_BYTES
        && !value.chars().any(char::is_control)
}

pub fn file_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MESSAGE_FILE_NOT_FOUND",
        "The original unsent attachment is unavailable.",
    )
}

pub fn room_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MESSAGING_ROOM_FORBIDDEN",
        "Choose a conversation you belong to.",
    )
}

pub fn already_bound() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MESSAGE_FILE_ALREADY_BOUND",
        "Only your own unsent message attachment can be discarded.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MESSAGING_FILE_SOURCE_CHANGED",
        "The original unsent attachment moved to another conversation.",
    )
}

pub fn receipt_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COLLABORATION_RECEIPT_INVALID",
        "The stored receipt is unavailable.",
    )
}

/// Exact Messaging-owned fields of one stored message file. The Files link is
/// only an identifier; its state and audit remain Files-owned facts.
pub struct DiscardFileFacts<'a> {
    pub created_by: &'a str,
    pub message_id: Option<&'a str>,
    pub attachment_link_id: Option<&'a str>,
    pub actor: &'a str,
}

/// The discard itself is applied by the Files owner port. Messaging refuses a
/// missing link, a file already attached to a message, and another user's
/// staged file before Files sees the command.
pub fn require_discardable(facts: DiscardFileFacts<'_>) -> Result<&'_ str, BusinessError> {
    let link = facts.attachment_link_id.ok_or_else(already_bound)?;
    if facts.message_id.is_some() || facts.created_by != facts.actor || link.is_empty() {
        return Err(already_bound());
    }
    Ok(link)
}
