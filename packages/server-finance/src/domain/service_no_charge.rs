//! Fully covered Field invoices fulfill a duty without creating a receivable.
use erpx_server_shared::public::{BusinessError, FailureKind};
pub fn require_no_charge_decision(
    draft: bool,
    version: i64,
    expected: i64,
    context_matches: bool,
    independent: bool,
) -> Result<(), BusinessError> {
    if !draft || version != expected || !context_matches {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_NO_CHARGE_CONTEXT_CHANGED",
            "Refresh the captured no-charge review before confirming.",
        ));
    }
    if !independent {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_APPROVER_REQUIRED",
            "Another Finance approver must confirm the original zero demand.",
        ));
    }
    Ok(())
}
