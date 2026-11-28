//! Pure types, validation and invariants for lane-32 Workflow failed-metrics
//! reads. No SQL and no I/O live here.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;

/// One registered private native owner target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

/// Strictly typed response shape: a non-negative dead-letter count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct FailedMetrics {
    pub count: i64,
}

impl FailedMetrics {
    pub fn new(count: i64) -> Result<Self, BusinessError> {
        if count < 0 {
            return Err(unavailable());
        }
        Ok(Self { count })
    }
}

pub(crate) fn invalid() -> BusinessError {
    BusinessError::invalid(
        "WORKFLOW_FAILED_METRICS_READ_INVALID",
        "Use one bounded Workflow failed-metrics read.",
    )
}

pub(crate) fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORKFLOW_FAILED_METRICS_READ_UNAVAILABLE",
        "The native Workflow failed-metrics reader is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_non_negative_count() {
        assert_eq!(FailedMetrics::new(0).unwrap().count, 0);
        assert_eq!(FailedMetrics::new(7).unwrap().count, 7);
    }

    #[test]
    fn rejects_a_negative_count() {
        assert_eq!(
            FailedMetrics::new(-1).unwrap_err().code,
            "WORKFLOW_FAILED_METRICS_READ_UNAVAILABLE"
        );
    }

    #[test]
    fn pins_the_response_shape() {
        assert_eq!(
            serde_json::to_string(&FailedMetrics::new(7).unwrap()).unwrap(),
            r#"{"count":7}"#
        );
    }
}
