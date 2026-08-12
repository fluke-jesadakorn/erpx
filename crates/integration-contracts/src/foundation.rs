use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AggregateKind, BoundedContext, ContractValidationError, MoneyAmount, assets, legal, marketing,
    organization, people_payroll,
};

owned_uuid_reference!(PartyRef);
owned_uuid_reference!(LocationRef);
owned_uuid_reference!(ProjectRef);
owned_uuid_reference!(CostCenterRef);

/// Typed accounting and operational dimensions carried by source facts.
///
/// The identifiers remain references only: no source context may dereference
/// another context's write repository while validating a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionSet {
    pub legal_entity: organization::CompanyRef,
    pub branch: Option<organization::BranchRef>,
    pub business_unit: Option<organization::BusinessUnitRef>,
    pub cost_center: Option<CostCenterRef>,
    pub location: Option<LocationRef>,
    pub project: Option<ProjectRef>,
    pub campaign: Option<marketing::CampaignRef>,
    pub channel: Option<marketing::ChannelRef>,
    pub party: Option<PartyRef>,
    pub employee: Option<people_payroll::EmployeeRef>,
    pub asset: Option<assets::AssetRef>,
    pub legal_matter: Option<legal::LegalMatterRef>,
    pub intercompany_partner: Option<organization::CompanyRef>,
}

impl DimensionSet {
    pub const fn for_legal_entity(legal_entity: organization::CompanyRef) -> Self {
        Self {
            legal_entity,
            branch: None,
            business_unit: None,
            cost_center: None,
            location: None,
            project: None,
            campaign: None,
            channel: None,
            party: None,
            employee: None,
            asset: None,
            legal_matter: None,
            intercompany_partner: None,
        }
    }
}

/// Stable, versioned link to a record owned by another bounded context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordRef {
    pub context: BoundedContext,
    pub aggregate_kind: AggregateKind,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

impl RecordRef {
    pub fn try_new(
        context: BoundedContext,
        aggregate_kind: impl Into<String>,
        aggregate_id: Uuid,
        aggregate_version: u64,
    ) -> Result<Self, ContractValidationError> {
        if aggregate_id.is_nil() {
            return Err(ContractValidationError::NilIdentifier);
        }
        if aggregate_version == 0 {
            return Err(ContractValidationError::ZeroVersion(
                "record_ref.aggregate_version",
            ));
        }
        Ok(Self {
            context,
            aggregate_kind: AggregateKind::try_new(aggregate_kind)?,
            aggregate_id,
            aggregate_version,
        })
    }

    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.aggregate_id.is_nil() {
            return Err(ContractValidationError::NilIdentifier);
        }
        if self.aggregate_version == 0 {
            return Err(ContractValidationError::ZeroVersion(
                "record_ref.aggregate_version",
            ));
        }
        Ok(())
    }
}

/// Transaction, functional, and optional group-reporting amounts retained at
/// the integration boundary. Conversion policy and rate authority stay in
/// Finance; source contexts may only publish the values Finance accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonetaryAmountSet {
    pub transaction: MoneyAmount,
    pub functional: MoneyAmount,
    pub reporting: Option<MoneyAmount>,
}
