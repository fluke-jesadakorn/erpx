use erpx_integration_contracts::{DisplayName, ReferenceCode, organization};

use crate::domain::{DomainEvent, OrganizationNodeId, PublishedNodeState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    NodeIdentifier,
    HoldingIdentifier,
    CompanyIdentifier,
    BranchIdentifier,
    Code,
    Name,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<organization::Event>, IntegrationMappingError> {
    events.iter().map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Result<organization::Event, IntegrationMappingError> {
    match event {
        DomainEvent::Changed { state, at } => {
            let _ = at;
            Ok(organization::Event::OrganizationUnitChanged(fact(state)?))
        }
        DomainEvent::Retired { node_id, at } => {
            let _ = at;
            Ok(organization::Event::OrganizationUnitRetired {
                business_unit: business_unit_ref(*node_id)?,
            })
        }
    }
}

fn fact(
    state: &PublishedNodeState,
) -> Result<organization::OrganizationUnitFact, IntegrationMappingError> {
    let node_id = state.node_id;
    let holding_id = state.placement.holding_id(node_id);
    let company_id = state.placement.company_id(node_id);
    let branch_id = state.placement.branch_id(node_id);
    Ok(organization::OrganizationUnitFact {
        business_unit: business_unit_ref(node_id)?,
        code: ReferenceCode::try_new(state.code.clone())
            .map_err(|_| IntegrationMappingError::Code)?,
        name: DisplayName::try_new(state.name.clone())
            .map_err(|_| IntegrationMappingError::Name)?,
        holding: erpx_kernel::HoldingId::try_from_uuid(holding_id)
            .map_err(|_| IntegrationMappingError::HoldingIdentifier)?,
        company: company_id
            .map(|id| {
                organization::CompanyRef::try_from_uuid(id)
                    .map_err(|_| IntegrationMappingError::CompanyIdentifier)
            })
            .transpose()?,
        branch: branch_id
            .map(|id| {
                organization::BranchRef::try_from_uuid(id)
                    .map_err(|_| IntegrationMappingError::BranchIdentifier)
            })
            .transpose()?,
        active: state.active,
    })
}

fn business_unit_ref(
    node_id: OrganizationNodeId,
) -> Result<organization::BusinessUnitRef, IntegrationMappingError> {
    organization::BusinessUnitRef::try_from_uuid(node_id.as_uuid())
        .map_err(|_| IntegrationMappingError::NodeIdentifier)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use super::*;
    use crate::domain::{OrganizationNode, OrganizationPlacement};

    #[test]
    fn changed_branch_maps_to_owner_contract_without_domain_leakage() {
        let mut node = OrganizationNode::create(
            OrganizationNodeId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            OrganizationPlacement::Branch {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
            },
            "BKK".into(),
            "Bangkok".into(),
            Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        )
        .unwrap();
        let mapped = published_events(&node.take_uncommitted_events()).unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [organization::Event::OrganizationUnitChanged(fact)]
                if fact.branch.is_some() && fact.company.is_some() && fact.active
        ));
    }
}
