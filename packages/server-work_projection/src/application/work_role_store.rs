use crate::domain::work_role_store::{InitialWorkRole, WorkRoleInitialization, WorkRoleReference};
/// Caller provides the complete current account grant list and its requested
/// workspace subset from Identity. A picker-provided list cannot be used here.
/// The saved version must match; there is no implicit switch between grants.
pub fn initial_work_role(
    purpose: WorkRoleInitialization,
    has_current_row: bool,
    preference: Option<&WorkRoleReference>,
    assignments: &[WorkRoleReference],
    eligible: &[WorkRoleReference],
) -> InitialWorkRole {
    if has_current_row {
        return InitialWorkRole::PreserveCurrent;
    }
    if let Some(preference) = preference {
        if let Some(assignment) = assignments.iter().find(|a| *a == preference) {
            return InitialWorkRole::Select {
                assignment: assignment.clone(),
                restoring: true,
            };
        }
    }
    if purpose == WorkRoleInitialization::InitializeWorkspace
        && assignments.len() == 1
        && eligible.len() == 1
        && assignments[0] == eligible[0]
    {
        return InitialWorkRole::Select {
            assignment: eligible[0].clone(),
            restoring: false,
        };
    }
    InitialWorkRole::Unselected
}
