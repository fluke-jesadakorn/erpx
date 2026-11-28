//! Pure types for the native Benefits settlement event-history read.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;

/// One registered private native owner target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementEventsRequest {
    pub company_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SettlementEventFact {
    pub kind: String,
    pub image: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SettlementEventsResponse {
    pub events: Vec<SettlementEventFact>,
}

impl From<Vec<SettlementEventFact>> for SettlementEventsResponse {
    fn from(events: Vec<SettlementEventFact>) -> Self {
        Self { events }
    }
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

impl SettlementEventsRequest {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if valid_id(&self.company_id) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}

pub(crate) fn invalid() -> BusinessError {
    BusinessError::invalid(
        "BENEFIT_SETTLEMENT_EVENTS_READ_INVALID",
        "Use one bounded Benefits settlement event read.",
    )
}

pub(crate) fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFIT_SETTLEMENT_EVENTS_READ_UNAVAILABLE",
        "The native Benefits settlement event reader is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_only_bounded_company_ids() {
        assert!(SettlementEventsRequest { company_id: "company-1".into() }.validate().is_ok());
        for id in ["", "bad company", &"x".repeat(201)] {
            assert!(SettlementEventsRequest { company_id: id.into() }.validate().is_err());
        }
    }
}
