use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::PlanningDomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Demand,
    Supply,
    Capacity,
}
impl PlanKind {
    pub const fn token(self) -> &'static str {
        match self {
            Self::Demand => "demand_plan",
            Self::Supply => "supply_plan",
            Self::Capacity => "capacity_plan",
        }
    }
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Demand => "planning.demand-plan",
            Self::Supply => "planning.supply-plan",
            Self::Capacity => "planning.capacity-plan",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Frozen,
    Published,
    Superseded,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningPlan {
    id: Uuid,
    version: u64,
    scope: TenantScope,
    kind: PlanKind,
    scenario: StableRecordRef,
    input_position: u64,
    policy_digest: String,
    status: PlanStatus,
    updated_at: DateTime<Utc>,
}

impl PlanningPlan {
    pub fn draft(
        id: Uuid,
        scope: TenantScope,
        kind: PlanKind,
        scenario: StableRecordRef,
        input_position: u64,
        policy_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, PlanningDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            kind,
            scenario,
            input_position,
            policy_digest: policy_digest.into(),
            status: PlanStatus::Draft,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn freeze(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        self.transition(PlanStatus::Draft, PlanStatus::Frozen, at)
    }
    pub fn publish(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        self.transition(PlanStatus::Frozen, PlanStatus::Published, at)
    }
    pub fn supersede(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        self.transition(PlanStatus::Published, PlanStatus::Superseded, at)
    }
    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        if matches!(
            self.status,
            PlanStatus::Published | PlanStatus::Superseded | PlanStatus::Cancelled
        ) {
            return Err(PlanningDomainError::InvalidTransition);
        }
        self.status = PlanStatus::Cancelled;
        self.advance(at)
    }
    pub fn validate(&self) -> Result<(), PlanningDomainError> {
        self.scope.validate()?;
        self.scenario.validate()?;
        if self.id.is_nil()
            || self.id.get_version_num() != 4
            || self.version == 0
            || self.input_position == 0
            || self.policy_digest.len() != 64
            || !self.policy_digest.bytes().all(|v| v.is_ascii_hexdigit())
        {
            return Err(PlanningDomainError::InvalidPlan);
        }
        Ok(())
    }
    fn transition(
        &mut self,
        expected: PlanStatus,
        next: PlanStatus,
        at: DateTime<Utc>,
    ) -> Result<(), PlanningDomainError> {
        if self.status != expected {
            return Err(PlanningDomainError::InvalidTransition);
        }
        self.status = next;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        if at < self.updated_at {
            return Err(PlanningDomainError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlanningDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn kind(&self) -> PlanKind {
        self.kind
    }
    pub const fn status(&self) -> PlanStatus {
        self.status
    }
    pub const fn scenario(&self) -> &StableRecordRef {
        &self.scenario
    }
    pub const fn input_position(&self) -> u64 {
        self.input_position
    }
    pub fn policy_digest(&self) -> &str {
        &self.policy_digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::RecordId;
    fn scope() -> TenantScope {
        TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        }
    }
    fn scenario() -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Planning,
            RecordTypeId::try_new("planning.scenario").unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn three_plan_kinds_are_first_class_and_versioned() {
        for kind in [PlanKind::Demand, PlanKind::Supply, PlanKind::Capacity] {
            let now = Utc::now();
            let mut plan = PlanningPlan::draft(
                Uuid::new_v4(),
                scope(),
                kind,
                scenario(),
                1,
                "a".repeat(64),
                now,
            )
            .unwrap();
            plan.freeze(now).unwrap();
            plan.publish(now).unwrap();
            plan.supersede(now).unwrap();
            assert_eq!(plan.version(), 4);
            assert!(plan.cancel(now).is_err());
        }
    }
}
