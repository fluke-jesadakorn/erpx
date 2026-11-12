//! Native Quality alert acknowledgement command policy.
//!
//! Transport-independent input admission and receipt facts. Current Account,
//! IAM recipient and Commercial purchase admission remain composition concerns;
//! the owner effect locks the exact alert row in the caller's serializable unit.
use crate::domain::quality::{MAX_SAFE_INTEGER, validate_text};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};

pub const QUALITY_ALERT_ACKNOWLEDGE: &str = "quality.alert.acknowledge";

/// Retained `quality.alert.acknowledge` input. The alert version is the exact
/// source version the recipient reviewed; the reason is trimmed by admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityAlertAcknowledgeCommand {
    pub key: CommandKey,
    pub alert_id: RecordId,
    pub expected_version: i64,
    pub reason: String,
}

impl QualityAlertAcknowledgeCommand {
    pub fn operation(&self) -> &'static str {
        QUALITY_ALERT_ACKNOWLEDGE
    }
}

/// Exact retained receipt `{id, version: expectedVersion+1, acknowledgedBy}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityAlertAcknowledgementReceipt {
    pub id: String,
    pub version: i64,
    pub acknowledged_by: String,
}

pub fn normalize_quality_alert_acknowledge(
    command: QualityAlertAcknowledgeCommand,
) -> Result<QualityAlertAcknowledgeCommand, BusinessError> {
    if !(1..=MAX_SAFE_INTEGER).contains(&command.expected_version) {
        return Err(BusinessError::invalid(
            "VERSION_REQUIRED",
            "Refresh the Quality escalation before acknowledging.",
        ));
    }
    Ok(QualityAlertAcknowledgeCommand {
        reason: validate_text(
            &command.reason,
            3,
            2000,
            "QUALITY_ALERT_REASON",
            "Enter an acknowledgement reason between 3 and 2000 characters.",
        )?,
        ..command
    })
}
