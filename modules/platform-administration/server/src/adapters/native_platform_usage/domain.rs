//! Pure types, validation and invariants for the native Platform usage read.
//!
//! The TypeScript authority is the `recordsExecutiveUsageSql` projection owned
//! by this crate (`adapters/records-ai-usage-projection.ts`): one bounded
//! `executive_generation_jobs` count for a Company and a UTC month. No SQL and
//! no transport live in this module.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;

/// Matches the `identifier` bound of the retained Platform Admin transport.
pub const MAX_COMPANY: usize = 200;
/// `YYYY-MM`.
pub const MAX_MONTH: usize = 7;
pub const MAX_USAGE: i64 = 1_000_000_000;

pub fn valid_company(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_COMPANY
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// UTC month exactly as `to_char(...,'YYYY-MM')` renders it.
pub fn valid_month(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != MAX_MONTH
        || bytes[4] != b'-'
        || !bytes[..4]
            .iter()
            .chain(&bytes[5..])
            .all(|byte| byte.is_ascii_digit())
    {
        return false;
    }
    let month = (bytes[5] - b'0') * 10 + (bytes[6] - b'0');
    bytes[0] != b'0' && (1..=12).contains(&month)
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct ExecutiveUsage {
    pub company_id: String,
    pub month: String,
    pub generation_jobs: i64,
}

impl ExecutiveUsage {
    pub fn new(company_id: String, month: String, generation_jobs: i64) -> Result<Self, BusinessError> {
        if !valid_company(&company_id)
            || !valid_month(&month)
            || !(0..=MAX_USAGE).contains(&generation_jobs)
        {
            return Err(source_invalid());
        }
        Ok(Self {
            company_id,
            month,
            generation_jobs,
        })
    }
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLATFORM_USAGE_REQUEST_INVALID",
        "Use one bounded Platform usage read.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLATFORM_USAGE_SOURCE_INVALID",
        "The current Platform usage source is invalid.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_are_bounded() {
        assert!(valid_company("company-001"));
        assert!(valid_company("h001:company"));
        assert!(!valid_company(""));
        assert!(!valid_company("x'. DROP"));
        assert!(valid_month("2026-01"));
        assert!(valid_month("2026-12"));
        assert!(!valid_month("2026-13"));
        assert!(!valid_month("2026-00"));
        assert!(!valid_month("2026-1"));
        assert!(!valid_month("0000-01"));
        assert!(!valid_month("2026/01"));
    }

    #[test]
    fn usage_rejects_unbounded_source_values() {
        assert!(ExecutiveUsage::new("company-001".into(), "2026-06".into(), 3).is_ok());
        assert!(ExecutiveUsage::new("company-001".into(), "2026-13".into(), 3).is_err());
        assert!(ExecutiveUsage::new("company-001".into(), "2026-06".into(), -1).is_err());
        assert!(ExecutiveUsage::new("company-001".into(), "2026-06".into(), MAX_USAGE + 1).is_err());
    }
}
