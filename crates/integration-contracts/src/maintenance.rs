use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, ReferenceCode, assets::AssetRef};

owned_uuid_reference!(WorkOrderRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkOrderStatus {
    Planned,
    Scheduled,
    InProgress,
    Blocked,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkOrderFact {
    pub work_order: WorkOrderRef,
    pub work_order_code: ReferenceCode,
    pub asset: AssetRef,
    pub status: WorkOrderStatus,
    pub scheduled_for: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    WorkOrderChanged(WorkOrderFact),
}

impl_integration_event!(Event, BoundedContext::Maintenance, {
    Self::WorkOrderChanged(_) => "maintenance.work_order_changed",
});
