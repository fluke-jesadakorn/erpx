//! Portable People grant, custom-role and invitation policy.
//!
//! Identity Access owns the reviewed People access commands that prepare an
//! account, publish a custom role, grant or invite access and withdraw an
//! assignment. Nothing here reads a database, HTTP request or clock; callers
//! supply the current facts and time.
//!
//! The command is a two-step preview/apply: the preview binds the actor,
//! assignment, session, reviewed change and a fingerprint of the current
//! snapshot; apply re-reads the snapshot inside the serializable owner
//! transaction, requires the same fingerprint and applies the change with its
//! optimistic version. Privilege operations are deliberately separate from the
//! relationship slice in [`crate::domain::people_access`].
use crate::domain::permission_catalog::PermissionCatalog;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PREVIEW_LIFETIME_MS: i64 = 600_000;
pub const MAX_NAME: usize = 150;
pub const MAX_ROLE_NAME: usize = 100;
pub const MAX_REASON: usize = 1_000;
pub const MAX_IDENTIFIER: usize = 180;
pub const MAX_EMAIL: usize = 254;
pub const MAX_PERMISSIONS: usize = 1_000;
pub const MAX_SUBJECTS: usize = 10_000;
pub const MAX_ROLES: usize = 2_000;
pub const MAX_ASSIGNMENTS: usize = 20_000;
pub const MAX_UNITS: usize = 4_000;
pub const ACCESS_APPLY_PERMISSION: &str = "iam.access.apply";
pub const ROLE_MANAGE_PERMISSION: &str = "iam.roles.manage";
pub const GRANT_APPLY_OPERATION: &str = "people.access.apply";
const PROTECTED_PERMISSIONS: [&str; 2] = ["company.owner", "iam.recovery"];

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}
fn unavailable(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        code,
        "This object is unavailable in your selected Company.",
    )
}

pub fn request_invalid() -> BusinessError {
    invalid(
        "PEOPLE_ACCESS_REQUEST_INVALID",
        "Review the access details and try again.",
    )
}
pub fn access_object_unavailable() -> BusinessError {
    unavailable("ACCESS_OBJECT_UNAVAILABLE")
}
pub fn access_object_changed() -> BusinessError {
    conflict(
        "ACCESS_OBJECT_CHANGED",
        "This record changed. Refresh and preview again.",
    )
}
pub fn preview_expired() -> BusinessError {
    conflict("PREVIEW_EXPIRED", "Preview this change again.")
}
pub fn preview_changed() -> BusinessError {
    conflict(
        "PREVIEW_CHANGED",
        "People or permissions changed. Refresh and preview again.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different identity command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_ACCESS_READBACK_INVALID",
        "The access command was not persisted as submitted.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_ACCESS_SOURCE_INVALID",
        "The access source is not available.",
    )
}
pub fn company_scope_required() -> BusinessError {
    forbidden(
        "COMPANY_SCOPE_REQUIRED",
        "Select a Company workspace for People access.",
    )
}
pub fn permission_required() -> BusinessError {
    forbidden(
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this operation.",
    )
}
pub fn self_access_change() -> BusinessError {
    forbidden(
        "SELF_ACCESS_CHANGE",
        "Another administrator must change your access.",
    )
}
pub fn protected_role() -> BusinessError {
    forbidden(
        "PROTECTED_ROLE",
        "Create a custom role instead of replacing a built-in role.",
    )
}
pub fn delegation_exceeded() -> BusinessError {
    forbidden(
        "DELEGATION_EXCEEDED",
        "The role exceeds your delegated permissions.",
    )
}
pub fn permission_unavailable() -> BusinessError {
    invalid(
        "PERMISSION_UNAVAILABLE",
        "Choose a published permission.",
    )
}
pub fn subject_inactive() -> BusinessError {
    invalid(
        "SUBJECT_INACTIVE",
        "Activate this relationship before enabling access.",
    )
}
pub fn invite_required() -> BusinessError {
    invalid("INVITE_REQUIRED", "Invite this user first.")
}
pub fn invitation_unavailable() -> BusinessError {
    invalid(
        "INVITATION_UNAVAILABLE",
        "Choose an unlinked user with an email address.",
    )
}
pub fn scope_invalid() -> BusinessError {
    invalid(
        "SCOPE_INVALID",
        "Holding access has no local scope target.",
    )
}
pub fn expiry_invalid() -> BusinessError {
    invalid(
        "EXPIRY_INVALID",
        "Access must expire within the external relationship and your authorizing assignment.",
    )
}
pub fn external_access_not_ready() -> BusinessError {
    invalid(
        "EXTERNAL_ACCESS_NOT_READY",
        "Choose an external organization, an internal sponsor and a future expiry before enabling access.",
    )
}
pub fn role_unavailable() -> BusinessError {
    conflict(
        "ROLE_UNAVAILABLE",
        "Choose an active role and supported scope.",
    )
}
pub fn role_changed() -> BusinessError {
    conflict(
        "ROLE_CHANGED",
        "The role changed. Review its current permissions before granting access.",
    )
}
pub fn identity_inactive() -> BusinessError {
    forbidden(
        "IDENTITY_INACTIVE",
        "Suspended or superseded identities cannot receive new access.",
    )
}
pub fn directory_identity_review_required() -> BusinessError {
    forbidden(
        "DIRECTORY_IDENTITY_REVIEW_REQUIRED",
        "Connect and review this directory identity in this company before assigning access.",
    )
}
pub fn scope_forbidden() -> BusinessError {
    forbidden(
        "SCOPE_FORBIDDEN",
        "The grant exceeds its parent scope.",
    )
}
pub fn scope_unavailable() -> BusinessError {
    conflict("SCOPE_UNAVAILABLE", "Choose an active organization scope.")
}
pub fn directory_account_link_required() -> BusinessError {
    conflict(
        "DIRECTORY_ACCOUNT_LINK_REQUIRED",
        "Link this account through its verified directory identity.",
    )
}
pub fn account_already_linked() -> BusinessError {
    conflict(
        "ACCOUNT_ALREADY_LINKED",
        "This account is already in this Company. Manage its existing user record.",
    )
}

/// Current render and command facts captured by the composition root. This is
/// not a grant: the repository rechecks the same facts inside its unit.
#[derive(Clone, Debug)]
pub struct PeopleGrantActor {
    pub user_id: String,
    pub session_id: String,
    pub company_id: String,
    pub assignment_id: String,
    pub scope_kind: String,
    pub permissions: Vec<String>,
    pub platform: bool,
    pub expires_at: i64,
}

impl PeopleGrantActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id,
            "companyId": self.company_id,
            "scopeKind": self.scope_kind,
            "permissions": self.permissions,
            "sessionId": self.session_id,
            "platform": self.platform,
            "expiresAt": self.expires_at,
        })
        .to_string()
    }
}

/// One reviewed People access change. Relationship metadata (`organization`,
/// `team`, `subject`) stays on [`crate::domain::people_access`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "operation", rename_all = "lowercase", deny_unknown_fields)]
pub enum PeopleGrantChange {
    Account {
        name: String,
        email: String,
        reason: String,
    },
    Role {
        #[serde(default)]
        id: Option<String>,
        #[serde(rename = "expectedVersion", default)]
        expected_version: i64,
        name: String,
        permissions: Vec<String>,
        scopes: Vec<String>,
        reason: String,
    },
    Grant {
        #[serde(rename = "subjectId")]
        subject_id: String,
        #[serde(rename = "roleId")]
        role_id: String,
        #[serde(rename = "scopeKind")]
        scope_kind: String,
        #[serde(rename = "scopeNodeId")]
        scope_node_id: Option<String>,
        #[serde(rename = "validUntil")]
        valid_until: i64,
        reason: String,
    },
    Invite {
        #[serde(rename = "subjectId")]
        subject_id: String,
        #[serde(rename = "roleId")]
        role_id: String,
        #[serde(rename = "scopeKind")]
        scope_kind: String,
        #[serde(rename = "scopeNodeId")]
        scope_node_id: Option<String>,
        #[serde(rename = "validUntil")]
        valid_until: i64,
        reason: String,
    },
    Revoke {
        #[serde(rename = "assignmentId")]
        assignment_id: String,
        #[serde(rename = "expectedVersion")]
        expected_version: i64,
        reason: String,
    },
}

impl PeopleGrantChange {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Account { .. } => "account",
            Self::Role { .. } => "role",
            Self::Grant { .. } => "grant",
            Self::Invite { .. } => "invite",
            Self::Revoke { .. } => "revoke",
        }
    }

    /// The permission the actor must currently hold for this operation.
    pub fn required_permission(&self) -> &'static str {
        match self {
            Self::Role { .. } => ROLE_MANAGE_PERMISSION,
            _ => ACCESS_APPLY_PERMISSION,
        }
    }

    pub fn subject_id(&self) -> Option<&str> {
        match self {
            Self::Grant { subject_id, .. } | Self::Invite { subject_id, .. } => Some(subject_id),
            _ => None,
        }
    }

    pub fn role_id(&self) -> Option<&str> {
        match self {
            Self::Grant { role_id, .. } | Self::Invite { role_id, .. } => Some(role_id),
            _ => None,
        }
    }

    pub fn caps_id(&self) -> Option<&str> {
        match self {
            Self::Role { id, .. } => id.as_deref(),
            Self::Revoke { assignment_id, .. } => Some(assignment_id),
            _ => None,
        }
    }
}

fn trimmed(value: &str, minimum: usize, maximum: usize) -> bool {
    let count = value.chars().count();
    count >= minimum
        && count <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn identifier(value: &str) -> bool {
    trimmed(value, 1, MAX_IDENTIFIER)
}

fn email(value: &str) -> bool {
    if value.len() > MAX_EMAIL || !trimmed(value, 3, MAX_EMAIL) {
        return false;
    }
    let mut parts = value.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !value.chars().any(char::is_whitespace)
}

fn scope_kind(value: &str) -> bool {
    matches!(value, "holding" | "branch" | "department")
}

/// Normalize and bound one submitted access change before any database read.
pub fn validate_change(change: PeopleGrantChange) -> Result<PeopleGrantChange, BusinessError> {
    let valid = match &change {
        PeopleGrantChange::Account { name, email: address, reason } => {
            trimmed(name, 2, MAX_NAME) && email(address) && trimmed(reason, 3, MAX_REASON)
        }
        PeopleGrantChange::Role {
            id,
            expected_version,
            name,
            permissions,
            scopes,
            reason,
        } => {
            trimmed(name, 2, MAX_ROLE_NAME)
                && trimmed(reason, 3, MAX_REASON)
                && !permissions.is_empty()
                && permissions.len() <= MAX_PERMISSIONS
                && permissions.iter().all(|value| identifier(value))
                && !scopes.is_empty()
                && scopes.len() <= 3
                && scopes.iter().all(|value| scope_kind(value))
                && *expected_version >= 0
                && id.as_deref().is_none_or(identifier)
        }
        PeopleGrantChange::Grant {
            subject_id,
            role_id,
            scope_kind: kind,
            scope_node_id,
            valid_until,
            reason,
        }
        | PeopleGrantChange::Invite {
            subject_id,
            role_id,
            scope_kind: kind,
            scope_node_id,
            valid_until,
            reason,
        } => {
            identifier(subject_id)
                && identifier(role_id)
                && scope_kind(kind)
                && scope_node_id.as_deref().is_none_or(identifier)
                && *valid_until > 0
                && trimmed(reason, 3, MAX_REASON)
                && (kind != "holding" || scope_node_id.is_none())
        }
        PeopleGrantChange::Revoke {
            assignment_id,
            expected_version,
            reason,
        } => {
            identifier(assignment_id)
                && *expected_version > 0
                && trimmed(reason, 3, MAX_REASON)
        }
    };
    if !valid {
        return Err(request_invalid());
    }
    Ok(change)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct GrantSubjectRow {
    pub id: String,
    pub name: String,
    pub email: String,
    pub kind: String,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
    #[serde(rename = "personId")]
    pub person_id: Option<String>,
    #[serde(rename = "organizationId")]
    pub organization_id: Option<String>,
    #[serde(rename = "sponsorPersonId")]
    pub sponsor_person_id: Option<String>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<i64>,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct GrantRoleRow {
    pub id: String,
    pub name: String,
    pub revision: i64,
    #[serde(rename = "builtIn")]
    pub built_in: bool,
    pub permissions: Vec<String>,
    #[serde(rename = "grantablePermissions")]
    pub grantable_permissions: Vec<String>,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct GrantAssignmentRow {
    pub id: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "roleId")]
    pub role_id: String,
    #[serde(rename = "scopeKind")]
    pub scope_kind: String,
    #[serde(rename = "scopeNodeId")]
    pub scope_node_id: Option<String>,
    #[serde(rename = "validUntil")]
    pub valid_until: i64,
    pub version: i64,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct GrantUnitRow {
    pub id: String,
    pub kind: String,
    #[serde(rename = "branchId")]
    pub branch_id: Option<String>,
}

/// Current facts reviewed by an access preview. Both preview and apply
/// serialize exactly this value for the fingerprint.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct GrantSnapshot {
    pub subjects: Vec<GrantSubjectRow>,
    pub roles: Vec<GrantRoleRow>,
    pub assignments: Vec<GrantAssignmentRow>,
    pub units: Vec<GrantUnitRow>,
    #[serde(rename = "authorizingUntil")]
    pub authorizing_until: i64,
    #[serde(rename = "authorizingScopeKind")]
    pub authorizing_scope_kind: String,
    #[serde(rename = "authorizingScopeNodeId")]
    pub authorizing_scope_node_id: Option<String>,
    #[serde(rename = "grantablePermissions")]
    pub grantable_permissions: Vec<String>,
}

impl GrantSnapshot {
    pub fn fingerprint_input(&self) -> Result<String, BusinessError> {
        serde_json::to_string(self).map_err(|_| readback_invalid())
    }

    pub fn subject(&self, id: &str) -> Option<&GrantSubjectRow> {
        self.subjects.iter().find(|row| row.id == id)
    }

    pub fn role(&self, id: &str) -> Option<&GrantRoleRow> {
        self.roles.iter().find(|row| row.id == id)
    }

    pub fn assignment(&self, id: &str) -> Option<&GrantAssignmentRow> {
        self.assignments.iter().find(|row| row.id == id)
    }

    /// A role is delegable when the actor currently holds every permission and
    /// grantable permission it carries, and it is not an owner/recovery role.
    pub fn role_delegable(&self, role: &GrantRoleRow) -> bool {
        role.permissions
            .iter()
            .chain(role.grantable_permissions.iter())
            .all(|permission| {
                !PROTECTED_PERMISSIONS.contains(&permission.as_str())
                    && self
                        .grantable_permissions
                        .iter()
                        .any(|held| held == permission)
            })
    }

    /// The catalog permissions an actor may put on a custom role.
    pub fn role_permission_available(&self, catalog: &PermissionCatalog, id: &str) -> bool {
        if PROTECTED_PERMISSIONS.contains(&id) {
            return false;
        }
        catalog
            .definitions()
            .iter()
            .any(|definition| {
                definition.id == id
                    && definition.availability
                        == crate::domain::permission_catalog::PermissionAvailability::Live
                    && definition.namespace
                        == crate::domain::permission_catalog::PermissionNamespace::Customer
            })
            && self.grantable_permissions.iter().any(|held| held == id)
    }

    /// An administrator may not publish a role that is assigned to them.
    /// Aliases of the same Person are checked by the owner adapter, which can
    /// resolve `iam_same_person` inside the command unit.
    pub fn role_self_assigned(&self, actor: &PeopleGrantActor, role_id: &str) -> bool {
        self.assignments
            .iter()
            .any(|row| row.role_id == role_id && row.user_id == actor.user_id)
    }

    pub fn same_person(&self, actor: &PeopleGrantActor, user_id: &str) -> bool {
        user_id == actor.user_id
    }
}

/// External relationships must be complete and unexpired before access is
/// enabled on them.
fn external_ready(subject: &GrantSubjectRow, now: i64) -> Result<(), BusinessError> {
    if subject.kind != "external" {
        return Ok(());
    }
    if subject.organization_id.is_none()
        || subject.sponsor_person_id.is_none()
        || subject.expires_at.is_none_or(|expires| expires <= now)
        || subject.status == "suspended"
    {
        return Err(external_access_not_ready());
    }
    Ok(())
}

/// Deadline and role invariants that need only the reviewed snapshot.
pub fn validate_snapshot(
    snapshot: &GrantSnapshot,
    actor: &PeopleGrantActor,
    catalog: &PermissionCatalog,
    change: &PeopleGrantChange,
    now: i64,
) -> Result<(), BusinessError> {
    match change {
        PeopleGrantChange::Account { .. } => {}
        PeopleGrantChange::Role {
            id,
            expected_version,
            permissions,
            scopes,
            ..
        } => {
            if permissions.iter().any(|permission| {
                !snapshot.role_permission_available(catalog, permission)
            }) {
                return Err(permission_unavailable());
            }
            if let Some(id) = id {
                let role = snapshot.role(id).ok_or_else(protected_role)?;
                if role.built_in || !id.starts_with("custom_") {
                    return Err(protected_role());
                }
                if role.revision != *expected_version {
                    return Err(role_changed());
                }
                if snapshot.role_self_assigned(actor, id) {
                    return Err(self_access_change());
                }
            } else if *expected_version != 0 {
                return Err(role_changed());
            }
            let _ = scopes;
        }
        PeopleGrantChange::Grant {
            subject_id,
            role_id,
            scope_kind,
            scope_node_id,
            valid_until,
            ..
        }
        | PeopleGrantChange::Invite {
            subject_id,
            role_id,
            scope_kind,
            scope_node_id,
            valid_until,
            ..
        } => {
            let subject = snapshot
                .subject(subject_id)
                .ok_or_else(access_object_unavailable)?;
            external_ready(subject, now)?;
            if subject.status == "suspended"
                || (subject.kind == "internal" && subject.status != "active")
            {
                return Err(subject_inactive());
            }
            if matches!(change, PeopleGrantChange::Grant { .. }) {
                if subject.user_id.is_none() {
                    return Err(invite_required());
                }
            } else if subject.user_id.is_some() || subject.email.is_empty() {
                return Err(invitation_unavailable());
            }
            if *scope_kind == "holding" {
                if scope_node_id.is_some() {
                    return Err(scope_invalid());
                }
                if snapshot.authorizing_scope_kind != "holding"
                    || actor.scope_kind != "holding"
                {
                    return Err(scope_forbidden());
                }
            } else {
                let node = snapshot.units.iter().find(|unit| {
                    unit.kind == *scope_kind && Some(unit.id.as_str()) == scope_node_id.as_deref()
                });
                let Some(node) = node else {
                    return Err(scope_unavailable());
                };
                let branch = if node.kind == "branch" {
                    Some(node.id.as_str())
                } else {
                    node.branch_id.as_deref()
                };
                let parent_node = snapshot.authorizing_scope_node_id.as_deref();
                if snapshot.authorizing_scope_kind == "department"
                    && parent_node != Some(node.id.as_str())
                {
                    return Err(scope_forbidden());
                }
                if snapshot.authorizing_scope_kind == "branch" && parent_node != branch {
                    return Err(scope_forbidden());
                }
                if actor.scope_kind == "department" && parent_node != Some(node.id.as_str()) {
                    return Err(scope_forbidden());
                }
                if actor.scope_kind == "branch" && parent_node != branch {
                    return Err(scope_forbidden());
                }
            }
            let role = snapshot
                .role(role_id)
                .ok_or_else(access_object_unavailable)?;
            if !role.scopes.iter().any(|scope| scope == scope_kind) {
                return Err(role_unavailable());
            }
            if *valid_until <= now
                || *valid_until > snapshot.authorizing_until
                || subject
                    .expires_at
                    .is_some_and(|expires| *valid_until > expires)
            {
                return Err(expiry_invalid());
            }
            if !snapshot.role_delegable(role) {
                return Err(delegation_exceeded());
            }
        }
        PeopleGrantChange::Revoke {
            assignment_id,
            expected_version,
            ..
        } => {
            let assignment = snapshot
                .assignment(assignment_id)
                .ok_or_else(access_object_unavailable)?;
            if snapshot.role(&assignment.role_id).is_none() {
                return Err(delegation_exceeded());
            }
            if assignment.version != *expected_version {
                return Err(conflict(
                    "ASSIGNMENT_CHANGED",
                    "This assignment changed. Preview again.",
                ));
            }
            if assignment.user_id == actor.user_id
                || snapshot.same_person(actor, &assignment.user_id)
            {
                return Err(self_access_change());
            }
        }
    }
    Ok(())
}

/// Reviewer facts for a grant. The native slice has no second reviewer
/// deadline; the authorizing assignment and the target relationship bind it.
#[derive(Clone, Debug)]
pub struct GrantPreviewPlan {
    pub company_id: String,
    pub actor_id: String,
    pub assignment_id: String,
    pub session_id: String,
    pub change_json: String,
    pub fingerprint: String,
    pub expires_at: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct GrantPreviewRow {
    pub change_json: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct GrantReceipt {
    pub request_hash: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub result_json: String,
    pub authority_json: String,
}

/// The applied access row, ready for the owner adapter.
#[derive(Clone, Debug)]
pub struct GrantApplyPlan {
    pub token: String,
    pub id: String,
    /// The account, role or subject the change is recorded against.
    pub subject_id: Option<String>,
    pub subject: Option<GrantSubjectRow>,
    pub assignment_id: Option<String>,
    pub invitation_id: Option<String>,
    pub role_revision: Option<i64>,
    pub valid_until: Option<i64>,
    pub scope_kind: Option<String>,
    pub scope_node_id: Option<String>,
    pub expected_version: i64,
    pub change: PeopleGrantChange,
    pub now: i64,
}

pub fn grant_apply_identity(actor: &PeopleGrantActor, key: &str, token: &str) -> String {
    json!({
        "operation": GRANT_APPLY_OPERATION,
        "actorId": actor.user_id,
        "companyId": actor.company_id,
        "assignmentId": actor.assignment_id,
        "sessionId": actor.session_id,
        "token": token,
        "key": key,
    })
    .to_string()
}

/// Deterministic identifiers. Replays keep them because the Idempotency-Key and
/// reviewed token bind them.
pub fn grant_id(change: &PeopleGrantChange, digest: &str) -> String {
    let prefix = match change {
        PeopleGrantChange::Account { .. } => "iam-account-",
        PeopleGrantChange::Role { .. } => "custom_",
        PeopleGrantChange::Grant { .. } | PeopleGrantChange::Invite { .. } => "iam-grant-",
        PeopleGrantChange::Revoke { .. } => "iam-revoke-",
    };
    format!("{prefix}{}", &digest[..40.min(digest.len())])
}

pub fn invitation_id(_change: &PeopleGrantChange, digest: &str) -> String {
    format!("iam-invite-{}", &digest[..40.min(digest.len())])
}

pub fn preview_summary(
    snapshot: &GrantSnapshot,
    change: &PeopleGrantChange,
) -> String {
    match change {
        PeopleGrantChange::Grant { subject_id, role_id, scope_kind, valid_until, .. }
        | PeopleGrantChange::Invite { subject_id, role_id, scope_kind, valid_until, .. } => {
            let subject = snapshot.subject(subject_id).map(|row| row.name.as_str()).unwrap_or("");
            let role = snapshot.role(role_id).map(|row| row.name.as_str()).unwrap_or("");
            format!("{subject} → {role} · {scope_kind} · {}", iso_millis(*valid_until))
        }
        PeopleGrantChange::Revoke { .. } => "Withdraw the selected access assignment".into(),
        PeopleGrantChange::Account { name, .. } => format!("account: {name}"),
        PeopleGrantChange::Role { name, .. } => format!("role: {name}"),
    }
}

/// Bounded, deterministic UTC rendering for the reviewed summary.
fn iso_millis(value: i64) -> String {
    let seconds = value.div_euclid(1000);
    let millis = value.rem_euclid(1000);
    let (days, day_seconds) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // 1970-01-01 based civil conversion (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (hour, minute, second) = (
        day_seconds / 3_600,
        (day_seconds % 3_600) / 60,
        day_seconds % 60,
    );
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Preview/apply token validation: the owner only accepts the UUID shape the
/// registered transports already require.
pub fn valid_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// The applied change as returned to the caller. Invitation results also carry
/// the staged delivery identifiers the Account consumer needs.
pub fn grant_apply_result(plan: &GrantApplyPlan, change: &Value) -> Value {
    let mut result = json!({"id": plan.id, "change": change});
    if let Some(object) = result.as_object_mut() {
        if let Some(value) = &plan.subject_id {
            object.insert("subjectId".into(), json!(value));
        }
        if let Some(value) = &plan.assignment_id {
            object.insert("assignmentId".into(), json!(value));
        }
        if let Some(value) = &plan.invitation_id {
            object.insert("invitationId".into(), json!(value));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::permission_catalog::permission_catalog;

    fn actor() -> PeopleGrantActor {
        PeopleGrantActor {
            user_id: "actor".into(),
            session_id: "session-actor".into(),
            company_id: "company".into(),
            assignment_id: "assignment-actor".into(),
            scope_kind: "holding".into(),
            permissions: vec![
                ACCESS_APPLY_PERMISSION.into(),
                ROLE_MANAGE_PERMISSION.into(),
            ],
            platform: false,
            expires_at: 1_000_000,
        }
    }

    fn snapshot() -> GrantSnapshot {
        GrantSnapshot {
            subjects: vec![GrantSubjectRow {
                id: "subject-1".into(),
                name: "Visitor".into(),
                email: "visitor@example.invalid".into(),
                kind: "external".into(),
                user_id: None,
                person_id: None,
                organization_id: Some("org-1".into()),
                sponsor_person_id: Some("person-sponsor".into()),
                expires_at: Some(900_000),
                status: "active".into(),
                version: 1,
            }],
            roles: vec![GrantRoleRow {
                id: "custom_read".into(),
                name: "Read only".into(),
                revision: 2,
                built_in: false,
                permissions: vec!["module.collaboration.read".into()],
                grantable_permissions: vec![],
                scopes: vec!["department".into(), "holding".into()],
            }],
            assignments: vec![GrantAssignmentRow {
                id: "assignment-target".into(),
                user_id: "other".into(),
                role_id: "custom_read".into(),
                scope_kind: "holding".into(),
                scope_node_id: None,
                valid_until: 800_000,
                version: 3,
                active: true,
            }],
            units: vec![GrantUnitRow {
                id: "department-1".into(),
                kind: "department".into(),
                branch_id: Some("branch-1".into()),
            }],
            authorizing_until: 950_000,
            authorizing_scope_kind: "holding".into(),
            authorizing_scope_node_id: None,
            grantable_permissions: vec!["module.collaboration.read".into()],
        }
    }

    fn grant(valid_until: i64) -> PeopleGrantChange {
        PeopleGrantChange::Invite {
            subject_id: "subject-1".into(),
            role_id: "custom_read".into(),
            scope_kind: "department".into(),
            scope_node_id: Some("department-1".into()),
            valid_until,
            reason: "Invite the collaborator".into(),
        }
    }

    #[test]
    fn rejects_unknown_operations_and_malformed_values() {
        assert!(
            serde_json::from_value::<PeopleGrantChange>(serde_json::json!({
                "operation": "subject",
                "name": "Relationship",
                "reason": "not a grant"
            }))
            .is_err()
        );
        assert_eq!(
            validate_change(PeopleGrantChange::Account {
                name: "x".into(),
                email: "not-an-email".into(),
                reason: "short".into(),
            })
            .unwrap_err()
            .code,
            "PEOPLE_ACCESS_REQUEST_INVALID"
        );
        assert_eq!(
            validate_change(PeopleGrantChange::Revoke {
                assignment_id: "assignment-1".into(),
                expected_version: 0,
                reason: "withdraw access".into(),
            })
            .unwrap_err()
            .code,
            "PEOPLE_ACCESS_REQUEST_INVALID"
        );
    }

    #[test]
    fn invite_requires_an_unlinked_subject_and_a_deadline_inside_the_authority() {
        let catalog = permission_catalog().unwrap();
        let change = validate_change(grant(900_000)).unwrap();
        validate_snapshot(&snapshot(), &actor(), catalog, &change, 1_000).unwrap();
        let change = validate_change(grant(960_000)).unwrap();
        assert_eq!(
            validate_snapshot(&snapshot(), &actor(), catalog, &change, 1_000)
                .unwrap_err()
                .code,
            "EXPIRY_INVALID"
        );
        let mut data = snapshot();
        data.subjects[0].user_id = Some("linked".into());
        let change = validate_change(grant(900_000)).unwrap();
        assert_eq!(
            validate_snapshot(&data, &actor(), catalog, &change, 1_000)
                .unwrap_err()
                .code,
            "INVITATION_UNAVAILABLE"
        );
    }

    #[test]
    fn grant_requires_a_linked_subject_and_enforces_scope() {
        let catalog = permission_catalog().unwrap();
        let change = validate_change(PeopleGrantChange::Grant {
            subject_id: "subject-1".into(),
            role_id: "custom_read".into(),
            scope_kind: "holding".into(),
            scope_node_id: None,
            valid_until: 900_000,
            reason: "Assign access".into(),
        })
        .unwrap();
        assert_eq!(
            validate_snapshot(&snapshot(), &actor(), catalog, &change, 1_000)
                .unwrap_err()
                .code,
            "INVITE_REQUIRED"
        );
        let mut data = snapshot();
        data.subjects[0].user_id = Some("linked".into());
        validate_snapshot(&data, &actor(), catalog, &change, 1_000).unwrap();
        let change = validate_change(PeopleGrantChange::Grant {
            subject_id: "subject-1".into(),
            role_id: "custom_read".into(),
            scope_kind: "department".into(),
            scope_node_id: Some("missing".into()),
            valid_until: 900_000,
            reason: "Assign access".into(),
        })
        .unwrap();
        assert_eq!(
            validate_snapshot(&data, &actor(), catalog, &change, 1_000)
                .unwrap_err()
                .code,
            "SCOPE_UNAVAILABLE"
        );
    }

    #[test]
    fn undelegable_and_protected_roles_are_rejected() {
        let catalog = permission_catalog().unwrap();
        let change = validate_change(PeopleGrantChange::Role {
            id: Some("it_admin".into()),
            expected_version: 1,
            name: "Escalated role".into(),
            permissions: vec!["module.collaboration.read".into()],
            scopes: vec!["holding".into()],
            reason: "Replace the built-in role".into(),
        })
        .unwrap();
        assert_eq!(
            validate_snapshot(&snapshot(), &actor(), catalog, &change, 1_000)
                .unwrap_err()
                .code,
            "PROTECTED_ROLE"
        );
        let change = validate_change(PeopleGrantChange::Role {
            id: None,
            expected_version: 0,
            name: "Ownership".into(),
            permissions: vec!["company.owner".into()],
            scopes: vec!["holding".into()],
            reason: "Publish ownership".into(),
        })
        .unwrap();
        assert_eq!(
            validate_snapshot(&snapshot(), &actor(), catalog, &change, 1_000)
                .unwrap_err()
                .code,
            "PERMISSION_UNAVAILABLE"
        );
    }

    #[test]
    fn revoke_cas_and_self_change_are_enforced() {
        let catalog = permission_catalog().unwrap();
        let change = validate_change(PeopleGrantChange::Revoke {
            assignment_id: "assignment-target".into(),
            expected_version: 2,
            reason: "Withdraw access".into(),
        })
        .unwrap();
        assert_eq!(
            validate_snapshot(&snapshot(), &actor(), catalog, &change, 1_000)
                .unwrap_err()
                .code,
            "ASSIGNMENT_CHANGED"
        );
        let change = validate_change(PeopleGrantChange::Revoke {
            assignment_id: "assignment-target".into(),
            expected_version: 3,
            reason: "Withdraw access".into(),
        })
        .unwrap();
        validate_snapshot(&snapshot(), &actor(), catalog, &change, 1_000).unwrap();
        let mut data = snapshot();
        data.assignments[0].user_id = "actor".into();
        assert_eq!(
            validate_snapshot(&data, &actor(), catalog, &change, 1_000)
                .unwrap_err()
                .code,
            "SELF_ACCESS_CHANGE"
        );
    }

    #[test]
    fn tokens_and_fingerprints_are_stable() {
        let data = snapshot();
        assert_eq!(
            data.fingerprint_input().unwrap(),
            data.fingerprint_input().unwrap()
        );
        assert!(valid_token("3f2504e0-4f89-11d3-9a0c-0305e82c3301"));
        assert!(!valid_token("not-a-token"));
        assert_eq!(iso_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_millis(1_700_000_000_000), "2023-11-14T22:13:20.000Z");
    }
}
