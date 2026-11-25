//! Pure domain types for the native Finance overview company-counts read.
//!
//! The axum owner owns the SQL and the projection. This module carries no SQL:
//! it validates the bounded Company identity, pins the response shape and
//! enforces that every count is a non-negative integer.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

/// The seven company-scoped counts that back the Finance workbench and the
/// workspace assistant. Field order is part of the wire contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OverviewCounts {
    pub journals: i64,
    pub posting: i64,
    pub invoices: i64,
    pub payments: i64,
    pub receivable: i64,
    pub payable: i64,
    pub reconcile: i64,
}

impl OverviewCounts {
    /// Fail closed on any impossible (negative) count rather than emitting a
    /// malformed projection. `count(*)` can never be negative.
    pub fn validated(self) -> Result<Self, BusinessError> {
        let counts = [
            self.journals,
            self.posting,
            self.invoices,
            self.payments,
            self.receivable,
            self.payable,
            self.reconcile,
        ];
        if counts.iter().any(|count| *count < 0) {
            return Err(unavailable());
        }
        Ok(self)
    }
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_NATIVE_LANE_2_REQUEST_INVALID",
        "Use a bounded Company and registered Holding target.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_NATIVE_LANE_2_UNAVAILABLE",
        "The native Finance overview read is unavailable.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_NATIVE_LANE_2_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

/// Company ids are opaque but bounded identifiers used as a SQL bind only.
pub fn valid_company_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounded_company_and_slug() {
        assert!(valid_company_id("acme:company-01"));
        assert!(valid_company_id("c.1_2"));
        assert!(!valid_company_id(""));
        assert!(!valid_company_id("bad/id?"));
        assert!(!valid_company_id(&"x".repeat(201)));
        assert!(valid_slug("acme-holding_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
        assert!(!valid_slug(&"a".repeat(129)));
    }

    #[test]
    fn rejects_negative_counts() {
        let good = OverviewCounts {
            journals: 1,
            posting: 0,
            invoices: 2,
            payments: 3,
            receivable: 4,
            payable: 5,
            reconcile: 6,
        };
        assert_eq!(good.validated(), Ok(good));
        let bad = OverviewCounts {
            reconcile: -1,
            ..good
        };
        assert_eq!(bad.validated(), Err(unavailable()));
    }
}
