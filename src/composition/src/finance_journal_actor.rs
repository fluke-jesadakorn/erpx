//! Pure projection of an already authorized selected Finance context.
//! This constructs wire metadata, never an independent authorization grant.
use erpx_server_finance::public::FinanceActor;
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam, effective_authority::AuthorizedWorkContext,
    work_context::WorkScopeKind,
};
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkAuthority,
};
pub(crate) fn finance_actor(current: &AuthorizedWorkContext) -> FinanceActor {
    FinanceActor {
        holding_id: current.holding_id().clone(),
        company_id: current.selection().workspace().company_id.clone(),
        user_id: current.user_id().clone(),
        assignment_id: current.selection().assignment().id.clone(),
    }
}
/// Shared Workflow wire projection, not an independently constructed grant.
pub(crate) fn command_actor(
    current: &AuthorizedWorkContext,
) -> Result<CommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: assignment.id.clone(),
        role: assignment.role.clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: selected.revision(),
            assignment_version: assignment.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|id| id.as_str().into()),
    })
}
