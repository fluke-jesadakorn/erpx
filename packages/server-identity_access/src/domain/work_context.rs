//! Current selected grant facts, not a permission-bearing actor or an owner authorization token.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkScopeKind {
    Holding,
    Branch,
    Department,
}
#[derive(Clone, Debug)]
pub struct ExpectedWorkSelection {
    pub revision: i64,
    pub assignment_id: AssignmentId,
    pub assignment_version: i64,
}
/// Adapter input only. Session facts must come from the retained Control verification lease.
#[derive(Clone, Debug)]
pub struct WorkContextRequest {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub session_id: RecordId,
    pub user_id: UserId,
    pub session_expires_at: i64,
    pub expected: Option<ExpectedWorkSelection>,
    pub mutation: bool,
}
#[derive(Clone, Debug)]
pub struct WorkspaceLineageFact {
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
}
#[derive(Clone, Debug)]
pub struct WorkAssignmentFact {
    pub id: AssignmentId,
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub role: String,
    pub modules: Vec<String>,
    pub scope_kind: WorkScopeKind,
    pub scope_node_id: Option<RecordId>,
    pub version: i64,
    pub valid_from: i64,
    pub valid_until: i64,
    pub revoked: bool,
}
#[derive(Clone, Debug)]
pub struct PublishedRoleFact {
    pub id: String,
    pub company_id: String,
    pub revision: i64,
    pub active: bool,
    pub scopes: Vec<WorkScopeKind>,
}
#[derive(Clone, Debug)]
pub struct ActiveSelectionFact {
    pub session_id: RecordId,
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub assignment_id: Option<AssignmentId>,
    pub assignment_version: Option<i64>,
    pub revision: i64,
    pub expires_at: i64,
}
#[derive(Clone, Debug)]
pub struct WorkContextSnapshot {
    pub workspace: Option<WorkspaceLineageFact>,
    pub selection: Option<ActiveSelectionFact>,
    pub assignment: Option<WorkAssignmentFact>,
    pub role: Option<PublishedRoleFact>,
    pub session_revoked: bool,
    pub directory_stale: bool,
    pub principal_active: bool,
    pub assignment_active: bool,
}
/// A caller still needs current effective IAM permission and the owner's record policy.
#[derive(Clone, Debug)]
pub struct CurrentWorkSelection {
    pub(crate) workspace: WorkspaceLineageFact,
    pub(crate) assignment: WorkAssignmentFact,
    pub(crate) revision: i64,
    pub(crate) role_revision: i64,
    pub(crate) expires_at: i64,
}
impl CurrentWorkSelection {
    pub fn workspace(&self) -> &WorkspaceLineageFact {
        &self.workspace
    }
    pub fn assignment(&self) -> &WorkAssignmentFact {
        &self.assignment
    }
    pub fn revision(&self) -> i64 {
        self.revision
    }
    pub fn role_revision(&self) -> i64 {
        self.role_revision
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
}
pub fn context_error(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        code,
        "Your selected work context changed. Review the current workspace and role.",
    )
}
pub fn assignment_applies(
    assignment: &WorkAssignmentFact,
    workspace: &WorkspaceLineageFact,
) -> bool {
    assignment.company_id == workspace.company_id
        && match assignment.scope_kind {
            WorkScopeKind::Holding => true,
            WorkScopeKind::Branch => {
                assignment.scope_node_id.is_some()
                    && assignment.scope_node_id == workspace.branch_id
            }
            WorkScopeKind::Department => {
                assignment.scope_node_id.is_some()
                    && assignment.scope_node_id == workspace.department_id
            }
        }
}

/// Host and session identity are supplied by trusted adapters, never this route input.
#[derive(Clone, Debug)]
pub struct HoldingWorkspaceRequest {
    pub company_id: CompanyId,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub expected: Option<ExpectedWorkSelection>,
    pub mutation: bool,
}
