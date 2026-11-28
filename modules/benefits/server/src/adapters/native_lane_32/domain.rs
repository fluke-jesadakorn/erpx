//! Pure types, validation and invariants for the native lane-32 Benefits
//! released payroll-input read. No SQL and no I/O live here.

use erpx_platform_shared_server::public::BusinessError;
use serde::Serialize;

/// One registered private native owner target. `host` and `holding_slug` are
/// the exact pair the composition mounts; `origin` is the registered browser
/// origin and is checked when a caller supplies it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

/// Bounds for a released payroll-input request. The four legacy positional
/// binds keep their original order; `company_id` is added as an owner-safe
/// scope bind and is required by the native route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollInputsRequest {
    pub company_id: String,
    pub engagement_id: String,
    pub currency: String,
    pub period_end: String,
    pub period_start: String,
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

impl PayrollInputsRequest {
    /// Fail-closed request validation. The currency is the ISO-4217-shaped
    /// uppercase code used by the original money columns; periods are UTC days.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company_id)
            || !valid_id(&self.engagement_id)
            || self.currency.len() != 3
            || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || !valid_date(&self.period_end)
            || !valid_date(&self.period_start)
            || self.period_start > self.period_end
        {
            return Err(invalid());
        }
        Ok(())
    }
}

pub(crate) fn invalid() -> BusinessError {
    BusinessError::invalid(
        "BENEFIT_PAYROLL_INPUT_READ_INVALID",
        "Use one bounded Benefits payroll-input read.",
    )
}

pub(crate) fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Unavailable,
        "BENEFIT_PAYROLL_INPUT_READ_UNAVAILABLE",
        "The native Benefits payroll-input reader is unavailable.",
    )
}

/// The exact released-input projection the People payroll owner consumes.
/// Money stays in minor units; no currency math is invented in Rust.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PayrollInputFact {
    pub id: String,
    pub plan_name: String,
    pub employee_deduction_minor: i64,
    pub employer_cost_minor: i64,
    pub source_election_version: i64,
}

/// Response shape pinned by the TS client and the contract test.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PayrollInputsResponse {
    pub inputs: Vec<PayrollInputFact>,
}

impl From<Vec<PayrollInputFact>> for PayrollInputsResponse {
    fn from(inputs: Vec<PayrollInputFact>) -> Self {
        Self { inputs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> PayrollInputsRequest {
        PayrollInputsRequest {
            company_id: "company-1".into(),
            engagement_id: "engagement-1".into(),
            currency: "THB".into(),
            period_end: "2026-12-31".into(),
            period_start: "2026-01-01".into(),
        }
    }

    #[test]
    fn accepts_one_bounded_request() {
        assert!(request().validate().is_ok());
    }

    #[test]
    fn rejects_unsafe_or_malformed_requests() {
        let mut value = request();
        value.company_id = "bad company".into();
        assert_eq!(value.validate().unwrap_err().code, "BENEFIT_PAYROLL_INPUT_READ_INVALID");
        let mut value = request();
        value.engagement_id = String::new();
        assert!(value.validate().is_err());
        let mut value = request();
        value.currency = "thb".into();
        assert!(value.validate().is_err());
        let mut value = request();
        value.period_end = "2026-1-1".into();
        assert!(value.validate().is_err());
        let mut value = request();
        value.period_start = "2027-01-01".into();
        assert!(value.validate().is_err());
    }

    #[test]
    fn response_shape_is_stable() {
        let response = PayrollInputsResponse::from(vec![PayrollInputFact {
            id: "input-1".into(),
            plan_name: "Family plan".into(),
            employee_deduction_minor: 100,
            employer_cost_minor: 200,
            source_election_version: 3,
        }]);
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"inputs":[{"id":"input-1","plan_name":"Family plan","employee_deduction_minor":100,"employer_cost_minor":200,"source_election_version":3}]}"#
        );
    }
}
