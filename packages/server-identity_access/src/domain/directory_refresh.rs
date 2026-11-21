//! Preflight can deny or schedule an observation; it never grants permission.
use crate::domain::work_context::{
    ActiveSelectionFact, ExpectedWorkSelection, HoldingWorkspaceRequest, PublishedRoleFact,
    WorkAssignmentFact, WorkScopeKind, WorkspaceLineageFact, assignment_applies, context_error,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId, UserId};

pub struct DirectoryRefreshSelectionFacts {
    pub revoked: bool,
    pub workspace: WorkspaceLineageFact,
    pub selection: ActiveSelectionFact,
    pub assignment: WorkAssignmentFact,
    pub role: PublishedRoleFact,
}
/// These are only the actual Company membership/provider link facts needed to
/// distinguish stale observation data from explicit administrative denial.
pub struct DirectoryRefreshMemberFacts {
    pub active: bool,
    pub version: i64,
    pub connection_active: bool,
    pub link_matches: bool,
    pub identity_enabled: bool,
    pub observed_at: i64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DirectoryRefreshDisposition {
    Current,
    Observe,
}
pub fn classify_member(
    member: &DirectoryRefreshMemberFacts,
    now: i64,
) -> Result<DirectoryRefreshDisposition, BusinessError> {
    classify_member_with_source(member, false, now)
}
pub fn classify_member_with_source(
    member: &DirectoryRefreshMemberFacts,
    source_refresh_required: bool,
    now: i64,
) -> Result<DirectoryRefreshDisposition, BusinessError> {
    if !member.active
        || member.version < 1
        || !member.connection_active
        || !member.link_matches
        || member.observed_at > now
    {
        return Err(directory_denied());
    }
    let deadline = member
        .observed_at
        .checked_add(300000)
        .ok_or_else(directory_denied)?;
    if source_refresh_required || deadline <= now {
        // B0 permits a complete new observation to discover re-enabled users;
        // an old disabled observation is not the same as an explicit tombstone.
        return Ok(DirectoryRefreshDisposition::Observe);
    }
    if !member.identity_enabled {
        return Err(directory_denied());
    }
    Ok(DirectoryRefreshDisposition::Current)
}
pub fn selection_fence(
    input: &HoldingWorkspaceRequest,
    session_id: &RecordId,
    user_id: &UserId,
    session_expires: i64,
    facts: &DirectoryRefreshSelectionFacts,
    now: i64,
) -> Result<ExpectedWorkSelection, BusinessError> {
    if session_expires <= now {
        return Err(BusinessError::new(
            FailureKind::Unauthenticated,
            "SESSION_EXPIRED",
            "Sign in again.",
        ));
    }
    if facts.revoked {
        return Err(BusinessError::new(
            FailureKind::Unauthenticated,
            "SESSION_REVOKED",
            "Sign in again to access this company.",
        ));
    }
    let s = &facts.selection;
    let a = &facts.assignment;
    let w = &facts.workspace;
    let role = &facts.role;
    if w.company_id != input.company_id
        || w.branch_slug != input.branch_slug
        || w.department_slug != input.department_slug
    {
        return Err(context_error("WORK_CONTEXT_CHANGED"));
    }
    if s.session_id != *session_id
        || s.user_id != *user_id
        || s.company_id != input.company_id
        || s.assignment_id.as_ref() != Some(&a.id)
        || s.assignment_version != Some(a.version)
        || s.expires_at <= now
        || s.revision < 0
        || a.user_id != *user_id
        || a.company_id != input.company_id
        || a.version <= 0
        || a.revoked
        || a.valid_from > now
        || a.valid_until <= now
        || !assignment_applies(a, w)
        || (a.scope_kind == WorkScopeKind::Holding && a.scope_node_id.is_some())
    {
        return Err(context_error("WORK_ROLE_UNAVAILABLE"));
    }
    if !role.active
        || role.id != a.role
        || role.revision <= 0
        || (role.company_id != "*" && role.company_id != input.company_id.as_str())
        || !role.scopes.contains(&a.scope_kind)
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ROLE_UNAVAILABLE",
            "This role is unavailable.",
        ));
    }
    if input.mutation && input.expected.is_none() {
        return Err(context_error("WORK_CONTEXT_CHANGED"));
    }
    if input.expected.as_ref().is_some_and(|expected| {
        expected.revision != s.revision
            || expected.assignment_id != a.id
            || expected.assignment_version != a.version
    }) {
        return Err(context_error("WORK_CONTEXT_CHANGED"));
    }
    // A server-captured CAS also constrains reads whose original transport had
    // no CAS. It cannot choose another grant or substitute an Actor.
    Ok(ExpectedWorkSelection {
        revision: s.revision,
        assignment_id: a.id.clone(),
        assignment_version: a.version,
    })
}
fn directory_denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "DIRECTORY_ACCESS_UNAVAILABLE",
        "Directory verification failed. Dependent access is unavailable until a successful refresh.",
    )
}
