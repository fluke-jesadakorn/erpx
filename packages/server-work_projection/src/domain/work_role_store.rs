//! Selection intent is not a grant. Identity must validate current physical
//! assignments and their scope before changing a session's selected authority.
use erpx_server_shared::public::AssignmentId;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkRoleReference {
    pub id: AssignmentId,
    pub version: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitialWorkRole {
    /// Preserve every existing row, including an unavailable/null selection. A
    /// preference from another device must never rebase an open session.
    PreserveCurrent,
    Unselected,
    Select {
        assignment: WorkRoleReference,
        restoring: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkRoleInitialization {
    RestoreSaved,
    InitializeWorkspace,
}
