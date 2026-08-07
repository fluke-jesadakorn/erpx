use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, ReferenceCode};

owned_uuid_reference!(PrincipalRef);
owned_uuid_reference!(RoleRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalStatus {
    Invited,
    Active,
    Suspended,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalAccessFact {
    pub principal: PrincipalRef,
    pub display_name: DisplayName,
    pub status: PrincipalStatus,
    pub assigned_roles: Vec<RoleRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    PrincipalAccessChanged(PrincipalAccessFact),
    PrincipalDisabled { principal: PrincipalRef },
    RoleDefinitionChanged { role: RoleRef, code: ReferenceCode },
}

impl_integration_event!(Event, BoundedContext::IdentityAccess, {
    Self::PrincipalAccessChanged(_) => "identity_access.principal_access_changed",
    Self::PrincipalDisabled { .. } => "identity_access.principal_disabled",
    Self::RoleDefinitionChanged { .. } => "identity_access.role_definition_changed",
});
