//! Shared Ecommerce command facts.
//!
//! Receipts, audit and outbox rows use the existing Holding workflow tables.
//! The request identity is owner-canonical; a command key must not cross the
//! retained TypeScript/native boundary because the retained fingerprint has a
//! different JSON key order.
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;
use serde_json::Value;

pub const CONFIGURATION_LIMIT: usize = 16 * 1024;
pub const RECEIPT_LIMIT: usize = 4 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandReceipt {
    pub request_hash: String,
    pub result_json: String,
    pub operation: Option<String>,
    pub actor_id: Option<String>,
    pub record_id: Option<String>,
    pub authority_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EcommerceReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl EcommerceReceipt {
    pub fn to_wire(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_SOURCE_INVALID",
        "The current Ecommerce source is invalid.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_REQUEST_INVALID",
        "Use one bounded Ecommerce request.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_SOURCE_INVALID",
        "The recorded Ecommerce change could not be read back.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ECOMMERCE_RECORD_NOT_FOUND",
        "Ecommerce record not found in this workspace.",
    )
}

pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ECOMMERCE_SOURCE_CHANGED",
        "Ecommerce record changed. Reload and try again.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
}

pub fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

/// ECMAScript `String.prototype.trim` whitespace: White_Space plus
/// Line_Terminator. Rust's `str::trim` additionally strips U+0085 (White_Space
/// in Unicode but not in JavaScript `trim`) and keeps U+FEFF, so the exact
/// set is spelled out to match the retained Zod `trim()` boundary.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|c| {
        matches!(c,
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
                | '\u{feff}')
    })
}

/// Zod v4 string limits count Unicode scalar values (code points).
/// Command input (channel `name`, review `reason`) uses [`valid_cp_text`].
pub(crate) fn chars_len(value: &str) -> usize {
    value.chars().count()
}

/// Code-point text check for Zod command paths only.
pub fn valid_cp_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && chars_len(value) <= limit && !value.chars().any(char::is_control)
}

/// Backwards-compatible alias for [`valid_cp_text`], used by the Zod
/// channel/review command paths.
pub fn cp_text(value: &str, limit: usize) -> bool {
    valid_cp_text(value, limit)
}

/// UTF-16 code-unit length, retained for transport/source rows (catalog
/// name/reviewer/title and receipt `status`). Do not use for Zod command
/// input; command paths use [`chars_len`].
pub(crate) fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

pub fn valid_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && utf16_len(value) <= limit && !value.chars().any(char::is_control)
}

pub fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

pub fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn valid_configuration(value: &str) -> bool {
    if value.len() > CONFIGURATION_LIMIT {
        return false;
    }
    serde_json::from_str::<Value>(value).is_ok_and(|parsed| parsed.is_object())
}

/// Structural receipt validation. The owner-written result must be exactly one
/// `{id,status,version}` object; status and version are validated by the
/// application against the command transition.
pub fn parse_receipt_result(value: &str) -> Result<EcommerceReceipt, BusinessError> {
    if value.len() > RECEIPT_LIMIT {
        return Err(source_invalid());
    }
    let parsed: Value = serde_json::from_str(value).map_err(|_| source_invalid())?;
    let object = parsed.as_object().ok_or_else(source_invalid)?;
    if object.len() != 3
        || !object.contains_key("id")
        || !object.contains_key("status")
        || !object.contains_key("version")
    {
        return Err(source_invalid());
    }
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let version = object
        .get("version")
        .and_then(Value::as_i64)
        .ok_or_else(source_invalid)?;
    if !valid_identifier(id) || !valid_text(status, 40) || version < 1 {
        return Err(source_invalid());
    }
    Ok(EcommerceReceipt {
        id: id.to_owned(),
        status: status.to_owned(),
        version,
    })
}

#[derive(Serialize)]
struct Identity<'a, T> {
    user: &'a str,
    assignment: &'a str,
    company: &'a str,
    branch: Option<&'a str>,
    department: Option<&'a str>,
    operation: &'a str,
    input: &'a T,
}

/// Canonical owner command identity for the PostgreSQL `sha256` digest.
pub fn command_identity<T: Serialize>(
    actor: &super::actor::EcommerceActor,
    operation: &str,
    input: &T,
) -> String {
    serde_json::to_string(&Identity {
        user: actor.user_id.as_str(),
        assignment: actor.assignment_id.as_str(),
        company: actor.company_id.as_str(),
        branch: actor.branch_id.as_ref().map(|value| value.as_str()),
        department: actor.department_id.as_ref().map(|value| value.as_str()),
        operation,
        input,
    })
    .expect("the Ecommerce command identity always serializes")
}
