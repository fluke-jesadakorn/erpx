//! Pure response types and invariants for the native R3 country-packs read.
//! No `sqlx`, no `axum`, no I/O: the domain owns the DTO and value bounds;
//! PostgreSQL execution lives in `postgres`.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};

/// The retained `listCountryPacks` read returns the raw projection rows with
/// snake_case column names and no further shaping. Field order is pinned by
/// the HTTP payload test so the JSON body is byte-identical to the TS route.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct CountryPackRow {
    pub id: String,
    pub country_code: String,
    /// `version` is a text column in `hr_country_packs` (see createCountryPack).
    pub version: String,
    pub status: String,
    pub effective_from: Option<String>,
    pub effective_to: Option<String>,
    pub adapter_id: Option<String>,
    pub source_digest: Option<String>,
    pub record_version: i64,
    pub export_formats_json: serde_json::Value,
    pub validated_by: Option<String>,
    pub updated_at: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_COUNTRY_PACKS_REQUEST_INVALID",
        "Use one bounded Company identity.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_COUNTRY_PACKS_READ_UNAVAILABLE",
        "The native People Payroll country-pack service is unavailable.",
    )
}

/// Bounded Company identity validation. The value travels only as a bind
/// parameter; this check rejects anything that could be mistaken for SQL.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Holding slugs are lowercase path segments only.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn company_identity_is_bounded() {
        assert!(valid_id("acme-co"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x'; DROP TABLE hr_country_packs --"));
    }

    #[test]
    fn holding_slug_is_bounded() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
    }
}
