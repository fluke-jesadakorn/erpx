//! Files use-case preflight, independent of persistence and transport.
pub use crate::application::attachment_command::{WithdrawalFacts, prepare_withdrawal};
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
