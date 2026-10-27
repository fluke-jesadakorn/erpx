//! A claim grants no personal or content authority. Its opaque handle permits
//! only that exact still-running lease to be closed by the owning worker.
use crate::domain::processing::{LegalProcessingCursor, LegalProcessingLease};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId};

#[derive(Clone)]
pub struct ClaimedLegalProcessing {
    pub(crate) lease: LegalProcessingLease,
    pub(crate) database: String,
}
impl ClaimedLegalProcessing {
    pub fn lease(&self) -> &LegalProcessingLease {
        &self.lease
    }
}
pub struct LegalMaintenanceScan {
    pub claim: Option<ClaimedLegalProcessing>,
    pub next_cursor: Option<LegalProcessingCursor>,
}
#[derive(Clone)]
pub struct LegalMaintenanceScope {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
}
pub(crate) fn require_failure_code(code: &str) -> Result<(), BusinessError> {
    if code.is_empty()
        || code.len() > 100
        || !code
            .bytes()
            .all(|v| v.is_ascii_uppercase() || v.is_ascii_digit() || v == b'_')
    {
        return Err(BusinessError::invalid(
            "LEGAL_PROCESSING_ERROR_CODE_INVALID",
            "Use a closed processing failure code.",
        ));
    }
    Ok(())
}
