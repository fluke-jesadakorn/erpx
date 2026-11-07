//! Files use-case preflight, independent of persistence and transport.
use crate::domain::field_visit_link::{WithdrawalDecision, withdrawal_decision};
use erpx_server_shared::public::BusinessError;
pub fn check_withdrawal_actor(
    state: &str,
    creator: &str,
    actor: &str,
) -> Result<(), BusinessError> {
    use erpx_server_shared::public::FailureKind;
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
    Ok(())
}
pub struct WithdrawalFacts<'a> {
    pub state: &'a str,
    pub creator: &'a str,
    pub withdrawn_by: Option<&'a str>,
    pub previous_reason: Option<&'a str>,
    pub actor: &'a str,
    pub reason: &'a str,
}
pub fn prepare_withdrawal(f: WithdrawalFacts<'_>) -> Result<WithdrawalDecision, BusinessError> {
    withdrawal_decision(
        f.state,
        f.creator,
        f.withdrawn_by,
        f.previous_reason,
        f.actor,
        f.reason,
    )
}
