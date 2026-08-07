use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, ReferenceCode};

owned_uuid_reference!(TenantRef);
owned_uuid_reference!(EnvironmentRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantStatus {
    Provisioning,
    Active,
    Suspended,
    Recovery,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantLifecycleFact {
    pub tenant: TenantRef,
    pub code: ReferenceCode,
    pub name: DisplayName,
    pub environment: EnvironmentRef,
    pub status: TenantStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    TenantLifecycleChanged(TenantLifecycleFact),
}

impl_integration_event!(Event, BoundedContext::PlatformAdministration, {
    Self::TenantLifecycleChanged(_) => "platform_administration.tenant_lifecycle_changed",
});
