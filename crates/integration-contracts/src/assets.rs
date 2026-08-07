use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, ReferenceCode};

owned_uuid_reference!(AssetRef);
owned_uuid_reference!(AssetLocationRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetCondition {
    Operational,
    Degraded,
    OutOfService,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetConditionFact {
    pub asset: AssetRef,
    pub asset_code: ReferenceCode,
    pub name: DisplayName,
    pub location: AssetLocationRef,
    pub condition: AssetCondition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    AssetConditionChanged(AssetConditionFact),
    MaintenanceRequested {
        asset: AssetRef,
        request_code: ReferenceCode,
    },
}

impl_integration_event!(Event, BoundedContext::Assets, {
    Self::AssetConditionChanged(_) => "assets.condition_changed",
    Self::MaintenanceRequested { .. } => "assets.maintenance_requested",
});
