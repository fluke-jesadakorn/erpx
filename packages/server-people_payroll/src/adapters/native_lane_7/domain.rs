//! Pure response shape and validation for the lane-7 legacy people clock.
//!
//! The owner owns the projection: one Company-scoped deadline derived from its
//! own access-cutoff, lifecycle-case and applied-package sources. No SQL and no
//! transport live in this module.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};

/// Wire discriminator pinned by the native client and the contract test.
pub const READ_KIND: &str = "people_payroll_lane_7_legacy_people_transition";
/// Owner-local route mounted by the lead alongside the other holding routers.
pub const ROUTE: &str = "/api/v1/holdings/{holding_slug}/people-payroll/legacy-people-clock";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyPeopleTransition {
    pub deadline: Option<i64>,
}

pub fn invalid_company() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE7_COMPANY_INVALID",
        "Use one bounded Company scope.",
    )
}

pub fn invalid_observed_at() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE7_OBSERVED_AT_INVALID",
        "Use a non-negative epoch-millisecond clock.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE7_UNAVAILABLE",
        "The native People Payroll clock service is unavailable.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE7_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn validate_company(company: &str) -> Result<(), BusinessError> {
    if valid_id(company) {
        Ok(())
    } else {
        Err(invalid_company())
    }
}

pub fn parse_observed_at(raw: &str) -> Result<i64, BusinessError> {
    if raw.is_empty() || raw.len() > 19 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid_observed_at());
    }
    match raw.parse::<i64>() {
        Ok(value) if value >= 0 => Ok(value),
        _ => Err(invalid_observed_at()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_company_scope() {
        assert!(validate_company("company-1").is_ok());
        assert!(validate_company("acme:company:01").is_ok());
        assert!(validate_company("").is_err());
        assert!(validate_company("../other").is_err());
        assert!(validate_company(&"x".repeat(201)).is_err());
    }

    #[test]
    fn parses_observed_clock() {
        assert_eq!(parse_observed_at("0"), Ok(0));
        assert_eq!(parse_observed_at("1764010200000"), Ok(1_764_010_200_000));
        assert!(parse_observed_at("-1").is_err());
        assert!(parse_observed_at("12ms").is_err());
        assert!(parse_observed_at("99999999999999999999").is_err());
    }

    #[test]
    fn pins_response_shape() {
        let value = LegacyPeopleTransition { deadline: Some(42) };
        assert_eq!(
            serde_json::to_string(&value).expect("serialize"),
            r#"{"deadline":42}"#
        );
        let none = LegacyPeopleTransition { deadline: None };
        assert_eq!(
            serde_json::to_string(&none).expect("serialize"),
            r#"{"deadline":null}"#
        );
    }
}
