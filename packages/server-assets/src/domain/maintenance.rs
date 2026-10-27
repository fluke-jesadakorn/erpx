use erpx_server_shared::public::{CompanyId, HoldingId, RecordId};
/// Minimal physical-Asset relationship for actual Maintenance source assembly.
/// This does not grant record access, purchases, cost visibility or stock writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceAssetFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub asset_id: RecordId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub version: i64,
}
