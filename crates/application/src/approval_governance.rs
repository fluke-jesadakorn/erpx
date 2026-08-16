use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use erpx_integration_contracts::RecordVersionRef;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_APPROVAL_GATES: usize = 64;
pub const MAX_APPROVAL_EVIDENCE: usize = 128;
pub const MAX_FRESH_MFA_AGE: Duration = Duration::minutes(15);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApprovalError {
    #[error("approval input is invalid")]
    Invalid,
    #[error("approval lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("approval version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("current approval authority is denied")]
    AuthorityDenied,
    #[error("required approval evidence is missing or unavailable")]
    EvidenceUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicyStatus {
    Draft,
    ReviewRequested,
    ChangesRequested,
    Approved,
    Scheduled,
    Active,
    Superseded,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecisionRule {
    Any,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalGatePolicy {
    pub gate_id: String,
    pub decision_rule: GateDecisionRule,
    pub required_approvals: u16,
    pub required_role: String,
    pub required_evidence: Vec<String>,
    pub deadline_after_seconds: u32,
    pub escalation_after_seconds: Option<u32>,
    pub requires_maker_checker: bool,
    pub requires_fresh_mfa: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalPolicyVersion {
    pub policy_id: Uuid,
    pub version: u64,
    pub scope: TenantScope,
    pub status: ApprovalPolicyStatus,
    pub gates: Vec<ApprovalGatePolicy>,
    pub effective_at: Option<DateTime<Utc>>,
    pub evidence_sha256: String,
}

impl ApprovalPolicyVersion {
    pub fn transition(
        &mut self,
        next: ApprovalPolicyStatus,
        effective_at: Option<DateTime<Utc>>,
    ) -> Result<(), ApprovalError> {
        let allowed = matches!(
            (self.status, next),
            (
                ApprovalPolicyStatus::Draft,
                ApprovalPolicyStatus::ReviewRequested
            ) | (
                ApprovalPolicyStatus::ReviewRequested,
                ApprovalPolicyStatus::ChangesRequested | ApprovalPolicyStatus::Approved
            ) | (
                ApprovalPolicyStatus::ChangesRequested,
                ApprovalPolicyStatus::ReviewRequested
            ) | (
                ApprovalPolicyStatus::Approved,
                ApprovalPolicyStatus::Scheduled
            ) | (
                ApprovalPolicyStatus::Scheduled,
                ApprovalPolicyStatus::Active
            ) | (
                ApprovalPolicyStatus::Active,
                ApprovalPolicyStatus::Superseded
            ) | (
                ApprovalPolicyStatus::Superseded,
                ApprovalPolicyStatus::Archived
            )
        );
        if !allowed
            || (next == ApprovalPolicyStatus::Scheduled && effective_at.is_none())
            || (next == ApprovalPolicyStatus::Active
                && self
                    .effective_at
                    .is_some_and(|value| effective_at.is_some_and(|activation| activation < value)))
        {
            return Err(ApprovalError::InvalidTransition);
        }
        if next == ApprovalPolicyStatus::Scheduled {
            self.effective_at = effective_at;
        }
        self.status = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ApprovalError> {
        if self.policy_id.is_nil()
            || self.version == 0
            || self.scope.validate().is_err()
            || self.gates.is_empty()
            || self.gates.len() > MAX_APPROVAL_GATES
            || !valid_digest(&self.evidence_sha256)
        {
            return Err(ApprovalError::Invalid);
        }
        let mut gates = HashSet::with_capacity(self.gates.len());
        for gate in &self.gates {
            if !valid_text(&gate.gate_id)
                || !valid_text(&gate.required_role)
                || gate.deadline_after_seconds == 0
                || gate.required_approvals == 0
                || !gates.insert(gate.gate_id.as_str())
                || gate.required_evidence.len() > MAX_APPROVAL_EVIDENCE
                || gate
                    .required_evidence
                    .iter()
                    .any(|value| !valid_text(value))
            {
                return Err(ApprovalError::Invalid);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    Approve,
    Reject,
    RequestChanges,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalSessionStatus {
    Pending,
    Approved,
    Rejected,
    ChangesRequested,
    Invalidated,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalAuthorityEvidence {
    pub principal_ref: String,
    pub role: String,
    pub scope: TenantScope,
    pub assignment_version: u64,
    pub valid_until: DateTime<Utc>,
    pub approval_limit_minor: i64,
    pub fresh_mfa_at: Option<DateTime<Utc>>,
    pub evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalDelegation {
    pub delegator_ref: String,
    pub delegate_ref: String,
    pub scope: TenantScope,
    pub source_assignment_version: u64,
    pub delegator_assignment_valid_until: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub reason: String,
}

impl ApprovalDelegation {
    pub fn validate_against(
        &self,
        authority: &ApprovalAuthorityEvidence,
        at: DateTime<Utc>,
    ) -> Result<(), ApprovalError> {
        if self.delegator_ref == self.delegate_ref
            || self.delegate_ref != authority.principal_ref
            || self.scope != authority.scope
            || self.source_assignment_version == 0
            || self.valid_from > at
            || self.valid_until <= at
            || self.valid_until > self.delegator_assignment_valid_until
            || !valid_text(&self.reason)
        {
            return Err(ApprovalError::AuthorityDenied);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub gate_id: String,
    pub actor_ref: String,
    pub represented_actor_ref: Option<String>,
    pub outcome: ApprovalOutcome,
    pub reason: String,
    pub authority_version: u64,
    pub authority_sha256: String,
    pub evidence_versions: Vec<String>,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerCommandRequest {
    pub source: RecordVersionRef,
    pub command_type: String,
    pub request_sha256: String,
    pub approval_session_id: Uuid,
    pub approval_session_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalSession {
    pub session_id: Uuid,
    pub version: u64,
    pub scope: TenantScope,
    pub source: RecordVersionRef,
    pub policy_id: Uuid,
    pub policy_version: u64,
    pub policy_sha256: String,
    pub requester_ref: String,
    pub amount_minor: i64,
    pub gates: Vec<ApprovalGatePolicy>,
    pub decisions: Vec<ApprovalDecision>,
    pub status: ApprovalSessionStatus,
    pub due_at: DateTime<Utc>,
    pub escalated_gate_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ApprovalSession {
    pub fn start(
        session_id: Uuid,
        source: RecordVersionRef,
        policy: &ApprovalPolicyVersion,
        requester_ref: String,
        amount_minor: i64,
        at: DateTime<Utc>,
    ) -> Result<Self, ApprovalError> {
        policy.validate()?;
        if session_id.is_nil()
            || policy.status != ApprovalPolicyStatus::Active
            || !valid_text(&requester_ref)
            || amount_minor < 0
        {
            return Err(ApprovalError::Invalid);
        }
        let due_seconds = policy
            .gates
            .iter()
            .map(|gate| gate.deadline_after_seconds)
            .max()
            .ok_or(ApprovalError::Invalid)?;
        Ok(Self {
            session_id,
            version: 1,
            scope: policy.scope.clone(),
            source,
            policy_id: policy.policy_id,
            policy_version: policy.version,
            policy_sha256: policy.evidence_sha256.clone(),
            requester_ref,
            amount_minor,
            gates: policy.gates.clone(),
            decisions: Vec::new(),
            status: ApprovalSessionStatus::Pending,
            due_at: at + Duration::seconds(i64::from(due_seconds)),
            escalated_gate_ids: Vec::new(),
            created_at: at,
            updated_at: at,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn decide(
        &mut self,
        expected_version: u64,
        gate_id: &str,
        outcome: ApprovalOutcome,
        reason: String,
        evidence_versions: Vec<String>,
        authority: ApprovalAuthorityEvidence,
        delegation: Option<&ApprovalDelegation>,
        at: DateTime<Utc>,
    ) -> Result<Option<OwnerCommandRequest>, ApprovalError> {
        if expected_version != self.version {
            return Err(ApprovalError::VersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        if self.status != ApprovalSessionStatus::Pending || at > self.due_at {
            return Err(ApprovalError::InvalidTransition);
        }
        if !valid_text(&reason) {
            return Err(ApprovalError::Invalid);
        }
        let gate = self
            .gates
            .iter()
            .find(|gate| gate.gate_id == gate_id)
            .ok_or(ApprovalError::Invalid)?;
        if authority.scope != self.scope
            || authority.assignment_version == 0
            || authority.valid_until <= at
            || authority.role != gate.required_role
            || authority.approval_limit_minor < self.amount_minor
            || !valid_digest(&authority.evidence_sha256)
            || (gate.requires_maker_checker && authority.principal_ref == self.requester_ref)
            || (gate.requires_fresh_mfa
                && authority
                    .fresh_mfa_at
                    .is_none_or(|mfa| mfa > at || at - mfa > MAX_FRESH_MFA_AGE))
        {
            return Err(ApprovalError::AuthorityDenied);
        }
        let represented = if let Some(value) = delegation {
            value.validate_against(&authority, at)?;
            Some(value.delegator_ref.clone())
        } else {
            None
        };
        if gate
            .required_evidence
            .iter()
            .any(|required| !evidence_versions.iter().any(|value| value == required))
            || evidence_versions.iter().any(|value| !valid_text(value))
        {
            return Err(ApprovalError::EvidenceUnavailable);
        }
        if self.decisions.iter().any(|decision| {
            decision.gate_id == gate_id && decision.actor_ref == authority.principal_ref
        }) {
            return Err(ApprovalError::InvalidTransition);
        }
        self.version = self.version.checked_add(1).ok_or(ApprovalError::Invalid)?;
        self.updated_at = at;
        self.decisions.push(ApprovalDecision {
            gate_id: gate_id.into(),
            actor_ref: authority.principal_ref,
            represented_actor_ref: represented,
            outcome,
            reason,
            authority_version: authority.assignment_version,
            authority_sha256: authority.evidence_sha256,
            evidence_versions,
            decided_at: at,
        });
        self.status = match outcome {
            ApprovalOutcome::Reject => ApprovalSessionStatus::Rejected,
            ApprovalOutcome::RequestChanges => ApprovalSessionStatus::ChangesRequested,
            ApprovalOutcome::Approve if self.all_gates_approved() => {
                ApprovalSessionStatus::Approved
            }
            ApprovalOutcome::Approve => ApprovalSessionStatus::Pending,
        };
        Ok(
            (self.status == ApprovalSessionStatus::Approved).then(|| OwnerCommandRequest {
                source: self.source.clone(),
                command_type: "ApplyApprovedTransition".into(),
                request_sha256: approval_request_digest(self),
                approval_session_id: self.session_id,
                approval_session_version: self.version,
            }),
        )
    }

    pub fn escalate_due(&mut self, at: DateTime<Utc>) -> Result<Vec<String>, ApprovalError> {
        if self.status != ApprovalSessionStatus::Pending {
            return Ok(Vec::new());
        }
        if at > self.due_at {
            self.status = ApprovalSessionStatus::Expired;
            self.version = self.version.checked_add(1).ok_or(ApprovalError::Invalid)?;
            self.updated_at = at;
            return Ok(Vec::new());
        }
        let elapsed = at - self.created_at;
        let mut added = Vec::new();
        for gate in &self.gates {
            if gate
                .escalation_after_seconds
                .is_some_and(|seconds| elapsed >= Duration::seconds(i64::from(seconds)))
                && !self.escalated_gate_ids.contains(&gate.gate_id)
            {
                self.escalated_gate_ids.push(gate.gate_id.clone());
                added.push(gate.gate_id.clone());
            }
        }
        if !added.is_empty() {
            self.version = self.version.checked_add(1).ok_or(ApprovalError::Invalid)?;
            self.updated_at = at;
        }
        Ok(added)
    }

    fn all_gates_approved(&self) -> bool {
        self.gates.iter().all(|gate| {
            let approvals = self
                .decisions
                .iter()
                .filter(|decision| {
                    decision.gate_id == gate.gate_id && decision.outcome == ApprovalOutcome::Approve
                })
                .count();
            match gate.decision_rule {
                GateDecisionRule::Any => approvals >= 1,
                GateDecisionRule::All => approvals >= usize::from(gate.required_approvals),
            }
        })
    }
}

fn approval_request_digest(session: &ApprovalSession) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(session).expect("serializable approval session");
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use erpx_integration_contracts::{
        StableRecordRef,
        catalog::ContextId,
        reference::{RecordTypeId, RecordVersionRef},
    };
    use erpx_kernel::RecordId;

    use super::*;

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_786_000_000, 0).unwrap()
    }
    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn policy() -> ApprovalPolicyVersion {
        ApprovalPolicyVersion {
            policy_id: Uuid::from_u128(10),
            version: 3,
            scope: scope(),
            status: ApprovalPolicyStatus::Active,
            gates: vec![ApprovalGatePolicy {
                gate_id: "finance-review".into(),
                decision_rule: GateDecisionRule::Any,
                required_approvals: 1,
                required_role: "finance-approver".into(),
                required_evidence: vec!["invoice-v7".into()],
                deadline_after_seconds: 3_600,
                escalation_after_seconds: Some(1_800),
                requires_maker_checker: true,
                requires_fresh_mfa: true,
            }],
            effective_at: Some(at()),
            evidence_sha256: format!("sha256:{}", "a".repeat(64)),
        }
    }
    fn source() -> RecordVersionRef {
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        RecordVersionRef::try_new(record, 7).unwrap()
    }
    fn authority(principal: &str) -> ApprovalAuthorityEvidence {
        ApprovalAuthorityEvidence {
            principal_ref: principal.into(),
            role: "finance-approver".into(),
            scope: scope(),
            assignment_version: 4,
            valid_until: at() + Duration::hours(2),
            approval_limit_minor: 10_000,
            fresh_mfa_at: Some(at()),
            evidence_sha256: format!("sha256:{}", "b".repeat(64)),
        }
    }

    #[test]
    fn approval_pins_policy_and_requires_fresh_independent_authority() {
        let mut session = ApprovalSession::start(
            Uuid::from_u128(20),
            source(),
            &policy(),
            "maker".into(),
            5_000,
            at(),
        )
        .unwrap();
        assert_eq!(
            session.decide(
                1,
                "finance-review",
                ApprovalOutcome::Approve,
                "self".into(),
                vec!["invoice-v7".into()],
                authority("maker"),
                None,
                at() + Duration::minutes(1),
            ),
            Err(ApprovalError::AuthorityDenied)
        );
        let command = session
            .decide(
                1,
                "finance-review",
                ApprovalOutcome::Approve,
                "within limit".into(),
                vec!["invoice-v7".into()],
                authority("checker"),
                None,
                at() + Duration::minutes(1),
            )
            .unwrap()
            .unwrap();
        assert_eq!(session.status, ApprovalSessionStatus::Approved);
        assert_eq!(command.source.version(), 7);
        assert!(command.request_sha256.starts_with("sha256:"));
    }

    #[test]
    fn stale_version_and_expired_delegation_fail_closed() {
        let mut session = ApprovalSession::start(
            Uuid::from_u128(21),
            source(),
            &policy(),
            "maker".into(),
            5_000,
            at(),
        )
        .unwrap();
        assert!(matches!(
            session.decide(
                99,
                "finance-review",
                ApprovalOutcome::Approve,
                "stale".into(),
                vec!["invoice-v7".into()],
                authority("checker"),
                None,
                at(),
            ),
            Err(ApprovalError::VersionConflict { .. })
        ));
        let delegation = ApprovalDelegation {
            delegator_ref: "checker".into(),
            delegate_ref: "delegate".into(),
            scope: scope(),
            source_assignment_version: 4,
            delegator_assignment_valid_until: at() + Duration::hours(1),
            valid_from: at() - Duration::hours(2),
            valid_until: at() - Duration::seconds(1),
            reason: "leave coverage".into(),
        };
        assert_eq!(
            delegation.validate_against(&authority("checker"), at()),
            Err(ApprovalError::AuthorityDenied)
        );
    }
}
