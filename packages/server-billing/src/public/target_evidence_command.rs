//! Native Billing target evidence commands.
pub use crate::adapters::target_evidence_command::{
    PgTargetEvidenceRepository, execute_target_evidence,
};
pub use crate::application::target_evidence_command::{
    TargetEvidenceRepository, execute_target_evidence as execute_target_evidence_with,
};
pub use crate::domain::target_evidence_command::{
    ApproveTargetExtra, BillingTargetEvidenceCommand, RecordOperatingCost, ReviewTargetCapacity,
    ValidatedCapacity, ValidatedOperatingCost, ValidatedTargetEvidence, ValidatedTargetExtra,
    capacity_already_reviewed, evidence_required, extra_approval_required, extra_service_dates,
    future_month_required, month_not_closed, operating_cost_already_verified,
    target_assignment_required, target_period_frozen,
};
