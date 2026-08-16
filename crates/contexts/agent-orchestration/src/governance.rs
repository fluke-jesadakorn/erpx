use crate::AgentDomainError;
use chrono::{DateTime, Utc};
use erpx_kernel::{RecordId, TenantScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentGovernanceRecordKind {
    DelegationTree,
    BudgetDecision,
}
impl AgentGovernanceRecordKind {
    pub const fn token(self) -> &'static str {
        match self {
            Self::DelegationTree => "delegation_tree",
            Self::BudgetDecision => "budget_decision",
        }
    }
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::DelegationTree => "agent.delegation-tree",
            Self::BudgetDecision => "agent.budget-decision",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentGovernanceStatus {
    Proposed,
    Approved,
    Active,
    Completed,
    Denied,
    Expired,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGovernanceRecord {
    id: Uuid,
    version: u64,
    scope: TenantScope,
    kind: AgentGovernanceRecordKind,
    subject_record_id: RecordId,
    policy_digest: String,
    status: AgentGovernanceStatus,
    updated_at: DateTime<Utc>,
}
impl AgentGovernanceRecord {
    pub fn propose(
        id: Uuid,
        scope: TenantScope,
        kind: AgentGovernanceRecordKind,
        subject_record_id: RecordId,
        policy_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            kind,
            subject_record_id,
            policy_digest: policy_digest.into(),
            status: AgentGovernanceStatus::Proposed,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn approve(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        self.transition(
            AgentGovernanceStatus::Proposed,
            AgentGovernanceStatus::Approved,
            at,
        )
    }
    pub fn activate(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        self.transition(
            AgentGovernanceStatus::Approved,
            AgentGovernanceStatus::Active,
            at,
        )
    }
    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        self.transition(
            AgentGovernanceStatus::Active,
            AgentGovernanceStatus::Completed,
            at,
        )
    }
    pub fn deny(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        self.transition(
            AgentGovernanceStatus::Proposed,
            AgentGovernanceStatus::Denied,
            at,
        )
    }
    pub fn validate(&self) -> Result<(), AgentDomainError> {
        self.scope
            .validate()
            .map_err(|_| AgentDomainError::InvalidGovernanceRecord)?;
        if self.id.is_nil()
            || self.id.get_version_num() != 4
            || self.version == 0
            || self.policy_digest.len() != 64
            || !self.policy_digest.bytes().all(|v| v.is_ascii_hexdigit())
        {
            return Err(AgentDomainError::InvalidGovernanceRecord);
        }
        Ok(())
    }
    fn transition(
        &mut self,
        expected: AgentGovernanceStatus,
        next: AgentGovernanceStatus,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if self.status != expected || at < self.updated_at {
            return Err(AgentDomainError::InvalidGovernanceRecord);
        }
        self.status = next;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(AgentDomainError::InvalidVersion)?;
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
    pub const fn kind(&self) -> AgentGovernanceRecordKind {
        self.kind
    }
    pub const fn status(&self) -> AgentGovernanceStatus {
        self.status
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub const fn subject_record_id(&self) -> RecordId {
        self.subject_record_id
    }
    pub fn policy_digest(&self) -> &str {
        &self.policy_digest
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> TenantScope {
        TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        }
    }
    #[test]
    fn governance_records_require_explicit_decisions() {
        for kind in [
            AgentGovernanceRecordKind::DelegationTree,
            AgentGovernanceRecordKind::BudgetDecision,
        ] {
            let now = Utc::now();
            let mut value = AgentGovernanceRecord::propose(
                Uuid::new_v4(),
                scope(),
                kind,
                RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
                "a".repeat(64),
                now,
            )
            .unwrap();
            assert!(value.activate(now).is_err());
            value.approve(now).unwrap();
            value.activate(now).unwrap();
            value.complete(now).unwrap();
            assert_eq!(value.version(), 4)
        }
    }
}
