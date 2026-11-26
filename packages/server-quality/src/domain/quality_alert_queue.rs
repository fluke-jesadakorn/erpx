//! Pure facts for the Quality escalation queue read.
//!
//! These values mirror the retained `QualityAlert` row byte-for-byte in field
//! names and types. SQL, HTTP and cross-owner purchase admission live outside
//! this module.
use erpx_server_shared::public::{BusinessError, FailureKind};

/// The retained workspace queue has no pagination cursor; the owner still
/// refuses an unbounded source snapshot.
pub const MAX_ALERTS: usize = 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertPhase {
    DueSoon,
    Overdue,
}

impl AlertPhase {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "due_soon" => Some(Self::DueSoon),
            "overdue" => Some(Self::Overdue),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::DueSoon => "due_soon",
            Self::Overdue => "overdue",
        }
    }

    /// The retained `ORDER BY ... CASE WHEN phase='overdue' THEN 0 ELSE 1`.
    pub fn sort_rank(self) -> u8 {
        match self {
            Self::Overdue => 0,
            Self::DueSoon => 1,
        }
    }
}

/// One escalation row exactly as the Quality workspace presenter consumes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityAlertRow {
    pub id: String,
    pub hold_id: String,
    pub phase: AlertPhase,
    pub source_version: i64,
    pub hold_version: i64,
    pub due_date: String,
    pub detected_at: i64,
    pub version: i64,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<i64>,
    pub reason: Option<String>,
    pub company_id: String,
    pub branch_id: String,
    pub status: String,
    pub product_name: String,
    pub warehouse_name: String,
    pub can_ack: i64,
    pub recipients: i64,
}

impl QualityAlertRow {
    /// A hold in a terminal state or an already acknowledged alert is complete;
    /// it never offers the acknowledgement action.
    pub fn is_complete(&self) -> bool {
        self.acknowledged_by.is_some()
            || matches!(
                self.status.as_str(),
                "released" | "cancelled" | "disposed"
            )
    }
}

/// The owner-side read guard. Row counts, version bounds and the retained
/// ordering are facts the adapter cannot relax.
pub fn assert_bounded(rows: &[QualityAlertRow]) -> Result<(), BusinessError> {
    if rows.len() > MAX_ALERTS {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "QUALITY_ALERT_QUEUE_LIMIT",
            "The Quality escalation queue exceeds its bounded read.",
        ));
    }
    if rows.iter().any(|row| {
        row.source_version < 1
            || row.hold_version < 1
            || row.version < 1
            || row.can_ack < 0
            || row.can_ack > 1
            || row.recipients < 0
    }) {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "QUALITY_ALERT_QUEUE_INVALID",
            "The Quality escalation source is invalid.",
        ));
    }
    if rows.windows(2).any(|pair| {
        let (left, right) = (&pair[0], &pair[1]);
        (
            left.due_date.as_str(),
            left.phase.sort_rank(),
            left.id.as_str(),
        ) > (
            right.due_date.as_str(),
            right.phase.sort_rank(),
            right.id.as_str(),
        )
    }) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "QUALITY_ALERT_QUEUE_CHANGED",
            "The Quality escalation order changed.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(due: &str, phase: AlertPhase, id: &str) -> QualityAlertRow {
        QualityAlertRow {
            id: id.to_owned(),
            hold_id: format!("hold-{id}"),
            phase,
            source_version: 1,
            hold_version: 1,
            due_date: due.to_owned(),
            detected_at: 1,
            version: 1,
            acknowledged_by: None,
            acknowledged_at: None,
            reason: None,
            company_id: "company".to_owned(),
            branch_id: "branch".to_owned(),
            status: "held".to_owned(),
            product_name: "Packaged goods".to_owned(),
            warehouse_name: "Main warehouse".to_owned(),
            can_ack: 1,
            recipients: 2,
        }
    }

    #[test]
    fn retains_due_date_phase_and_id_order() {
        let rows = vec![
            row("2026-11-10", AlertPhase::Overdue, "alert-a"),
            row("2026-11-10", AlertPhase::DueSoon, "alert-b"),
            row("2026-11-11", AlertPhase::DueSoon, "alert-c"),
        ];
        assert!(assert_bounded(&rows).is_ok());
        let reversed = [rows[1].clone(), rows[0].clone()];
        assert_eq!(
            assert_bounded(&reversed).unwrap_err().code,
            "QUALITY_ALERT_QUEUE_CHANGED"
        );
    }

    #[test]
    fn terminal_and_acknowledged_alerts_are_complete() {
        let mut alert = row("2026-11-10", AlertPhase::Overdue, "alert-a");
        assert!(!alert.is_complete());
        alert.status = "released".to_owned();
        assert!(alert.is_complete());
        alert.status = "held".to_owned();
        alert.acknowledged_by = Some("inspector".to_owned());
        assert!(alert.is_complete());
    }

    #[test]
    fn bounded_source_refuses_invalid_counts() {
        assert!(assert_bounded(&vec![row("2026-11-10", AlertPhase::Overdue, "a"); MAX_ALERTS + 1]).is_err());
        let mut invalid = row("2026-11-10", AlertPhase::Overdue, "a");
        invalid.can_ack = 2;
        assert_eq!(
            assert_bounded(&[invalid]).unwrap_err().code,
            "QUALITY_ALERT_QUEUE_INVALID"
        );
    }
}
