//! Pure response model and validation for the native PMS property-readiness
//! read. This is the owner of the response shape; no SQL and no transport here.

use erpx_server_shared::public::{BusinessError, RecordId};

/// Bounded readiness facts for one Property. Every value is a boolean witness,
/// so the payload has no ambiguity between "absent" and "false".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertyReadinessFacts {
    pub has_room_types: bool,
    pub has_active_rooms: bool,
    pub has_active_rates: bool,
    pub has_stays: bool,
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.' || byte == b':')
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "PMS_NATIVE_LANE_20_REQUEST_INVALID",
        "Use one bounded Property readiness read.",
    )
}

pub fn invalid_configuration() -> BusinessError {
    BusinessError::invalid(
        "PMS_NATIVE_LANE_20_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn configuration_unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "PMS_NATIVE_LANE_20_UNAVAILABLE",
        "The native PMS property readiness service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_bounded() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("Acme"));
        assert!(valid_id("property-1"));
        assert!(!valid_id("bad company id!"));
        assert!(!valid_id(&"x".repeat(201)));
    }
}
