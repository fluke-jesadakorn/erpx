//! Pure response model and validation for the native PMS property-readiness
//! read. This is the owner of the response shape; no SQL and no transport here.

use erpx_platform_shared_server::public::{BusinessError, RecordId};

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

/// Room-count facts for one Property. All values are bounded non-negative
/// counts, so the wire shape has no ambiguous missing/zero distinction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomSummaryFacts {
    pub total: i32,
    pub active: i32,
    pub dirty: i32,
    pub clean: i32,
    pub inspected: i32,
    pub dnd: i32,
    pub occupied: i32,
}

/// Optional room-list filters are bounded but may be empty.
pub fn valid_filter(value: &str) -> bool {
    value.len() <= 200
}

/// A calendar record after the retired `decodePmsRecord` step. The owner
/// parses the stored JSON itself, so the wire already has the final shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarRecord {
    pub id: String,
    pub kind: String,
    pub data: serde_json::Value,
    pub version: i32,
    pub updated_at: i64,
}

/// One cutover-import batch row. The projection is byte-for-byte the column
/// list retired from `adapters/pms/import.ts`; the report payload stays its
/// original JSON text so the owner application keeps its derived field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportBatchRow {
    pub id: String,
    pub cutover_date: String,
    pub state: String,
    pub version: i32,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub opening_journal_id: Option<String>,
    pub report_json: String,
    pub created_at: i64,
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
        erpx_platform_shared_server::public::FailureKind::Unavailable,
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
