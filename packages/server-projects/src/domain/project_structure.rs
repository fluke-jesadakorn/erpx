//! Native Projects work-structure and handoff command facts.
//!
//! The owner command covers task-to-package binding, approved actual binding
//! and the independent handoff lifecycle. Every command carries the exact
//! project version it was prepared against; the application re-checks the
//! current owner, source versions and lifecycle before the adapter writes the
//! binding, project version, receipt, audit and outbox in one serializable
//! unit. Finance stays the posting owner: a cost actual is only linked from an
//! already approved allocation whose journal is posted.
use crate::domain::project_commands::{
    ProjectCommandActor, ProjectCommandProject, authority_json, js_trim,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};

const REASON_MIN: usize = 3;
const REASON_LIMIT: usize = 2_000;
const EVIDENCE_MIN: usize = 3;
const EVIDENCE_LIMIT: usize = 10_000;

#[derive(Clone, Debug)]
pub struct ProjectStructureActor {
    pub command: ProjectCommandActor,
    /// Current Commercial priced cost-history capability. Composition supplies
    /// it after its own same-transaction admission.
    pub cost_history: bool,
}

impl ProjectStructureActor {
    fn has(&self, permission: &str) -> bool {
        self.command
            .permissions
            .iter()
            .any(|value| value == permission)
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.command.require_write(now)
    }

    pub fn has_finance_write(&self) -> bool {
        self.has("finance.write")
    }

    /// Cost visibility mirrors the retained Projects read policy: the priced
    /// cost-history capability, current `finance.read` and the selected
    /// branch/department scope must all hold.
    pub fn cost_access(&self, project: &ProjectCommandProject) -> bool {
        self.cost_history
            && self.has("finance.read")
            && self
                .command
                .branch_id
                .as_ref()
                .is_none_or(|branch| project.branch_id.as_deref() == Some(branch.as_str()))
            && self
                .command
                .department_id
                .as_ref()
                .is_none_or(|department| {
                    project.department_id.as_deref() == Some(department.as_str())
                })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActualSourceKind {
    WorkLog,
    CostAllocation,
}

impl ActualSourceKind {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "work_log" => Ok(Self::WorkLog),
            "cost_allocation" => Ok(Self::CostAllocation),
            _ => Err(BusinessError::invalid(
                "PROJECT_ACTUAL_SOURCE_INVALID",
                "Choose an approved project time or cost source.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkLog => "work_log",
            Self::CostAllocation => "cost_allocation",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum ProjectStructureCommand {
    #[serde(rename_all = "camelCase")]
    AssignTask {
        expected_version: i64,
        task_id: String,
        task_version: i64,
        milestone_id: String,
        milestone_version: i64,
        #[serde(default)]
        binding_version: i64,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    UnassignTask {
        expected_version: i64,
        task_id: String,
        task_version: i64,
        binding_version: i64,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    LinkActual {
        expected_version: i64,
        milestone_id: String,
        milestone_version: i64,
        source_kind: String,
        source_id: String,
        source_version: i64,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    PrepareHandoff {
        expected_version: i64,
        recipient_id: String,
        evidence: String,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    AcceptHandoff {
        expected_version: i64,
        handoff_id: String,
        handoff_version: i64,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    RejectHandoff {
        expected_version: i64,
        handoff_id: String,
        handoff_version: i64,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    CancelHandoff {
        expected_version: i64,
        handoff_id: String,
        handoff_version: i64,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    Reopen {
        expected_version: i64,
        handoff_id: String,
        handoff_version: i64,
        reason: String,
    },
}

impl ProjectStructureCommand {
    pub fn action(&self) -> &'static str {
        match self {
            Self::AssignTask { .. } => "assign-task",
            Self::UnassignTask { .. } => "unassign-task",
            Self::LinkActual { .. } => "link-actual",
            Self::PrepareHandoff { .. } => "prepare-handoff",
            Self::AcceptHandoff { .. } => "accept-handoff",
            Self::RejectHandoff { .. } => "reject-handoff",
            Self::CancelHandoff { .. } => "cancel-handoff",
            Self::Reopen { .. } => "reopen",
        }
    }

    pub fn operation(&self) -> String {
        format!("project.structure.{}", self.action())
    }

    pub fn expected_version(&self) -> i64 {
        match self {
            Self::AssignTask {
                expected_version,
                ..
            }
            | Self::UnassignTask {
                expected_version,
                ..
            }
            | Self::LinkActual {
                expected_version,
                ..
            }
            | Self::PrepareHandoff {
                expected_version,
                ..
            }
            | Self::AcceptHandoff {
                expected_version,
                ..
            }
            | Self::RejectHandoff {
                expected_version,
                ..
            }
            | Self::CancelHandoff {
                expected_version,
                ..
            }
            | Self::Reopen {
                expected_version,
                ..
            } => *expected_version,
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Self::AssignTask { reason, .. }
            | Self::UnassignTask { reason, .. }
            | Self::LinkActual { reason, .. }
            | Self::PrepareHandoff { reason, .. }
            | Self::AcceptHandoff { reason, .. }
            | Self::RejectHandoff { reason, .. }
            | Self::CancelHandoff { reason, .. }
            | Self::Reopen { reason, .. } => reason,
        }
    }

    pub fn purpose_is_new_work(&self) -> bool {
        matches!(self, Self::AssignTask { .. } | Self::Reopen { .. })
    }

    /// Trim the retained Zod `trim()` fields with ECMAScript whitespace.
    pub fn normalize(mut self) -> Self {
        match &mut self {
            Self::AssignTask { reason, .. }
            | Self::UnassignTask { reason, .. }
            | Self::LinkActual { reason, .. }
            | Self::AcceptHandoff { reason, .. }
            | Self::RejectHandoff { reason, .. }
            | Self::CancelHandoff { reason, .. }
            | Self::Reopen { reason, .. } => {
                *reason = js_trim(reason).to_owned();
            }
            Self::PrepareHandoff {
                evidence, reason, ..
            } => {
                *evidence = js_trim(evidence).to_owned();
                *reason = js_trim(reason).to_owned();
            }
        }
        self
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.expected_version() < 1 || !valid_reason(self.reason()) {
            return Err(BusinessError::invalid(
                "PROJECT_STRUCTURE_INVALID",
                "Review the current project version and reason before changing its structure.",
            ));
        }
        match self {
            Self::AssignTask {
                task_id,
                task_version,
                milestone_id,
                milestone_version,
                binding_version,
                ..
            } => {
                if !valid_id(task_id)
                    || *task_version < 1
                    || !valid_id(milestone_id)
                    || *milestone_version < 1
                    || *binding_version < 0
                {
                    return Err(structure_invalid());
                }
            }
            Self::UnassignTask {
                task_id,
                task_version,
                binding_version,
                ..
            } => {
                if !valid_id(task_id)
                    || *task_version < 1
                    || *binding_version < 1
                {
                    return Err(structure_invalid());
                }
            }
            Self::LinkActual {
                milestone_id,
                milestone_version,
                source_kind,
                source_id,
                source_version,
                ..
            } => {
                ActualSourceKind::parse(source_kind)?;
                if !valid_id(milestone_id)
                    || *milestone_version < 1
                    || !valid_id(source_id)
                    || *source_version < 1
                {
                    return Err(structure_invalid());
                }
            }
            Self::PrepareHandoff {
                recipient_id,
                evidence,
                ..
            } => {
                if !valid_id(recipient_id) || !valid_evidence(evidence) {
                    return Err(BusinessError::invalid(
                        "PROJECT_HANDOFF_INVALID",
                        "Name an independent project member and retain review evidence.",
                    ));
                }
            }
            Self::AcceptHandoff {
                handoff_id,
                handoff_version,
                ..
            }
            | Self::RejectHandoff {
                handoff_id,
                handoff_version,
                ..
            }
            | Self::CancelHandoff {
                handoff_id,
                handoff_version,
                ..
            }
            | Self::Reopen {
                handoff_id,
                handoff_version,
                ..
            } => {
                if !valid_id(handoff_id) || *handoff_version < 1 {
                    return Err(BusinessError::invalid(
                        "PROJECT_HANDOFF_INVALID",
                        "Review the current project handoff before deciding it.",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectStructureReceipt {
    pub id: String,
    pub version: i64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_sha256: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StoredStructureReceipt {
    pub operation: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProjectStructureTask {
    pub id: String,
    pub version: i64,
    pub project_id: String,
}

#[derive(Clone, Debug)]
pub struct ProjectTaskBinding {
    pub milestone_id: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ProjectPackageFact {
    pub id: String,
    pub version: i64,
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct ApprovedTime {
    pub minutes: i64,
}

#[derive(Clone, Debug)]
pub struct ApprovedCost {
    pub amount_minor: i64,
    pub currency: String,
}

#[derive(Clone, Debug)]
pub struct ProjectHandoffFact {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub recipient_id: String,
    pub prepared_by: String,
    pub snapshot_sha256: String,
}

#[derive(Clone, Debug)]
pub enum StructurePlanAction {
    AssignTask {
        task_id: String,
        milestone_id: String,
        task_version: i64,
        milestone_version: i64,
        binding_version: i64,
    },
    UnassignTask {
        task_id: String,
        task_version: i64,
        binding_version: i64,
    },
    LinkActual {
        actual_id: String,
        milestone_id: String,
        milestone_version: i64,
        source_kind: ActualSourceKind,
        source_id: String,
        source_version: i64,
        minutes: Option<i64>,
        amount_minor: Option<i64>,
        currency: Option<String>,
    },
    PrepareHandoff {
        handoff_id: String,
        recipient_id: String,
        snapshot_json: String,
        snapshot_sha256: String,
        evidence: String,
    },
    Decision {
        handoff_id: String,
        handoff_version: i64,
        status: String,
    },
    Reopen {
        handoff_id: String,
        handoff_version: i64,
    },
}

#[derive(Clone, Debug)]
pub struct StructurePlan {
    pub key: CommandKey,
    pub operation: String,
    pub company_id: CompanyId,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub project_id: String,
    pub project_version: i64,
    pub required_lifecycle: String,
    pub next_version: i64,
    pub lifecycle: String,
    pub action: StructurePlanAction,
    pub event_action: String,
    pub reason: String,
    pub result: ProjectStructureReceipt,
    pub request_fingerprint: String,
    pub now: i64,
}

pub fn missing_project() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "PROJECT_NOT_FOUND", "Project not found.")
}

pub fn missing_handoff() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROJECT_HANDOFF_NOT_FOUND",
        "Project handoff not found.",
    )
}

pub fn owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_OWNER_REQUIRED",
        "The project owner manages work structure and handoff.",
    )
}

pub fn structure_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_STRUCTURE_FROZEN",
        "Reopen the project before changing work structure.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The project changed. Reload its current structure before continuing.",
    )
}

pub fn task_binding_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_TASK_BINDING_CHANGED",
        "Reload the task and its current work package.",
    )
}

pub fn package_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_PACKAGE_CHANGED",
        "Reload the current work package before continuing.",
    )
}

pub fn package_terminal() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_PACKAGE_TERMINAL",
        "Reopen the work package before changing its assigned work.",
    )
}

pub fn time_source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_TIME_SOURCE_CHANGED",
        "Choose the current independently approved project time.",
    )
}

pub fn cost_source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_COST_SOURCE_CHANGED",
        "Choose the current approved allocation from a posted Finance journal.",
    )
}

pub fn finance_permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_PERMISSION_REQUIRED",
        "The finance.write permission is required for project cost actuals.",
    )
}

pub fn independent_handoff_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INDEPENDENT_HANDOFF_REQUIRED",
        "A different project member must receive and review this handoff.",
    )
}

pub fn handoff_not_ready() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_HANDOFF_NOT_READY",
        "The project must be active before a new handoff.",
    )
}

pub fn handoff_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_HANDOFF_INCOMPLETE",
        "Close all work and reviews, and assign completed tasks and approved time/cost sources to work packages before handoff.",
    )
}

pub fn handoff_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_HANDOFF_CHANGED",
        "Reload the current handoff before reviewing it.",
    )
}

pub fn handoff_decided() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_HANDOFF_DECIDED",
        "This handoff already has a recorded decision.",
    )
}

pub fn recipient_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_RECIPIENT_REQUIRED",
        "The named independent receiving member must decide this handoff.",
    )
}

pub fn preparer_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_HANDOFF_PREPARER_REQUIRED",
        "The preparing owner cancels the pending handoff.",
    )
}

pub fn recipient_reopen_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_RECIPIENT_REOPEN_REQUIRED",
        "The independent receiving member must review reopening an accepted project.",
    )
}

pub fn recipient_not_member() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROJECT_HANDOFF_RECIPIENT_INVALID",
        "Choose a current project member as the independent recipient.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_KEY_CONFLICT",
        "This command key was already used for a different Project request.",
    )
}

pub fn incomplete_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_COMMAND_RECEIPT_INCOMPLETE",
        "The original Project command receipt is incomplete.",
    )
}

pub fn invalid_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_COMMAND_RECEIPT_INVALID",
        "The original Project command receipt is invalid.",
    )
}

pub fn snapshot_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_HANDOFF_SNAPSHOT_INVALID",
        "The retained handoff snapshot could not be produced.",
    )
}

/// SHA-256 of the retained snapshot bytes, rendered as lower-case hex. The
/// Projects crate intentionally keeps this tiny, dependency-free implementation
/// so the owner package does not need a new workspace dependency.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn sha256_hex(value: &str) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = value.as_bytes().to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}

pub fn authority(actor: &ProjectStructureActor) -> Result<String, BusinessError> {
    authority_json(&actor.command)
}

pub fn fingerprint(
    operation: &str,
    project_id: &str,
    input: &ProjectStructureCommand,
) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({
        "operation": operation,
        "projectId": project_id,
        "input": input,
    }))
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECT_COMMAND_INVALID",
            "The current Project command could not be retained.",
        )
    })
}

pub fn structure_fingerprint(
    input: &ProjectStructureCommand,
    project_id: &str,
) -> Result<String, BusinessError> {
    fingerprint(&input.operation(), project_id, input)
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn cp_len(value: &str) -> usize {
    value.chars().count()
}

fn valid_reason(value: &str) -> bool {
    let length = cp_len(value);
    (REASON_MIN..=REASON_LIMIT).contains(&length) && value == js_trim(value)
}

fn valid_evidence(value: &str) -> bool {
    let length = cp_len(value);
    (EVIDENCE_MIN..=EVIDENCE_LIMIT).contains(&length) && value == js_trim(value)
}

fn structure_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECT_STRUCTURE_INVALID",
        "Review the selected task, work package, source and versions.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assign_task_binding_version_defaults_to_zero() {
        let command: ProjectStructureCommand = serde_json::from_value(serde_json::json!({
            "action": "assign-task",
            "expectedVersion": 3,
            "taskId": "task",
            "taskVersion": 2,
            "milestoneId": "milestone",
            "milestoneVersion": 1,
            "reason": "Assign the current task."
        }))
        .unwrap();
        match &command {
            ProjectStructureCommand::AssignTask { binding_version, .. } => {
                assert_eq!(*binding_version, 0);
            }
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn whitespace_only_reason_is_rejected() {
        let command = ProjectStructureCommand::Reopen {
            expected_version: 1,
            handoff_id: "handoff".into(),
            handoff_version: 1,
            reason: "   ".into(),
        };
        assert_eq!(
            command.validate().unwrap_err().code,
            "PROJECT_STRUCTURE_INVALID"
        );
    }
}
