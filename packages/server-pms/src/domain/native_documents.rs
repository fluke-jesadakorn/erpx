//! Pure response model and bounds for the native PMS document entrypoint read.
//!
//! This replaces the TypeScript `pmsDocumentQueries` plan/execute pair
//! (`document(id)` and `relatedPostings(property, document)`), which leaves
//! TypeScript preparing and executing the owner's SELECT. No SQL, no transport
//! and no I/O live here.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};

/// The complete `pms_documents` row returned by the retired `document(id)` read.
/// Every column the TypeScript operation exposed is present, with `finance_id`
/// optional exactly as in the table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentRow {
    pub id: String,
    pub property_id: String,
    pub guest_account_id: String,
    pub kind: String,
    pub finance_id: Option<String>,
    pub snapshot_json: String,
    pub created_at: i64,
}

/// One row of the retired `relatedPostings(property, document)` projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelatedPosting {
    pub journal_id: String,
}

/// Holding slugs are exact lowercase registrations, never caller SQL.
pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

/// Document and Property identifiers are bounded and cannot carry a URI or SQL
/// control character.
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
        "PMS_NATIVE_DOCUMENTS_REQUEST_INVALID",
        "Use one bounded document read.",
    )
}

pub fn invalid_configuration() -> BusinessError {
    BusinessError::invalid(
        "PMS_NATIVE_DOCUMENTS_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_NATIVE_DOCUMENTS_UNAVAILABLE",
        "The native PMS document read is unavailable.",
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
        assert!(valid_id("doc-1"));
        assert!(valid_id("property:1"));
        assert!(!valid_id("bad company id!"));
        assert!(!valid_id(&"x".repeat(201)));
    }
}
