//! Pure response types and invariants for the payroll payment-batch reads.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};

/// The owner list read has no SQL limit; bound the native reply so an unbounded
/// result can never become an unbounded TS payload.
pub const MAX_BATCHES: usize = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentBatch {
    pub source_id: String,
    pub id: String,
    pub status: String,
    pub created_by: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentBatchDetail {
    pub id: String,
    pub status: String,
    pub created_by: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE9_FLOW_REQUEST_INVALID",
        "Use one bounded Company and payroll source identity.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE9_FLOW_READ_UNAVAILABLE",
        "The native People Payroll payment-batch service is unavailable.",
    )
}

/// Bounded identity validation; Company and source travel as values, never SQL.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}
