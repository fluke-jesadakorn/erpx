//! Version-pinned, reusable human approval sessions.
//!
//! Approval decisions are Operations-owned evidence. They do not mutate the
//! subject directly; the subject owner consumes the typed decision through an
//! idempotent command and independently checks the pinned aggregate version.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{BoundedContext, RecordRef};
use erpx_kernel::{ActorKind, AuthorityScope, HumanDecisionProof, SessionId};
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

pub fn approval_session_decision_ref(
    command_id: Uuid,
    session_id: ApprovalSessionId,
    subject_version: u64,
    gate: ApprovalGateKind,
    outcome: ApprovalGateStatus,
) -> String {
    let gate = match gate {
        ApprovalGateKind::SalesCommercial => "sales-commercial",
        ApprovalGateKind::FinanceAccountingPlan => "finance-accounting-plan",
        ApprovalGateKind::FinancePosting => "finance-posting",
    };
    let action = match outcome {
        ApprovalGateStatus::Pending => "pending",
        ApprovalGateStatus::Approved => "approved",
        ApprovalGateStatus::Rejected => "rejected",
        ApprovalGateStatus::Cancelled => "cancelled",
    };
    format!(
        "operations.approval-session:command:{command_id}:target:{}:subject-version:{subject_version}:gate:{gate}:action:{action}",
        session_id.as_uuid()
    )
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
    #[serde(default)]
    decision_actor_kind: Option<ActorKind>,
    #[serde(default)]
    human_decision_ref: Option<String>,
    #[serde(default)]
    decision_session_id: Option<SessionId>,
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
    pub const fn decision_actor_kind(&self) -> Option<ActorKind> {
        self.decision_actor_kind
    }
    pub fn human_decision_ref(&self) -> Option<&str> {
        self.human_decision_ref.as_deref()
    }
    pub const fn decision_session_id(&self) -> Option<SessionId> {
        self.decision_session_id
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
    scope: AuthorityScope,
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
        scope: AuthorityScope,
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
                    decision_actor_kind: None,
                    human_decision_ref: None,
                    decision_session_id: None,
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
        command_id: Uuid,
        kind: ApprovalGateKind,
        subject_version: u64,
        outcome: ApprovalGateStatus,
        human_decision: HumanDecisionProof,
        reason_code: String,
    ) -> Result<(), ApprovalSessionError> {
        let decided_by = human_decision.principal_ref().to_owned();
        let at = human_decision.decided_at();
        let decision_actor_kind = human_decision.actor().kind;
        let human_decision_ref = human_decision.decision_ref().to_owned();
        let decision_session_id = human_decision.actor().session_id;
        if self.status != ApprovalSessionStatus::Pending
            || command_id.is_nil()
            || subject_version != self.subject.aggregate_version
            || !matches!(
                outcome,
                ApprovalGateStatus::Approved | ApprovalGateStatus::Rejected
            )
            || human_decision.authority_scope() != &self.scope
            || human_decision.decision_ref()
                != approval_session_decision_ref(
                    command_id,
                    self.id,
                    subject_version,
                    kind,
                    outcome,
                )
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
        gate.status = outcome;
        gate.decided_by = Some(decided_by);
        gate.decision_actor_kind = Some(decision_actor_kind);
        gate.human_decision_ref = Some(human_decision_ref);
        gate.decision_session_id = decision_session_id;
        gate.reason_code = Some(reason_code);
        gate.decided_at = Some(at);
        self.status = if outcome == ApprovalGateStatus::Rejected {
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
    pub const fn scope(&self) -> &AuthorityScope {
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
    use erpx_kernel::{ActorAttribution, ActorKind, SessionId};

    use super::*;

    fn human_decision(principal_ref: &str, reference: &str) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: principal_ref.into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(
                    SessionId::try_from_uuid(Uuid::from_u128(9)).expect("non-nil session"),
                ),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Holding {
                holding_id: Uuid::from_u128(2),
            },
            reference,
            DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
        )
        .unwrap()
    }

    fn command_id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn bound_human_decision(
        principal_ref: &str,
        command_id: Uuid,
        subject_version: u64,
        gate: ApprovalGateKind,
        outcome: ApprovalGateStatus,
    ) -> HumanDecisionProof {
        human_decision(
            principal_ref,
            &approval_session_decision_ref(
                command_id,
                ApprovalSessionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
                subject_version,
                gate,
                outcome,
            ),
        )
    }

    fn session() -> ApprovalSession {
        ApprovalSession::create(
            ApprovalSessionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            AuthorityScope::Holding {
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
        assert_eq!(
            value.decide(
                command_id(20),
                ApprovalGateKind::SalesCommercial,
                4,
                ApprovalGateStatus::Approved,
                bound_human_decision(
                    "sales-maker",
                    command_id(20),
                    4,
                    ApprovalGateKind::SalesCommercial,
                    ApprovalGateStatus::Approved,
                ),
                "ok".into(),
            ),
            Err(ApprovalSessionError::MakerCheckerViolation)
        );
        value
            .decide(
                command_id(21),
                ApprovalGateKind::SalesCommercial,
                4,
                ApprovalGateStatus::Approved,
                bound_human_decision(
                    "sales-approver",
                    command_id(21),
                    4,
                    ApprovalGateKind::SalesCommercial,
                    ApprovalGateStatus::Approved,
                ),
                "commercial-ok".into(),
            )
            .unwrap();
        assert_eq!(value.status(), ApprovalSessionStatus::Pending);
        assert_eq!(
            value.decide(
                command_id(22),
                ApprovalGateKind::FinanceAccountingPlan,
                4,
                ApprovalGateStatus::Approved,
                bound_human_decision(
                    "sales-approver",
                    command_id(22),
                    4,
                    ApprovalGateKind::FinanceAccountingPlan,
                    ApprovalGateStatus::Approved,
                ),
                "finance-ok".into(),
            ),
            Err(ApprovalSessionError::InvalidDecision)
        );
        value
            .decide(
                command_id(23),
                ApprovalGateKind::FinanceAccountingPlan,
                4,
                ApprovalGateStatus::Approved,
                bound_human_decision(
                    "finance-approver",
                    command_id(23),
                    4,
                    ApprovalGateKind::FinanceAccountingPlan,
                    ApprovalGateStatus::Approved,
                ),
                "finance-ok".into(),
            )
            .unwrap();
        assert_eq!(value.status(), ApprovalSessionStatus::Approved);
        let finance_gate = value
            .gates()
            .get(&ApprovalGateKind::FinanceAccountingPlan)
            .unwrap();
        assert_eq!(
            finance_gate.decision_actor_kind(),
            Some(ActorKind::HoldingUser)
        );
        assert_eq!(
            finance_gate.human_decision_ref(),
            Some(
                approval_session_decision_ref(
                    command_id(23),
                    value.id(),
                    4,
                    ApprovalGateKind::FinanceAccountingPlan,
                    ApprovalGateStatus::Approved,
                )
                .as_str()
            )
        );
        assert!(finance_gate.decision_session_id().is_some());
    }

    #[test]
    fn stale_decision_fails_and_edit_supersedes_the_whole_session() {
        let mut value = session();
        let at = value.created_at();
        assert_eq!(
            value.decide(
                command_id(24),
                ApprovalGateKind::SalesCommercial,
                5,
                ApprovalGateStatus::Approved,
                bound_human_decision(
                    "sales-approver",
                    command_id(24),
                    5,
                    ApprovalGateKind::SalesCommercial,
                    ApprovalGateStatus::Approved,
                ),
                "ok".into(),
            ),
            Err(ApprovalSessionError::InvalidDecision)
        );
        assert!(value.supersede_if_source_changed(5, at).unwrap());
        assert_eq!(value.status(), ApprovalSessionStatus::Superseded);
    }

    #[test]
    fn decision_reference_cannot_be_wrong_or_reused_for_another_gate() {
        let mut value = session();
        let command_id = command_id(25);
        let reused = bound_human_decision(
            "sales-approver",
            command_id,
            4,
            ApprovalGateKind::FinanceAccountingPlan,
            ApprovalGateStatus::Approved,
        );
        assert_eq!(
            value.decide(
                command_id,
                ApprovalGateKind::SalesCommercial,
                4,
                ApprovalGateStatus::Approved,
                reused,
                "commercial-ok".into(),
            ),
            Err(ApprovalSessionError::InvalidDecision)
        );
        value
            .decide(
                command_id,
                ApprovalGateKind::SalesCommercial,
                4,
                ApprovalGateStatus::Approved,
                bound_human_decision(
                    "sales-approver",
                    command_id,
                    4,
                    ApprovalGateKind::SalesCommercial,
                    ApprovalGateStatus::Approved,
                ),
                "commercial-ok".into(),
            )
            .unwrap();
    }

    #[test]
    fn my_work_queue_returns_only_current_assigned_sessions() {
        let value = session();
        assert_eq!(my_approval_work([&value], "sales-approver").len(), 1);
        assert!(my_approval_work([&value], "other").is_empty());
    }
}
