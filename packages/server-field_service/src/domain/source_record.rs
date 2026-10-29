//! Field source-record lineage policy; caller supplies owner-verified facts.
use erpx_server_shared::public::{BusinessError, FailureKind};
pub struct FieldRecordLineage {
    pub agreement_scope: bool,
    pub customer: bool,
    pub ticket: bool,
    pub asset: bool,
    pub agreement_asset_matches: bool,
    pub history: bool,
    pub parts_history: bool,
    pub has_asset: bool,
}
pub fn require_source_lineage(f: &FieldRecordLineage) -> Result<(), BusinessError> {
    if !(f.agreement_scope
        && f.customer
        && f.ticket
        && f.asset
        && f.agreement_asset_matches
        && f.history
        && (!f.has_asset || f.parts_history))
    {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SOURCE_RECORD_NOT_FOUND",
            "Source record is unavailable in this work scope.",
        ));
    }
    Ok(())
}
