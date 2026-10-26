use crate::domain::work_context::{
    CurrentWorkSelection, WorkContextRequest, WorkContextSnapshot, assignment_applies,
    context_error,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, HoldingId};
use std::{future::Future, pin::Pin};
pub type WorkContextFuture<'a, T, E = BusinessError> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;
pub trait CurrentWorkContextPort: Send {
    type Error: From<BusinessError> + Send;
    fn holding_id(&self) -> &HoldingId;
    fn snapshot<'a>(
        &'a mut self,
        request: &'a WorkContextRequest,
        now: i64,
    ) -> WorkContextFuture<'a, WorkContextSnapshot, Self::Error>;
}
/// No fallback assignment, role selection, external refresh or cached authority is used.
/// This proves the selected grant only; it does not produce an authorized business actor.
pub async fn inspect_current_work_selection<P: CurrentWorkContextPort + ?Sized>(
    port: &mut P,
    clock: &(dyn Clock + Sync),
    request: &WorkContextRequest,
) -> Result<CurrentWorkSelection, P::Error> {
    let now = clock.now().get();
    if request.session_expires_at <= now {
        return Err(BusinessError::new(
            FailureKind::Unauthenticated,
            "SESSION_EXPIRED",
            "Sign in again.",
        )
        .into());
    }
    if port.holding_id() != &request.holding_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "WORKSPACE_NOT_FOUND",
            "Workspace not found.",
        )
        .into());
    }
    let facts = port.snapshot(request, now).await?;
    validate_current_selection(request, facts, now).map_err(Into::into)
}
fn validate_current_selection(
    request: &WorkContextRequest,
    facts: WorkContextSnapshot,
    now: i64,
) -> Result<CurrentWorkSelection, BusinessError> {
    if facts.session_revoked {
        return Err(BusinessError::new(
            FailureKind::Unauthenticated,
            "SESSION_REVOKED",
            "Sign in again to access this company.",
        ));
    }
    if facts.directory_stale {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "DIRECTORY_ACCESS_UNAVAILABLE",
            "Your directory access needs a successful refresh.",
        ));
    }
    let workspace = facts.workspace.ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "WORKSPACE_NOT_FOUND",
            "Workspace not found.",
        )
    })?;
    if workspace.company_id != request.company_id
        || workspace.branch_slug != request.branch_slug
        || workspace.department_slug != request.department_slug
    {
        return Err(context_error("WORK_CONTEXT_CHANGED"));
    }
    let selection = facts
        .selection
        .ok_or_else(|| context_error("WORK_ROLE_REQUIRED"))?;
    let assignment = facts
        .assignment
        .ok_or_else(|| context_error("WORK_ROLE_UNAVAILABLE"))?;
    if selection.session_id != request.session_id
        || selection.user_id != request.user_id
        || selection.company_id != request.company_id
        || selection.assignment_id.as_ref() != Some(&assignment.id)
        || selection.assignment_version != Some(assignment.version)
        || selection.expires_at <= now
        || assignment.user_id != request.user_id
        || assignment.version <= 0
        || selection.revision < 0
        || assignment.revoked
        || assignment.valid_from > now
        || assignment.valid_until <= now
        || !facts.principal_active
        || !facts.assignment_active
        || !assignment_applies(&assignment, &workspace)
    {
        return Err(context_error("WORK_ROLE_UNAVAILABLE"));
    }
    if request.mutation && request.expected.is_none() {
        return Err(context_error("WORK_CONTEXT_CHANGED"));
    }
    if request.expected.as_ref().is_some_and(|expected| {
        expected.revision != selection.revision
            || expected.assignment_id != assignment.id
            || expected.assignment_version != assignment.version
    }) {
        return Err(context_error("WORK_CONTEXT_CHANGED"));
    }
    let role = facts.role.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Forbidden,
            "ROLE_UNAVAILABLE",
            "This role is unavailable.",
        )
    })?;
    if !role.active
        || role.id != assignment.role
        || role.revision <= 0
        || role.company_id != "*" && role.company_id != request.company_id.as_str()
        || !role.scopes.contains(&assignment.scope_kind)
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ROLE_UNAVAILABLE",
            "This role is unavailable.",
        ));
    }
    Ok(CurrentWorkSelection {
        workspace,
        assignment,
        revision: selection.revision,
        role_revision: role.revision,
    })
}
