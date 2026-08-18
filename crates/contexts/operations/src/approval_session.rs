//! Version-pinned, reusable human approval sessions.
//!
//! Approval decisions are Operations-owned evidence. They do not mutate the
//! subject directly; the subject owner consumes the typed decision through an
//! idempotent command and independently checks the pinned aggregate version.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{BoundedContext, RecordRef};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApprovalSessionId(Uuid);

impl ApprovalSessionId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, ApprovalSessionError> {
        if value.is_nil() {
            return Err(ApprovalSessionError::InvalidIdentifier);
        }
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalGateKind {
    SalesCommercial,
    FinanceAccountingPlan,
    FinancePosting,
}

impl ApprovalGateKind {
    pub const fn owner(self) -> BoundedContext {
        match self {
            Self::SalesCommercial => BoundedContext::Sales,
            Self::FinanceAccountingPlan | Self::FinancePosting => BoundedContext::Finance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalGateStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalGateSpec {
    pub kind: ApprovalGateKind,
    pub assigned_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalGate {
    kind: ApprovalGateKind,
    assigned_to: Option<String>,
    status: ApprovalGateStatus,
    decided_by: Option<String>,
    reason_code: Option<String>,
    decided_at: Option<DateTime<Utc>>,
}

impl ApprovalGate {
    pub const fn kind(&self) -> ApprovalGateKind {
        self.kind
    }
    pub fn assigned_to(&self) -> Option<&str> {
        self.assigned_to.as_deref()
    }
    pub const fn status(&self) -> ApprovalGateStatus {
        self.status
    }
    pub fn decided_by(&self) -> Option<&str> {
        self.decided_by.as_deref()
    }
    pub fn reason_code(&self) -> Option<&str> {
        self.reason_code.as_deref()
    }
    pub const fn decided_at(&self) -> Option<DateTime<Utc>> {
        self.decided_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalSessionStatus {
    Pending,
    Approved,
    Rejected,
    Superseded,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalSession {
    id: ApprovalSessionId,
    version: u64,
    scope: TenantScope,
    subject: RecordRef,
    policy_version: u32,
    requested_by: String,
    gates: BTreeMap<ApprovalGateKind, ApprovalGate>,
    status: ApprovalSessionStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ApprovalSession {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: ApprovalSessionId,
        scope: TenantScope,
        subject: RecordRef,
        policy_version: u32,
        requested_by: String,
        gate_specs: Vec<ApprovalGateSpec>,
        at: DateTime<Utc>,
    ) -> Result<Self, ApprovalSessionError> {
        scope
            .validate()
            .map_err(|_| ApprovalSessionError::InvalidScope)?;
        subject
            .validate()
            .map_err(|_| ApprovalSessionError::InvalidSubject)?;
        if !matches!(
            subject.context,
            BoundedContext::Sales | BoundedContext::Finance
        ) || policy_version == 0
            || !valid_text(&requested_by, 256)
            || gate_specs.is_empty()
            || gate_specs.len() > 3
        {
            return Err(ApprovalSessionError::InvalidSession);
        }
        let mut gates = BTreeMap::new();
        for spec in gate_specs {
            if spec
                .assigned_to
                .as_deref()
                .is_some_and(|value| !valid_text(value, 256))
                || gates.contains_key(&spec.kind)
            {
                return Err(ApprovalSessionError::InvalidGate);
            }
            gates.insert(
                spec.kind,
                ApprovalGate {
                    kind: spec.kind,
                    assigned_to: spec.assigned_to,
                    status: ApprovalGateStatus::Pending,
                    decided_by: None,
                    reason_code: None,
                    decided_at: None,
                },
            );
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            subject,
            policy_version,
            requested_by,
            gates,
            status: ApprovalSessionStatus::Pending,
            created_at: at,
            updated_at: at,
        })
    }

    pub fn decide(
        &mut self,
        kind: ApprovalGateKind,
        subject_version: u64,
        decision: ApprovalGateStatus,
        decided_by: String,
        reason_code: String,
        at: DateTime<Utc>,
    ) -> Result<(), ApprovalSessionError> {
        if self.status != ApprovalSessionStatus::Pending
            || subject_version != self.subject.aggregate_version
            || !matches!(
                decision,
                ApprovalGateStatus::Approved | ApprovalGateStatus::Rejected
            )
            || !valid_text(&decided_by, 256)
            || !valid_text(&reason_code, 128)
        {
            return Err(ApprovalSessionError::InvalidDecision);
        }
        if decided_by == self.requested_by {
            return Err(ApprovalSessionError::MakerCheckerViolation);
        }
        let gate = self
            .gates
            .get(&kind)
            .ok_or(ApprovalSessionError::InvalidGate)?;
        if gate.status != ApprovalGateStatus::Pending
            || gate
                .assigned_to
                .as_deref()
                .is_some_and(|assigned| assigned != decided_by)
        {
            return Err(ApprovalSessionError::InvalidDecision);
        }
        let opposite_reviewer = match kind.owner() {
            BoundedContext::Sales => self.finance_reviewer(),
            BoundedContext::Finance => self.sales_reviewer(),
            _ => None,
        };
        if opposite_reviewer.is_some_and(|reviewer| reviewer == decided_by) {
            return Err(ApprovalSessionError::ReviewerSeparationViolation);
        }
        self.advance(at)?;
        let gate = self
            .gates
            .get_mut(&kind)
            .expect("approval gate was checked before session advance");
        gate.status = decision;
        gate.decided_by = Some(decided_by);
        gate.reason_code = Some(reason_code);
        gate.decided_at = Some(at);
        self.status = if decision == ApprovalGateStatus::Rejected {
            ApprovalSessionStatus::Rejected
        } else if self
            .gates
            .values()
            .all(|gate| gate.status == ApprovalGateStatus::Approved)
        {
            ApprovalSessionStatus::Approved
        } else {
            ApprovalSessionStatus::Pending
        };
        Ok(())
    }

    pub fn cancel(
        &mut self,
        requested_by: &str,
        at: DateTime<Utc>,
    ) -> Result<(), ApprovalSessionError> {
        if self.status != ApprovalSessionStatus::Pending || requested_by != self.requested_by {
            return Err(ApprovalSessionError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = ApprovalSessionStatus::Cancelled;
        for gate in self.gates.values_mut() {
            if gate.status == ApprovalGateStatus::Pending {
                gate.status = ApprovalGateStatus::Cancelled;
            }
        }
        Ok(())
    }

    /// Supersedes the pinned approval evidence when its owner reports a newer
    /// source version. Returning `false` means the session remains current.
    pub fn supersede_if_source_changed(
        &mut self,
        current_subject_version: u64,
        at: DateTime<Utc>,
    ) -> Result<bool, ApprovalSessionError> {
        if current_subject_version == self.subject.aggregate_version {
            return Ok(false);
        }
        if current_subject_version < self.subject.aggregate_version
            || matches!(
                self.status,
                ApprovalSessionStatus::Superseded | ApprovalSessionStatus::Cancelled
            )
        {
            return Err(ApprovalSessionError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = ApprovalSessionStatus::Superseded;
        for gate in self.gates.values_mut() {
            if gate.status == ApprovalGateStatus::Pending {
                gate.status = ApprovalGateStatus::Cancelled;
            }
        }
        Ok(true)
    }

    pub fn is_work_for(&self, principal_ref: &str) -> bool {
        self.status == ApprovalSessionStatus::Pending
            && self.gates.values().any(|gate| {
                gate.status == ApprovalGateStatus::Pending
                    && gate
                        .assigned_to
                        .as_deref()
                        .is_none_or(|assigned| assigned == principal_ref)
            })
    }

    fn sales_reviewer(&self) -> Option<&str> {
        self.gates
            .get(&ApprovalGateKind::SalesCommercial)
            .and_then(ApprovalGate::decided_by)
    }

    fn finance_reviewer(&self) -> Option<&str> {
        self.gates
            .iter()
            .find(|(kind, gate)| {
                kind.owner() == BoundedContext::Finance
                    && gate.status == ApprovalGateStatus::Approved
            })
            .and_then(|(_, gate)| gate.decided_by())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), ApprovalSessionError> {
        if at < self.updated_at {
            return Err(ApprovalSessionError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(ApprovalSessionError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> ApprovalSessionId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn subject(&self) -> &RecordRef {
        &self.subject
    }
    pub const fn policy_version(&self) -> u32 {
        self.policy_version
    }
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }
    pub const fn gates(&self) -> &BTreeMap<ApprovalGateKind, ApprovalGate> {
        &self.gates
    }
    pub const fn status(&self) -> ApprovalSessionStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

pub fn my_approval_work<'a>(
    sessions: impl IntoIterator<Item = &'a ApprovalSession>,
    principal_ref: &str,
) -> Vec<&'a ApprovalSession> {
    sessions
        .into_iter()
        .filter(|session| session.is_work_for(principal_ref))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApprovalSessionError {
    #[error("approval session identifier is invalid")]
    InvalidIdentifier,
    #[error("approval session scope is invalid")]
    InvalidScope,
    #[error("approval session subject is invalid")]
    InvalidSubject,
    #[error("approval session is invalid")]
    InvalidSession,
    #[error("approval gate is invalid")]
    InvalidGate,
    #[error("approval decision is invalid or stale")]
    InvalidDecision,
    #[error("maker-checker separation was violated")]
    MakerCheckerViolation,
    #[error("Sales and Finance reviewers must be different people")]
    ReviewerSeparationViolation,
    #[error("approval session transition is invalid")]
    InvalidTransition,
    #[error("approval session timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("approval session version overflowed")]
    VersionOverflow,
}

fn valid_text(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> ApprovalSession {
        ApprovalSession::create(
            ApprovalSessionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            TenantScope::Holding {
                holding_id: Uuid::from_u128(2),
            },
            RecordRef::try_new(BoundedContext::Sales, "quote", Uuid::from_u128(3), 4).unwrap(),
            2,
            "sales-maker".into(),
            vec![
                ApprovalGateSpec {
                    kind: ApprovalGateKind::SalesCommercial,
                    assigned_to: Some("sales-approver".into()),
                },
                ApprovalGateSpec {
                    kind: ApprovalGateKind::FinanceAccountingPlan,
                    assigned_to: Some("finance-approver".into()),
                },
            ],
            DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn parallel_quote_gates_enforce_maker_checker_and_cross_function_separation() {
        let mut value = session();
        let at = value.created_at();
        assert_eq!(
            value.decide(
                ApprovalGateKind::SalesCommercial,
                4,
                ApprovalGateStatus::Approved,
                "sales-maker".into(),
                "ok".into(),
                at,
            ),
            Err(ApprovalSessionError::MakerCheckerViolation)
        );
        value
            .decide(
                ApprovalGateKind::SalesCommercial,
                4,
                ApprovalGateStatus::Approved,
                "sales-approver".into(),
                "commercial-ok".into(),
                at,
            )
            .unwrap();
        assert_eq!(value.status(), ApprovalSessionStatus::Pending);
        assert_eq!(
            value.decide(
                ApprovalGateKind::FinanceAccountingPlan,
                4,
                ApprovalGateStatus::Approved,
                "sales-approver".into(),
                "finance-ok".into(),
                at,
            ),
            Err(ApprovalSessionError::InvalidDecision)
        );
        value
            .decide(
                ApprovalGateKind::FinanceAccountingPlan,
                4,
                ApprovalGateStatus::Approved,
                "finance-approver".into(),
                "finance-ok".into(),
                at,
            )
            .unwrap();
        assert_eq!(value.status(), ApprovalSessionStatus::Approved);
    }

    #[test]
    fn stale_decision_fails_and_edit_supersedes_the_whole_session() {
        let mut value = session();
        let at = value.created_at();
        assert_eq!(
            value.decide(
                ApprovalGateKind::SalesCommercial,
                5,
                ApprovalGateStatus::Approved,
                "sales-approver".into(),
                "ok".into(),
                at,
            ),
            Err(ApprovalSessionError::InvalidDecision)
        );
        assert!(value.supersede_if_source_changed(5, at).unwrap());
        assert_eq!(value.status(), ApprovalSessionStatus::Superseded);
    }

    #[test]
    fn my_work_queue_returns_only_current_assigned_sessions() {
        let value = session();
        assert_eq!(my_approval_work([&value], "sales-approver").len(), 1);
        assert!(my_approval_work([&value], "other").is_empty());
    }
}
