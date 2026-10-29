//! Pure Field visit link withdrawal policy; authorization remains external.
use erpx_server_shared::public::{BusinessError, FailureKind};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WithdrawalDecision {
    Apply,
    Replay,
}
pub fn withdrawal_decision(
    state: &str,
    creator: &str,
    withdrawn_by: Option<&str>,
    previous_reason: Option<&str>,
    actor: &str,
    reason: &str,
) -> Result<WithdrawalDecision, BusinessError> {
    if state == "bound" {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "BOUND_ATTACHMENT_IMMUTABLE",
            "Bound evidence cannot be withdrawn. Add a correction instead.",
        ));
    }
    if creator != actor {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ATTACHMENT_WITHDRAW_FORBIDDEN",
            "Only the person who staged this file may withdraw it.",
        ));
    }
    match state {
        "draft" => Ok(WithdrawalDecision::Apply),
        "withdrawn" if withdrawn_by == Some(actor) && previous_reason == Some(reason) => {
            Ok(WithdrawalDecision::Replay)
        }
        "withdrawn" => Err(BusinessError::new(
            FailureKind::Conflict,
            "IDEMPOTENCY_CONFLICT",
            "This file was withdrawn with a different command.",
        )),
        _ => Err(BusinessError::new(
            FailureKind::Conflict,
            "ATTACHMENT_LINK_LOCKED",
            "This file link is closed.",
        )),
    }
}
pub fn valid_field_purpose(target: &str, component: &str, purpose: &str) -> bool {
    target == "field_service_visit"
        && component == "record"
        && matches!(
            purpose,
            "site_photo" | "technician_report" | "signed_job_sheet" | "exception_evidence"
        )
}
