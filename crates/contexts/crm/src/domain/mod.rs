use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_CRM_TEXT_BYTES: usize = 255;

macro_rules! crm_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);
        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, CrmDomainError> {
                if value.is_nil() {
                    return Err(CrmDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = CrmDomainError;
            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}
crm_id!(CrmLeadId, "crm_lead_id");
crm_id!(OpportunityId, "opportunity_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityStage {
    Qualified,
    Discovery,
    Proposal,
    Negotiation,
    Won,
    Lost,
}
impl OpportunityStage {
    const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Qualified, Self::Discovery | Self::Lost)
                | (Self::Discovery, Self::Proposal | Self::Lost)
                | (Self::Proposal, Self::Negotiation | Self::Lost)
                | (Self::Negotiation, Self::Won | Self::Lost)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CrmDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("CRM records require an exact company or branch scope")]
    InvalidScope,
    #[error("CRM opportunity amount cannot be negative")]
    NegativeAmount,
    #[error("CRM currency must be a three-letter uppercase ASCII code")]
    InvalidCurrency,
    #[error("CRM opportunity transition is not allowed")]
    InvalidTransition,
    #[error("CRM timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("CRM aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrmLead {
    id: CrmLeadId,
    version: u64,
    scope: TenantScope,
    marketing_candidate_id: Uuid,
    customer_id: Option<Uuid>,
    source: String,
    qualification_score_basis_points: u16,
    accepted_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}
impl CrmLead {
    #[allow(clippy::too_many_arguments)]
    pub fn accept(
        id: CrmLeadId,
        scope: TenantScope,
        marketing_candidate_id: Uuid,
        customer_id: Option<Uuid>,
        source: String,
        score: u16,
        at: DateTime<Utc>,
    ) -> Result<Self, CrmDomainError> {
        exact_scope(&scope)?;
        require_id(marketing_candidate_id, "marketing_candidate_id")?;
        if customer_id.is_some_and(|id| id.is_nil()) {
            return Err(CrmDomainError::NilIdentifier {
                field: "customer_id",
            });
        }
        validate_text(&source, "source")?;
        if score == 0 || score > 10_000 {
            return Err(CrmDomainError::Required {
                field: "qualification_score_basis_points",
            });
        }
        let mut value = Self {
            id,
            version: 1,
            scope,
            marketing_candidate_id,
            customer_id,
            source,
            qualification_score_basis_points: score,
            accepted_at: at,
            uncommitted_events: vec![],
        };
        value.uncommitted_events.push(DomainEvent::LeadAccepted {
            lead_id: id,
            marketing_candidate_id,
            customer_id,
            source: value.source.clone(),
        });
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), CrmDomainError> {
        exact_scope(&self.scope)?;
        require_id(self.marketing_candidate_id, "marketing_candidate_id")?;
        if self.customer_id.is_some_and(|id| id.is_nil()) {
            return Err(CrmDomainError::NilIdentifier {
                field: "customer_id",
            });
        }
        validate_text(&self.source, "source")?;
        if self.version == 0
            || self.qualification_score_basis_points == 0
            || self.qualification_score_basis_points > 10_000
        {
            return Err(CrmDomainError::Required {
                field: "qualification_score_basis_points",
            });
        }
        Ok(())
    }
    pub const fn id(&self) -> CrmLeadId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn marketing_candidate_id(&self) -> Uuid {
        self.marketing_candidate_id
    }
    pub const fn customer_id(&self) -> Option<Uuid> {
        self.customer_id
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub const fn qualification_score_basis_points(&self) -> u16 {
        self.qualification_score_basis_points
    }
    pub const fn accepted_at(&self) -> DateTime<Utc> {
        self.accepted_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrmOpportunity {
    id: OpportunityId,
    version: u64,
    scope: TenantScope,
    accepted_lead_id: CrmLeadId,
    customer_id: Uuid,
    name: String,
    expected_value_minor: i64,
    currency: String,
    stage: OpportunityStage,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}
impl CrmOpportunity {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: OpportunityId,
        scope: TenantScope,
        accepted_lead_id: CrmLeadId,
        customer_id: Uuid,
        name: String,
        expected_value_minor: i64,
        currency: String,
        at: DateTime<Utc>,
    ) -> Result<Self, CrmDomainError> {
        exact_scope(&scope)?;
        require_id(customer_id, "customer_id")?;
        validate_text(&name, "name")?;
        validate_money(expected_value_minor, &currency)?;
        let mut value = Self {
            id,
            version: 1,
            scope,
            accepted_lead_id,
            customer_id,
            name,
            expected_value_minor,
            currency,
            stage: OpportunityStage::Qualified,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![],
        };
        value.publish_changed();
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), CrmDomainError> {
        exact_scope(&self.scope)?;
        require_id(self.customer_id, "customer_id")?;
        validate_text(&self.name, "name")?;
        validate_money(self.expected_value_minor, &self.currency)?;
        if self.version == 0 {
            return Err(CrmDomainError::VersionOverflow);
        }
        if self.updated_at < self.created_at {
            return Err(CrmDomainError::InvalidTimestamp);
        }
        Ok(())
    }
    pub fn transition(
        &mut self,
        next: OpportunityStage,
        at: DateTime<Utc>,
    ) -> Result<(), CrmDomainError> {
        if !self.stage.can_transition_to(next) {
            return Err(CrmDomainError::InvalidTransition);
        }
        if at < self.updated_at {
            return Err(CrmDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(CrmDomainError::VersionOverflow)?;
        self.stage = next;
        self.updated_at = at;
        self.publish_changed();
        Ok(())
    }
    fn publish_changed(&mut self) {
        self.uncommitted_events
            .push(DomainEvent::OpportunityChanged {
                opportunity_id: self.id,
                customer_id: self.customer_id,
                stage: self.stage,
                expected_value_minor: self.expected_value_minor,
                currency: self.currency.clone(),
            });
    }
    pub const fn id(&self) -> OpportunityId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn accepted_lead_id(&self) -> CrmLeadId {
        self.accepted_lead_id
    }
    pub const fn customer_id(&self) -> Uuid {
        self.customer_id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub const fn expected_value_minor(&self) -> i64 {
        self.expected_value_minor
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub const fn stage(&self) -> OpportunityStage {
        self.stage
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum CrmAggregate {
    Lead(CrmLead),
    Opportunity(CrmOpportunity),
}
impl CrmAggregate {
    pub const fn version(&self) -> u64 {
        match self {
            Self::Lead(v) => v.version(),
            Self::Opportunity(v) => v.version(),
        }
    }
    pub const fn scope(&self) -> &TenantScope {
        match self {
            Self::Lead(v) => v.scope(),
            Self::Opportunity(v) => v.scope(),
        }
    }
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Lead(v) => v.id().as_uuid(),
            Self::Opportunity(v) => v.id().as_uuid(),
        }
    }
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Lead(_) => "crm_lead",
            Self::Opportunity(_) => "opportunity",
        }
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Lead(v) => v.take_uncommitted_events(),
            Self::Opportunity(v) => v.take_uncommitted_events(),
        }
    }
    pub fn validate(&self) -> Result<(), CrmDomainError> {
        match self {
            Self::Lead(value) => value.validate(),
            Self::Opportunity(value) => value.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    LeadAccepted {
        lead_id: CrmLeadId,
        marketing_candidate_id: Uuid,
        customer_id: Option<Uuid>,
        source: String,
    },
    OpportunityChanged {
        opportunity_id: OpportunityId,
        customer_id: Uuid,
        stage: OpportunityStage,
        expected_value_minor: i64,
        currency: String,
    },
}

fn exact_scope(scope: &TenantScope) -> Result<(), CrmDomainError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } if !holding_id.is_nil() && !company_id.is_nil() => Ok(()),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil() => Ok(()),
        _ => Err(CrmDomainError::InvalidScope),
    }
}
fn require_id(value: Uuid, field: &'static str) -> Result<(), CrmDomainError> {
    if value.is_nil() {
        Err(CrmDomainError::NilIdentifier { field })
    } else {
        Ok(())
    }
}
fn validate_text(value: &str, field: &'static str) -> Result<(), CrmDomainError> {
    if value.trim().is_empty() {
        return Err(CrmDomainError::Required { field });
    }
    if value.len() > MAX_CRM_TEXT_BYTES {
        return Err(CrmDomainError::TooLong {
            field,
            maximum: MAX_CRM_TEXT_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(CrmDomainError::ControlCharacter { field });
    }
    Ok(())
}
fn validate_money(amount: i64, currency: &str) -> Result<(), CrmDomainError> {
    if amount < 0 {
        return Err(CrmDomainError::NegativeAmount);
    }
    if currency.len() != 3 || !currency.bytes().all(|v| v.is_ascii_uppercase()) {
        return Err(CrmDomainError::InvalidCurrency);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn opportunity() -> CrmOpportunity {
        CrmOpportunity::create(
            OpportunityId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            scope(),
            CrmLeadId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            Uuid::from_u128(5),
            "Renewal".into(),
            10_000,
            "THB".into(),
            Utc::now(),
        )
        .unwrap()
    }
    #[test]
    fn opportunity_follows_forward_only_pipeline() {
        let mut value = opportunity();
        let at = value.created_at();
        assert_eq!(
            value.transition(OpportunityStage::Proposal, at),
            Err(CrmDomainError::InvalidTransition)
        );
        value.transition(OpportunityStage::Discovery, at).unwrap();
        value.transition(OpportunityStage::Proposal, at).unwrap();
        value.transition(OpportunityStage::Lost, at).unwrap();
        assert_eq!(
            value.transition(OpportunityStage::Won, at),
            Err(CrmDomainError::InvalidTransition)
        );
    }
    #[test]
    fn opportunity_money_is_bounded() {
        assert!(matches!(
            CrmOpportunity::create(
                OpportunityId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
                scope(),
                CrmLeadId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
                Uuid::from_u128(5),
                "x".into(),
                -1,
                "thb".into(),
                Utc::now()
            ),
            Err(CrmDomainError::NegativeAmount)
        ));
    }
    #[test]
    fn accepted_marketing_fact_is_immutable_inside_crm() {
        let lead = CrmLead::accept(
            CrmLeadId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            scope(),
            Uuid::from_u128(8),
            None,
            "campaign".into(),
            9000,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(lead.marketing_candidate_id(), Uuid::from_u128(8));
        assert_eq!(lead.version(), 1);
    }
}
