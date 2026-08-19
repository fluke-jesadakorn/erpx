use erpx_kernel::HoldingId;
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, ReferenceCode};

owned_uuid_reference!(EnvironmentRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingStatus {
    PendingSubscription,
    Provisioning,
    ProvisioningFailed,
    Active,
    Suspended,
    Recovery,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingLifecycleFact {
    pub holding: HoldingId,
    pub code: ReferenceCode,
    pub name: DisplayName,
    pub environment: EnvironmentRef,
    pub status: HoldingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    HoldingLifecycleChanged(HoldingLifecycleFact),
}

impl_integration_event!(Event, BoundedContext::PlatformAdministration, {
    Self::HoldingLifecycleChanged(_) => "platform_administration.holding_lifecycle_changed",
});
