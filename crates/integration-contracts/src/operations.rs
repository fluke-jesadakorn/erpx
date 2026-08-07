use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, ContractValidationError, DisplayName, ReferenceCode};

owned_uuid_reference!(OperationalProcessRef);
owned_uuid_reference!(WorkQueuePolicyRef);

/// Typed source facts accepted by the Operations projector. Adding a source is
/// an explicit contract change, so source ownership cannot be hidden in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "fact", rename_all = "snake_case")]
pub enum SourceFact {
    Commercial(crate::commercial::SubscriptionFact),
    Customer(crate::customer::CustomerProfileFact),
    Marketing(crate::marketing::EngagementFact),
    Crm(crate::crm::OpportunityFact),
    Procurement(crate::procurement::ReleasedPurchaseOrderFact),
    Inventory(crate::inventory::StockMovementFact),
    Assets(crate::assets::AssetConditionFact),
    Maintenance(crate::maintenance::WorkOrderFact),
    Quality(crate::quality::InspectionFact),
    Sales(crate::sales::SalesOrderFact),
    PeoplePayroll(crate::people_payroll::PayrollFinalizationFact),
    PosSale(crate::pos::PosSaleFact),
    PosRefund(crate::pos::PosRefundFact),
    Finance(crate::finance::JournalPostingFact),
    Legal(crate::legal::LegalObligationFact),
    Support(crate::support::SupportCaseFact),
    SupportIncident(crate::support::SupportIncidentFact),
}

impl SourceFact {
    pub const fn source_context(&self) -> BoundedContext {
        match self {
            Self::Commercial(_) => BoundedContext::Commercial,
            Self::Customer(_) => BoundedContext::Customer,
            Self::Marketing(_) => BoundedContext::Marketing,
            Self::Crm(_) => BoundedContext::Crm,
            Self::Procurement(_) => BoundedContext::Procurement,
            Self::Inventory(_) => BoundedContext::Inventory,
            Self::Assets(_) => BoundedContext::Assets,
            Self::Maintenance(_) => BoundedContext::Maintenance,
            Self::Quality(_) => BoundedContext::Quality,
            Self::Sales(_) => BoundedContext::Sales,
            Self::PeoplePayroll(_) => BoundedContext::PeoplePayroll,
            Self::PosSale(_) | Self::PosRefund(_) => BoundedContext::Pos,
            Self::Finance(_) => BoundedContext::Finance,
            Self::Legal(_) => BoundedContext::Legal,
            Self::Support(_) | Self::SupportIncident(_) => BoundedContext::Support,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalSignalFact {
    pub process: OperationalProcessRef,
    pub process_code: ReferenceCode,
    pub state: ReferenceCode,
    pub observed_at: DateTime<Utc>,
    pub source: SourceFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkQueuePolicyFact {
    pub policy: WorkQueuePolicyRef,
    pub name: DisplayName,
    pub allowed_sources: Vec<BoundedContext>,
    pub escalation_after_minutes: u32,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    OperationalStateChanged(OperationalSignalFact),
    WorkQueuePolicyPublished(WorkQueuePolicyFact),
}

impl crate::IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Operations
    }

    fn event_type(&self) -> &'static str {
        match self {
            Self::OperationalStateChanged(_) => "operations.state_changed",
            Self::WorkQueuePolicyPublished(_) => "operations.work_queue_policy_published",
        }
    }

    fn schema_version(&self) -> u16 {
        1
    }

    fn validate(&self) -> Result<(), ContractValidationError> {
        let Self::WorkQueuePolicyPublished(fact) = self else {
            return Ok(());
        };
        if fact.allowed_sources.is_empty()
            || fact.allowed_sources.len() > 16
            || fact.allowed_sources.contains(&BoundedContext::Operations)
            || fact
                .allowed_sources
                .iter()
                .enumerate()
                .any(|(index, source)| fact.allowed_sources[..index].contains(source))
        {
            return Err(ContractValidationError::OutOfRange(
                "operations.allowed_sources",
            ));
        }
        if !(1..=10_080).contains(&fact.escalation_after_minutes) {
            return Err(ContractValidationError::OutOfRange(
                "operations.escalation_after_minutes",
            ));
        }
        if fact.published_at.timestamp() < 0 {
            return Err(ContractValidationError::InvalidTimestamp);
        }
        Ok(())
    }
}
