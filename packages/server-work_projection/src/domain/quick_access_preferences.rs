//! Quick-access preference validation. Pinned IDs are selectors only; the
//! caller must supply the current authorized catalogue before saving new pins.
use erpx_server_shared::public::BusinessError;

pub const MAX_PINS: usize = 6;
pub const MAX_ID_UNITS: usize = 200;
pub const MAX_REQUEST_KEY_UNITS: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickAccessPreference {
    pub pinned_ids: Option<Vec<String>>,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveQuickAccessInput {
    pub user_id: String,
    pub surface: String,
    pub context_key: String,
    pub pinned_ids: Option<Vec<String>>,
    pub expected_version: i64,
    pub available_ids: Vec<String>,
    pub request_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedQuickAccess {
    pub pinned_ids: Option<Vec<String>>,
    pub expected_version: i64,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn id_valid(value: &str) -> bool {
    !value.is_empty() && value.encode_utf16().count() <= MAX_ID_UNITS
}

pub fn validate(input: &SaveQuickAccessInput) -> Result<ValidatedQuickAccess, BusinessError> {
    if input.user_id.is_empty() || input.surface.is_empty() || input.context_key.is_empty() {
        return Err(invalid("INVALID_SCOPE", "The shortcut scope is invalid."));
    }
    if input.request_key.is_empty()
        || input.request_key.encode_utf16().count() > MAX_REQUEST_KEY_UNITS
    {
        return Err(invalid(
            "IDEMPOTENCY_KEY_REQUIRED",
            "Provide a command key to save shortcuts.",
        ));
    }
    if input.expected_version < 0 {
        return Err(invalid(
            "INVALID_SHORTCUTS",
            "Use a nonnegative shortcut version.",
        ));
    }
    if let Some(pins) = &input.pinned_ids {
        if pins.len() > MAX_PINS || pins.iter().any(|id| !id_valid(id)) || {
            let mut seen = std::collections::HashSet::with_capacity(pins.len());
            pins.iter().any(|id| !seen.insert(id.as_str()))
        } {
            return Err(invalid(
                "INVALID_SHORTCUTS",
                "Choose up to six unique shortcuts.",
            ));
        }
    }
    Ok(ValidatedQuickAccess {
        pinned_ids: input.pinned_ids.clone(),
        expected_version: input.expected_version,
    })
}

pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "VERSION_CONFLICT",
        "Your shortcuts changed on another device. Review the latest order before saving.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This save key belongs to a different shortcut change.",
    )
}

pub fn shortcut_unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        "SHORTCUT_UNAVAILABLE",
        "A selected shortcut is no longer available.",
    )
}
