use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};

use crate::{
    BoundedContext, ContractValidationError, DisplayName, IntegrationEvent, RecordRef,
    ReferenceCode, customer::CustomerRef,
};

owned_uuid_reference!(LegalCaseRef);
owned_uuid_reference!(ObligationRef);
owned_uuid_reference!(LegalContractRef);
owned_uuid_reference!(LegalMatterRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationStatus {
    Open,
    Satisfied,
    Waived,
    Breached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatterStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordRelation {
    GovernedBy,
    Supports,
    BilledUnder,
    AttributedTo,
    Disputes,
    SettlementFor,
    Supersedes,
    AccountingFor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkedFact {
    pub matter: LegalMatterRef,
    pub relation: RecordRelation,
    pub target: RecordRef,
    pub target_scope: TenantScope,
    pub target_digest: ReferenceCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatterChangedFact {
    pub matter: LegalMatterRef,
    pub matter_reference: ReferenceCode,
    pub title: DisplayName,
    pub status: MatterStatus,
    pub link_count: u64,
    pub legal_hold_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalHoldChangedFact {
    pub matter: LegalMatterRef,
    pub active: bool,
    pub reason: DisplayName,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalObligationFact {
    pub obligation: ObligationRef,
    pub legal_case: LegalCaseRef,
    pub customer: Option<CustomerRef>,
    pub obligation_code: ReferenceCode,
    pub title: DisplayName,
    pub status: ObligationStatus,
    pub due_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    ObligationChanged(LegalObligationFact),
    MatterChanged(MatterChangedFact),
    RecordLinked(RecordLinkedFact),
    LegalHoldChanged(LegalHoldChangedFact),
}

impl IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Legal
    }

    fn event_type(&self) -> &'static str {
        match self {
            Self::ObligationChanged(_) => "legal.obligation_changed",
            Self::MatterChanged(_) => "legal.matter_changed",
            Self::RecordLinked(_) => "legal.record_linked",
            Self::LegalHoldChanged(_) => "legal.hold_changed",
        }
    }

    fn schema_version(&self) -> u16 {
        1
    }

    fn validate(&self) -> Result<(), ContractValidationError> {
        match self {
            Self::MatterChanged(fact) if fact.link_count > 1_024 => {
                Err(ContractValidationError::OutOfRange("matter.link_count"))
            }
            Self::RecordLinked(fact) => {
                fact.target.validate()?;
                fact.target_scope
                    .validate()
                    .map_err(|_| ContractValidationError::InvalidTenantScope)?;
                let digest = fact
                    .target_digest
                    .as_str()
                    .strip_prefix("sha256:")
                    .unwrap_or(fact.target_digest.as_str());
                if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    Err(ContractValidationError::OutOfRange("record_link.digest"))
                } else {
                    Ok(())
                }
            }
            Self::LegalHoldChanged(fact) if fact.changed_at.timestamp() < 0 => {
                Err(ContractValidationError::InvalidTimestamp)
            }
            _ => Ok(()),
        }
    }
}
