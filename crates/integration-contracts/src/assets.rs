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
        /// Human-readable maintenance title. Older retained events do not
        /// carry this field, so consumers must fall back to `request_code`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<DisplayName>,
    },
}

impl_integration_event!(Event, BoundedContext::Assets, {
    Self::AssetConditionChanged(_) => "assets.condition_changed",
    Self::MaintenanceRequested { .. } => "assets.maintenance_requested",
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_maintenance_request_without_title_remains_decodable() {
        let asset = uuid::Uuid::from_u128(1);
        let payload = serde_json::json!({
            "type": "maintenance_requested",
            "fact": {
                "asset": asset,
                "request_code": "MR-1"
            }
        });
        let event: Event = serde_json::from_value(payload).unwrap();
        assert!(matches!(
            event,
            Event::MaintenanceRequested {
                asset: decoded,
                request_code,
                title: None,
            } if decoded.as_uuid() == asset && request_code.as_str() == "MR-1"
        ));
    }
}
