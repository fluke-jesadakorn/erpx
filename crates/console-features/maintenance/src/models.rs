//! Target-neutral Maintenance view models. Transport/domain mapping lives in the gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceRequestItem {
    pub source_event_id: String,
    pub source_aggregate_version: u64,
    pub asset_id: String,
    pub request_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFactItem {
    pub asset_id: String,
    pub asset_code: String,
    pub name: String,
    pub location_id: String,
    pub condition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DowntimeItem {
    pub downtime_id: String,
    pub started_at: String,
    pub ended_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkOrderItem {
    pub work_order_id: String,
    pub version: u64,
    pub code: String,
    pub scope: String,
    pub request: MaintenanceRequestItem,
    pub asset_fact: AssetFactItem,
    pub title: String,
    pub status: String,
    pub scheduled_for: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub downtime: Vec<DowntimeItem>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkOrderItems {
    pub work_orders: Vec<WorkOrderItem>,
    pub next_cursor: Option<String>,
}
