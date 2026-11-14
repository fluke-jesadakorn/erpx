//! Pure Field visit link withdrawal policy; authorization remains external.
pub use crate::domain::attachment_command::{WithdrawalDecision, withdrawal_decision};
pub fn valid_field_purpose(target: &str, component: &str, purpose: &str) -> bool {
    target == "field_service_visit"
        && component == "record"
        && matches!(
            purpose,
            "site_photo" | "technician_report" | "signed_job_sheet" | "exception_evidence"
        )
}
