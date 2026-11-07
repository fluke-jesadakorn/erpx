//! Portable Identity Access assignment policy.
//!
//! An assignment is the durable link between an Account, a published role and
//! one Holding/Company/branch/department scope. This slice owns the bounded
//! assignment workspace read and the reviewed withdrawal of one assignment.
//! Nothing here reaches a database, HTTP request, clock or effect.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_REASON: usize = 1_000;
pub const MAX_WORKSPACE_ROWS: usize = 1_000;
pub const REVOKE_OPERATION: &str = "iam.assignment.revoke";

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
pub fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IAM_ACCESS_CHANGED",
        "Your current identity or selected role changed. Reload before retrying.",
    )
}
pub fn permission_required() -> BusinessError {
    forbidden(
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this operation.",
    )
}
pub fn assignment_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ASSIGNMENT_NOT_FOUND",
        "Assignment not found in this Company.",
    )
}
pub fn assignment_changed() -> BusinessError {
    conflict(
        "ASSIGNMENT_CHANGED",
        "This assignment changed. Reload before withdrawing access.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different assignment command.",
    )
}
pub fn self_access_change() -> BusinessError {
    forbidden(
        "SELF_ACCESS_CHANGE",
        "Another administrator must change your access.",
    )
}
pub fn last_administrator() -> BusinessError {
    conflict(
        "LAST_ADMINISTRATOR",
        "The last recovery administrator must remain available.",
    )
}
pub fn last_recovery_administrator() -> BusinessError {
    conflict(
        "LAST_RECOVERY_ADMINISTRATOR",
        "Designate another verified local recovery administrator first.",
    )
}
pub fn scope_forbidden() -> BusinessError {
    forbidden(
        "SCOPE_FORBIDDEN",
        "This assignment is outside your delegated scope.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_ASSIGNMENT_SOURCE_INVALID",
        "The assignment source is not available.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_ASSIGNMENT_READBACK_INVALID",
        "The assignment was not persisted as submitted.",
    )
}
pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "IAM_ASSIGNMENT_REQUEST_INVALID",
        "Review the assignment request and try again.",
    )
}

/// Current render and command facts captured by the composition root. This is
/// not a grant; the repository rechecks the same facts inside its unit.
#[derive(Clone, Debug)]
pub struct AssignmentActor {
    pub user_id: UserId,
    pub session_id: RecordId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub scope_kind: String,
    pub scope_node_id: Option<RecordId>,
    pub role: Option<String>,
    pub role_revision: Option<i64>,
    pub assignment_version: Option<i64>,
    pub identity_version: Option<i64>,
    pub authority_revision: Option<i64>,
    pub permissions: Vec<String>,
    pub person_user_ids: Vec<UserId>,
    pub emergency_until: Option<i64>,
    pub expires_at: i64,
}

impl AssignmentActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn require_context(&self, now: i64) -> Result<(), BusinessError> {
        if now >= self.expires_at {
            return Err(conflict(
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Choose a current role again.",
            ));
        }
        Ok(())
    }

    pub fn require_read(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("iam.read") {
            return Err(permission_required());
        }
        Ok(())
    }

    pub fn require_write(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("iam.manage") {
            return Err(permission_required());
        }
        Ok(())
    }

    pub fn same_person_user(&self, user_id: &str) -> bool {
        self.user_id.as_str() == user_id
            || self
                .person_user_ids
                .iter()
                .any(|alias| alias.as_str() == user_id)
    }

    /// Company-wide actors may withdraw any assignment in the Company. A
    /// branch/department actor may withdraw only an assignment with exactly the
    /// same selected scope node; descendant-workspace expansion remains a
    /// retained owner policy outside this bounded slice.
    pub fn can_withdraw(&self, row: &AssignmentRow) -> bool {
        if row.company_id != self.company_id.as_str() {
            return false;
        }
        if self.scope_kind == "holding" {
            return true;
        }
        self.scope_node_id
            .as_ref()
            .is_some_and(|node| row.scope_node_id.as_deref() == Some(node.as_str()))
    }

    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id.as_str(),
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "scopeKind": self.scope_kind,
            "scopeNodeId": self.scope_node_id.as_ref().map(RecordId::as_str),
            "role": self.role,
            "roleRevision": self.role_revision,
            "assignmentVersion": self.assignment_version,
            "identityVersion": self.identity_version,
            "revision": self.authority_revision,
            "permissions": self.permissions,
            "sessionId": self.session_id.as_str(),
            "emergencyUntil": self.emergency_until,
            "expiresAt": self.expires_at,
        })
        .to_string()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentRevocation {
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRevocation {
    pub expected_version: i64,
    pub reason: String,
}

fn trim_unicode(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(
            character,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

impl AssignmentRevocation {
    pub fn validate(self) -> Result<ValidatedRevocation, BusinessError> {
        let reason = trim_unicode(&self.reason).to_owned();
        if self.expected_version < 1
            || self.expected_version > 1_000_000_000
            || !(3..=MAX_REASON).contains(&reason.encode_utf16().count())
            || reason.chars().any(char::is_control)
        {
            return Err(request_invalid());
        }
        Ok(ValidatedRevocation {
            expected_version: self.expected_version,
            reason,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AssignmentRow {
    pub id: String,
    pub user_id: String,
    pub role: String,
    pub modules_json: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub valid_from: i64,
    pub valid_until: i64,
    pub revoked_at: Option<i64>,
    pub version: i64,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub parent_id: Option<String>,
    pub source: String,
    pub source_id: Option<String>,
    pub identity_status: Option<String>,
    pub login_policy: Option<String>,
    pub person_id: Option<String>,
    pub connection_id: Option<String>,
    pub identity_version: Option<i64>,
    pub group_name: Option<String>,
    pub mapping_connection_id: Option<String>,
    pub directory_enabled: Option<bool>,
    pub observed_at: Option<i64>,
    pub connection_status: Option<String>,
    pub active: bool,
}

impl AssignmentRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if RecordId::new(self.id.clone()).is_err()
            || RecordId::new(self.user_id.clone()).is_err()
            || self.role.trim().is_empty()
            || self.role.len() > 200
            || !matches!(
                self.scope_kind.as_str(),
                "holding" | "branch" | "department"
            )
            || self.version < 1
            || self.version > 1_000_000_000
            || self.valid_from < 0
            || self.valid_until <= self.valid_from
            || self
                .revoked_at
                .is_some_and(|revoked| revoked < self.valid_from)
            || !matches!(
                self.source.as_str(),
                "legacy"
                    | "manual"
                    | "directory"
                    | "emergency"
                    | "policy"
                    | "lifecycle"
                    | "provisioning"
            )
            || serde_json::from_str::<Value>(&self.modules_json).is_err()
            || (self
                .identity_status
                .as_deref()
                .is_some_and(|status| !matches!(status, "active" | "suspended" | "superseded")))
            || (self
                .login_policy
                .as_deref()
                .is_some_and(|policy| !matches!(policy, "local" | "directory" | "emergency")))
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "user_id": self.user_id,
            "role": self.role,
            "modules_json": self.modules_json,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "valid_from": self.valid_from,
            "valid_until": self.valid_until,
            "revoked_at": self.revoked_at,
            "version": self.version,
            "scope_kind": self.scope_kind,
            "scope_node_id": self.scope_node_id,
            "parent_id": self.parent_id,
            "source": self.source,
            "source_id": self.source_id,
            "identity_status": self.identity_status,
            "login_policy": self.login_policy,
            "person_id": self.person_id,
            "connection_id": self.connection_id,
            "identity_version": self.identity_version,
            "group_name": self.group_name,
            "mapping_connection_id": self.mapping_connection_id,
            "directory_enabled": self.directory_enabled,
            "observed_at": self.observed_at,
            "connection_status": self.connection_status,
            "active": self.active,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AssignmentRoleRow {
    pub id: String,
    pub name: String,
    pub company_id: String,
    pub revision: i64,
    pub built_in: bool,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub scopes: Vec<String>,
    pub status: String,
}

impl AssignmentRoleRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if self.id.trim().is_empty()
            || self.id.len() > 200
            || self.name.trim().is_empty()
            || self.name.len() > 300
            || self.revision < 1
            || !matches!(self.status.as_str(), "active" | "archived")
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "companyId": self.company_id,
            "revision": self.revision,
            "builtIn": self.built_in,
            "permissions": self.permissions,
            "grantablePermissions": self.grantable_permissions,
            "scopes": self.scopes,
            "status": self.status,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AssignmentGuardFacts {
    pub same_person: bool,
    pub recovery_member: bool,
    pub other_recovery_administrator: bool,
    pub other_administrator: bool,
}

#[derive(Clone, Debug)]
pub struct RevocationPlan {
    pub id: String,
    pub user_id: String,
    pub role: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub assignment_version: i64,
    pub expected_version: i64,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub reason: String,
    pub revoked_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl AssignmentReceipt {
    pub fn revoked(id: String, version: i64) -> Self {
        Self {
            id,
            status: "revoked".into(),
            version,
        }
    }
    pub fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "status": self.status,
            "version": self.version,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub authority_json: String,
}

pub fn revocation_identity(
    actor: &AssignmentActor,
    key: &str,
    id: &str,
    input: &ValidatedRevocation,
    operation: &str,
) -> String {
    json!({
        "actor": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "operation": operation,
        "key": key,
        "payload": {
            "id": id,
            "expectedVersion": input.expected_version,
            "reason": input.reason,
        }
    })
    .to_string()
}
