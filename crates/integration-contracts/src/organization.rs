use erpx_kernel::HoldingId;
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, ReferenceCode};

owned_uuid_reference!(CompanyRef);
owned_uuid_reference!(BranchRef);
owned_uuid_reference!(BusinessUnitRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationUnitFact {
    pub business_unit: BusinessUnitRef,
    pub code: ReferenceCode,
    pub name: DisplayName,
    pub holding: HoldingId,
    pub company: Option<CompanyRef>,
    pub branch: Option<BranchRef>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    OrganizationUnitChanged(OrganizationUnitFact),
    OrganizationUnitRetired { business_unit: BusinessUnitRef },
}

impl_integration_event!(Event, BoundedContext::Organization, {
    Self::OrganizationUnitChanged(_) => "organization.unit_changed",
    Self::OrganizationUnitRetired { .. } => "organization.unit_retired",
});
