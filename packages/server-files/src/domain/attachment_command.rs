//! Pure Files-owned attachment command policy.
//!
//! These rules are transport- and persistence-independent. Owner/record
//! authorization remains a composition port; this module only validates the
//! request shape and the Files-owned link lifecycle.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::Digest;

pub const MIN_WITHDRAWAL_REASON_UNITS: usize = 3;
pub const MAX_WITHDRAWAL_REASON_UNITS: usize = 1000;
pub const MIN_COMMAND_KEY_BYTES: usize = 8;
pub const MAX_COMMAND_KEY_BYTES: usize = 180;

/// Owners that already expose a native intake/command surface. The generic
/// Files command must never become a second writer for those lifecycles.
pub fn has_native_owner_command(target_type: &str) -> bool {
    matches!(
        target_type,
        "field_service_visit" | "project" | "finance_journal" | "finance_payment"
    )
}

/// Read authority category of a non-native-command attachment target. The
/// composition root maps each category to the owner's current read port; this
/// is only the stable Files-side classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WithdrawOwnerAccess {
    /// The generic Files descriptor read gate (`records.read` plus the current
    /// Company/branch/department selection).
    RecordsRead,
    /// Branding read: `records.read`, the `brand` module and the exact
    /// holding/company/branch target identity.
    Brand,
    /// Legal read: `legal.read` and the current Files scope. Legal item
    /// membership/classification remains owned by the Legal record policy and
    /// is not duplicated by Files.
    Legal,
    /// Messaging read: `records.read`, the `collaboration` module and current
    /// conversation membership.
    Messaging,
    /// Customer portal read: the current staff customer-file permission. The
    /// external customer session authority remains owned by Customer Authority.
    Portal,
}

/// Classify a generic target for the owner read check. Unknown targets stay on
/// the generic Files descriptor gate rather than becoming owner-native writes.
pub fn withdraw_owner_access(target_type: &str) -> WithdrawOwnerAccess {
    match target_type {
        "holding_brand" | "branch_brand" | "company_storefront_brand" => WithdrawOwnerAccess::Brand,
        "legal_item" | "legal_external" => WithdrawOwnerAccess::Legal,
        "messaging_room" => WithdrawOwnerAccess::Messaging,
        "customer_access_binding" => WithdrawOwnerAccess::Portal,
        _ => WithdrawOwnerAccess::RecordsRead,
    }
}

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

/// Match the TypeScript writer's `reason.trim()` and UTF-16 length policy so a
/// native retry is byte-for-byte the same audit payload.
pub fn normalize_withdrawal_reason(raw: &str) -> Result<String, BusinessError> {
    let text = raw.trim_matches(is_js_whitespace);
    let units = text.encode_utf16().count();
    if !(MIN_WITHDRAWAL_REASON_UNITS..=MAX_WITHDRAWAL_REASON_UNITS).contains(&units) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "WITHDRAWAL_REASON_REQUIRED",
            "Explain why this file is being withdrawn.",
        ));
    }
    Ok(text.to_owned())
}

pub fn valid_command_key(key: &str) -> bool {
    (MIN_COMMAND_KEY_BYTES..=MAX_COMMAND_KEY_BYTES).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
}

/// Stable audit/replay identity, identical to the retained TypeScript writer.
pub fn withdrawal_event_id(company_id: &str, link_id: &str, key: &str) -> String {
    let hash = hex::encode(sha2::Sha256::digest(
        format!("{company_id}\0{link_id}\0{key}").as_bytes(),
    ));
    format!("attachment-withdraw-{}", &hash[..32])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WithdrawalDecision {
    Apply,
    Replay,
}

/// Files-owned link lifecycle. The caller proves current owner/holding/company
/// authority separately; draft links stay private to their creator.
pub fn withdrawal_decision(
    state: &str,
    creator: &str,
    withdrawn_by: Option<&str>,
    previous_reason: Option<&str>,
    actor: &str,
    reason: &str,
) -> Result<WithdrawalDecision, BusinessError> {
    if state == "bound" {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "BOUND_ATTACHMENT_IMMUTABLE",
            "Bound evidence cannot be withdrawn. Add a correction instead.",
        ));
    }
    if creator != actor {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ATTACHMENT_WITHDRAW_FORBIDDEN",
            "Only the person who staged this file may withdraw it.",
        ));
    }
    match state {
        "draft" => Ok(WithdrawalDecision::Apply),
        "withdrawn" if withdrawn_by == Some(actor) && previous_reason == Some(reason) => {
            Ok(WithdrawalDecision::Replay)
        }
        "withdrawn" => Err(BusinessError::new(
            FailureKind::Conflict,
            "IDEMPOTENCY_CONFLICT",
            "This file was withdrawn with a different command.",
        )),
        _ => Err(BusinessError::new(
            FailureKind::Conflict,
            "ATTACHMENT_LINK_LOCKED",
            "This file link is closed.",
        )),
    }
}
