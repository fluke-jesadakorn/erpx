//! Directory's standing reviewed delegation facts, not a serialized Actor.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub struct DirectoryRoleFact<'a> {
    pub revision: i64,
    pub permissions: &'a [String],
    pub grantable: &'a [String],
    pub scopes: &'a [String],
}
pub struct DirectoryParentFact<'a> {
    pub current: bool,
    pub company_matches: bool,
    pub scope: &'a str,
    pub node: Option<&'a str>,
    pub valid_until: i64,
    /// None means missing/unpublished; an actual published empty ceiling is Some(&[]).
    pub grantable: Option<&'a [String]>,
}
pub struct DirectoryMemberFact {
    pub active: bool,
    pub directory: bool,
}
pub struct DirectoryScopeFact<'a> {
    pub branch: Option<&'a str>,
    pub department: Option<&'a str>,
}
pub struct DirectoryGrantFacts<'a> {
    pub same_person: bool,
    pub parent: Option<DirectoryParentFact<'a>>,
    pub member: Option<DirectoryMemberFact>,
    pub role: Option<DirectoryRoleFact<'a>>,
    pub requested_scope: &'a str,
    pub requested_node: Option<&'a str>,
    pub requested_until: i64,
    pub scope: Option<DirectoryScopeFact<'a>>,
    pub platform: bool,
}
fn error(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}
fn scope_denied() -> BusinessError {
    error(
        FailureKind::Forbidden,
        "SCOPE_FORBIDDEN",
        "The grant exceeds its exact current parent scope.",
    )
}
pub fn validate_mapping_grant(
    facts: &DirectoryGrantFacts<'_>,
    now: i64,
) -> Result<(), BusinessError> {
    if facts.same_person {
        return Err(error(
            FailureKind::Forbidden,
            "SELF_ACCESS_CHANGE",
            "Another administrator must change your access.",
        ));
    }
    let parent = facts.parent.as_ref().filter(|p| p.current).ok_or_else(|| {
        error(
            FailureKind::Conflict,
            "PARENT_EXPIRED",
            "The authorizing assignment is no longer active.",
        )
    })?;
    if facts.requested_until <= now || facts.requested_until > parent.valid_until {
        return Err(BusinessError::invalid(
            "EXPIRY_INVALID",
            "Access must expire within the authorizing assignment.",
        ));
    }
    let ceiling = parent.grantable.ok_or_else(|| {
        error(
            FailureKind::Conflict,
            "PARENT_ROLE_UNAVAILABLE",
            "The current parent role is unavailable.",
        )
    })?;
    let member=facts.member.as_ref().filter(|m|m.directory).ok_or_else(||error(FailureKind::Forbidden,"DIRECTORY_IDENTITY_REVIEW_REQUIRED","Connect and review this Directory identity in this Company before assigning access."))?;
    if !member.active {
        return Err(error(
            FailureKind::Forbidden,
            "IDENTITY_INACTIVE",
            "Suspended or superseded identities cannot receive new access.",
        ));
    }
    let role = facts
        .role
        .as_ref()
        .filter(|r| r.revision > 0 && r.scopes.iter().any(|scope| scope == facts.requested_scope))
        .ok_or_else(|| {
            error(
                FailureKind::Conflict,
                "ROLE_UNAVAILABLE",
                "Choose an active role and supported scope.",
            )
        })?;
    if role
        .permissions
        .iter()
        .chain(role.grantable)
        .any(|permission| !ceiling.contains(permission))
    {
        return Err(error(
            FailureKind::Forbidden,
            "DELEGATION_EXCEEDED",
            "This role exceeds the current delegated grant permissions.",
        ));
    }
    if facts.platform {
        if facts.requested_scope != "platform"
            || facts.requested_node.is_some()
            || parent.scope != "platform"
            || parent.node.is_some()
        {
            return Err(scope_denied());
        }
        return Ok(());
    }
    if !parent.company_matches || facts.requested_scope == "platform" {
        return Err(scope_denied());
    }
    if facts.requested_scope == "holding" {
        // ADR055: unlike B0's ignored field, no node may be discarded or widened.
        if facts.requested_node.is_some() || parent.scope != "holding" || parent.node.is_some() {
            return Err(scope_denied());
        }
        return Ok(());
    }
    let node = facts.scope.as_ref().ok_or_else(|| {
        error(
            FailureKind::Conflict,
            "SCOPE_UNAVAILABLE",
            "Choose an active Organization scope.",
        )
    })?;
    let shape = match facts.requested_scope {
        "branch" => {
            facts.requested_node.is_some()
                && facts.requested_node == node.branch
                && node.department.is_none()
        }
        "department" => facts.requested_node.is_some() && facts.requested_node == node.department,
        _ => false,
    };
    let covered = match parent.scope {
        "holding" => parent.node.is_none(),
        "branch" => parent.node.is_some() && parent.node == node.branch,
        "department" => {
            facts.requested_scope == "department"
                && parent.node.is_some()
                && parent.node == node.department
        }
        _ => false,
    };
    if !shape || !covered {
        return Err(scope_denied());
    }
    Ok(())
}
