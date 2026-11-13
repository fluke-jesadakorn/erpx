//! Portable People relationship metadata policy.
//!
//! Identity Access owns external organizations, external teams and the external
//! relationship (subject) metadata of a Company. Nothing here reads a database,
//! HTTP request or clock; callers supply the current facts and time.
//!
//! The reviewed relationship command is a two-step preview/apply: the preview
//! binds the actor, assignment, session, reviewed change and a fingerprint of
//! the current snapshot; apply re-reads the snapshot inside the serializable
//! owner transaction, requires the same fingerprint and applies the change with
//! its optimistic version. Relationship commands never create an account, link
//! an identity, publish a role or grant access.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PREVIEW_LIFETIME_MS: i64 = 600_000;
pub const MAX_NAME: usize = 150;
pub const MAX_REASON: usize = 1_000;
pub const MAX_IDENTIFIER: usize = 180;
pub const MAX_EMAIL: usize = 254;
pub const MAX_SUBJECTS: usize = 10_000;
pub const MAX_ORGANIZATIONS: usize = 5_000;
pub const MAX_TEAMS: usize = 10_000;
pub const MAX_UNITS: usize = 4_000;

pub const RELATIONSHIP_READ_PERMISSION: &str = "people.relationships.read";
pub const RELATIONSHIP_MANAGE_PERMISSION: &str = "people.relationships.manage";
pub const RELATIONSHIP_APPLY_OPERATION: &str = "people.relationships.apply";

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
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
        "Review the relationship details and try again.",
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
pub fn team_has_members() -> BusinessError {
    conflict(
        "TEAM_HAS_MEMBERS",
        "Move this team's users before changing its external organization.",
    )
}
pub fn team_organization_mismatch() -> BusinessError {
    invalid(
        "TEAM_ORGANIZATION_MISMATCH",
        "Choose a team from this external organization.",
    )
}
pub fn sponsor_unavailable() -> BusinessError {
    invalid(
        "SPONSOR_UNAVAILABLE",
        "Choose an active employee in this Company.",
    )
}
pub fn self_access_change() -> BusinessError {
    forbidden(
        "SELF_ACCESS_CHANGE",
        "Another administrator must change your access.",
    )
}
pub fn protected_account() -> BusinessError {
    forbidden(
        "PROTECTED_ACCOUNT",
        "A protected recovery account cannot be changed through relationship management.",
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
        "The relationship command was not persisted as submitted.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_ACCESS_SOURCE_INVALID",
        "The relationship source is not available.",
    )
}

/// Current render and command facts captured by the composition root. This is
/// not a grant: the repository rechecks the same facts inside its unit.
#[derive(Clone, Debug)]
pub struct PeopleAccessActor {
    pub user_id: UserId,
    pub session_id: RecordId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub assignment_id: AssignmentId,
    pub scope_kind: String,
    pub permissions: Vec<String>,
    pub platform: bool,
    pub expires_at: i64,
}

impl PeopleAccessActor {
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

    /// Relationship commands always run inside a Company workspace.
    pub fn require_company(&self, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if self.platform {
            return Err(company_scope_required());
        }
        Ok(())
    }

    pub fn require(&self, permission: &str, now: i64) -> Result<(), BusinessError> {
        self.require_company(now)?;
        if !self.has(permission) {
            return Err(permission_required());
        }
        Ok(())
    }

    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id.as_str(),
            "companyId": self.company_id.as_str(),
            "scopeKind": self.scope_kind,
            "permissions": self.permissions,
            "sessionId": self.session_id.as_str(),
            "platform": self.platform,
            "expiresAt": self.expires_at,
        })
        .to_string()
    }
}

/// One reviewed relationship change. Privilege-bearing operations (`account`,
/// `role`, `grant`, `invite`, `revoke`) stay on the IAM owner command and are
/// rejected here without parsing.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "operation", rename_all = "lowercase", deny_unknown_fields)]
pub enum RelationshipChange {
    Organization {
        #[serde(default)]
        id: Option<String>,
        #[serde(rename = "expectedVersion", default)]
        expected_version: i64,
        name: String,
        kind: String,
        reason: String,
    },
    Team {
        #[serde(default)]
        id: Option<String>,
        #[serde(rename = "expectedVersion", default)]
        expected_version: i64,
        name: String,
        #[serde(rename = "organizationId")]
        organization_id: String,
        #[serde(rename = "departmentId")]
        department_id: Option<String>,
        reason: String,
    },
    Subject {
        #[serde(default)]
        id: Option<String>,
        #[serde(rename = "expectedVersion", default)]
        expected_version: i64,
        name: String,
        email: String,
        #[serde(rename = "organizationId")]
        organization_id: String,
        #[serde(rename = "teamId")]
        team_id: Option<String>,
        #[serde(rename = "sponsorPersonId")]
        sponsor_person_id: Option<String>,
        #[serde(rename = "expiresAt")]
        expires_at: Option<i64>,
        status: String,
        reason: String,
    },
}

impl RelationshipChange {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Organization { .. } => "organization",
            Self::Team { .. } => "team",
            Self::Subject { .. } => "subject",
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Organization { id, .. } | Self::Team { id, .. } | Self::Subject { id, .. } => {
                id.as_deref()
            }
        }
    }

    pub fn expected_version(&self) -> i64 {
        match self {
            Self::Organization {
                expected_version, ..
            }
            | Self::Team {
                expected_version, ..
            }
            | Self::Subject {
                expected_version, ..
            } => *expected_version,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Organization { name, .. }
            | Self::Team { name, .. }
            | Self::Subject { name, .. } => name,
        }
    }
}

fn trimmed(value: &str, minimum: usize, maximum: usize) -> bool {
    let count = value.chars().count();
    count >= minimum
        && count <= maximum
        && value.trim() == value
        && !value.chars().any(|character| character.is_control())
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

/// Normalize and bound one submitted relationship change before any database
/// read. The operation tag itself is decided by [`RelationshipChange`].
pub fn validate_change(change: RelationshipChange) -> Result<RelationshipChange, BusinessError> {
    let change = match change {
        RelationshipChange::Organization {
            id,
            expected_version,
            name,
            kind,
            reason,
        } => {
            if !trimmed(&name, 2, MAX_NAME)
                || !trimmed(&reason, 3, MAX_REASON)
                || !matches!(kind.as_str(), "organization" | "independent")
                || expected_version < 0
                || id.as_deref().is_some_and(|value| !identifier(value))
            {
                return Err(request_invalid());
            }
            RelationshipChange::Organization {
                id,
                expected_version,
                name,
                kind,
                reason,
            }
        }
        RelationshipChange::Team {
            id,
            expected_version,
            name,
            organization_id,
            department_id,
            reason,
        } => {
            if !trimmed(&name, 2, MAX_NAME)
                || !trimmed(&reason, 3, MAX_REASON)
                || !identifier(&organization_id)
                || department_id.as_deref().is_some_and(|value| !identifier(value))
                || expected_version < 0
                || id.as_deref().is_some_and(|value| !identifier(value))
            {
                return Err(request_invalid());
            }
            RelationshipChange::Team {
                id,
                expected_version,
                name,
                organization_id,
                department_id,
                reason,
            }
        }
        RelationshipChange::Subject {
            id,
            expected_version,
            name,
            email: address,
            organization_id,
            team_id,
            sponsor_person_id,
            expires_at,
            status,
            reason,
        } => {
            if !trimmed(&name, 2, MAX_NAME)
                || !trimmed(&reason, 3, MAX_REASON)
                || !email(&address)
                || !identifier(&organization_id)
                || team_id.as_deref().is_some_and(|value| !identifier(value))
                || sponsor_person_id
                    .as_deref()
                    .is_some_and(|value| !identifier(value))
                || !matches!(status.as_str(), "active" | "suspended")
                || expires_at.is_some_and(|value| value <= 0)
                || expected_version < 0
                || id.as_deref().is_some_and(|value| !identifier(value))
            {
                return Err(request_invalid());
            }
            RelationshipChange::Subject {
                id,
                expected_version,
                name,
                email: address,
                organization_id,
                team_id,
                sponsor_person_id,
                expires_at,
                status,
                reason,
            }
        }
    };
    Ok(change)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct OrganizationRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TeamRow {
    pub id: String,
    pub name: String,
    #[serde(rename = "organizationId")]
    pub organization_id: String,
    #[serde(rename = "departmentId")]
    pub department_id: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SubjectRow {
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
    #[serde(rename = "teamId")]
    pub team_id: Option<String>,
    #[serde(rename = "sponsorPersonId")]
    pub sponsor_person_id: Option<String>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<i64>,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UnitRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(rename = "branchId")]
    pub branch_id: Option<String>,
}

/// Current facts reviewed by a relationship preview. Both preview and apply
/// serialize exactly this value for the fingerprint.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RelationshipSnapshot {
    pub organizations: Vec<OrganizationRow>,
    pub teams: Vec<TeamRow>,
    pub subjects: Vec<SubjectRow>,
    pub units: Vec<UnitRow>,
    #[serde(rename = "authorizingUntil")]
    pub authorizing_until: i64,
}

impl RelationshipSnapshot {
    /// Canonical fingerprint input. Field order is fixed by the struct, so two
    /// reads of unchanged rows always produce the same digest.
    pub fn fingerprint_input(&self) -> Result<String, BusinessError> {
        serde_json::to_string(self).map_err(|_| readback_invalid())
    }
}

pub fn organization_in<'a>(
    snapshot: &'a RelationshipSnapshot,
    id: &str,
) -> Option<&'a OrganizationRow> {
    snapshot.organizations.iter().find(|row| row.id == id)
}

fn version_matches(actual: i64, expected: i64) -> bool {
    actual == expected
}

/// Pure relationship invariants against the reviewed snapshot. Sponsor, self
/// and protected-account facts need owner queries and are checked by the
/// application layer.
pub fn validate_snapshot(
    snapshot: &RelationshipSnapshot,
    change: &RelationshipChange,
) -> Result<(), BusinessError> {
    match change {
        RelationshipChange::Organization {
            id,
            expected_version,
            ..
        } => {
            let existing = id
                .as_deref()
                .map(|id| organization_in(snapshot, id).ok_or_else(access_object_unavailable))
                .transpose()?;
            if !version_matches(existing.map(|row| row.version).unwrap_or(0), *expected_version) {
                return Err(access_object_changed());
            }
        }
        RelationshipChange::Team {
            id,
            expected_version,
            organization_id,
            department_id,
            ..
        } => {
            let existing = id
                .as_deref()
                .map(|id| {
                    snapshot
                        .teams
                        .iter()
                        .find(|row| row.id == id)
                        .ok_or_else(access_object_unavailable)
                })
                .transpose()?;
            if !version_matches(existing.map(|row| row.version).unwrap_or(0), *expected_version) {
                return Err(access_object_changed());
            }
            if organization_in(snapshot, organization_id).is_none() {
                return Err(access_object_unavailable());
            }
            if department_id.as_deref().is_some_and(|id| {
                !snapshot
                    .units
                    .iter()
                    .any(|unit| unit.id == id && unit.kind == "department")
            }) {
                return Err(access_object_unavailable());
            }
            if let Some(existing) = existing
                && existing.organization_id != *organization_id
                && snapshot
                    .subjects
                    .iter()
                    .any(|subject| subject.team_id.as_deref() == Some(existing.id.as_str()))
            {
                return Err(team_has_members());
            }
        }
        RelationshipChange::Subject {
            id,
            expected_version,
            organization_id,
            team_id,
            ..
        } => {
            let existing = id
                .as_deref()
                .map(|id| {
                    snapshot
                        .subjects
                        .iter()
                        .find(|row| row.id == id)
                        .ok_or_else(access_object_unavailable)
                })
                .transpose()?;
            if !version_matches(existing.map(|row| row.version).unwrap_or(0), *expected_version) {
                return Err(access_object_changed());
            }
            if organization_in(snapshot, organization_id).is_none() {
                return Err(access_object_unavailable());
            }
            if let Some(team_id) = team_id
                && !snapshot
                    .teams
                    .iter()
                    .any(|team| team.id == *team_id && team.organization_id == *organization_id)
            {
                return Err(team_organization_mismatch());
            }
        }
    }
    Ok(())
}

/// A relationship preview is bound to the actor, session, authorizing
/// assignment, the reviewed change and the reviewed snapshot.
#[derive(Clone, Debug)]
pub struct RelationshipPreviewPlan {
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
pub struct RelationshipPreviewRow {
    pub change_json: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct RelationshipReceipt {
    pub request_hash: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub result_json: String,
    pub authority_json: String,
}

/// The applied relationship row, ready for the owner adapter.
#[derive(Clone, Debug)]
pub struct RelationshipApplyPlan {
    pub token: String,
    pub id: String,
    pub change: RelationshipChange,
    pub expected_version: i64,
    pub resulting_version: i64,
    /// Existing account link retained for an updated external subject.
    pub retained_user_id: Option<String>,
    pub retained_person_id: Option<String>,
    pub now: i64,
}

pub fn relationship_apply_identity(
    actor: &PeopleAccessActor,
    key: &str,
    token: &str,
) -> String {
    json!({
        "operation": RELATIONSHIP_APPLY_OPERATION,
        "actorId": actor.user_id.as_str(),
        "companyId": actor.company_id.as_str(),
        "assignmentId": actor.assignment_id.as_str(),
        "sessionId": actor.session_id.as_str(),
        "token": token,
        "key": key,
    })
    .to_string()
}

pub fn relationship_apply_result(
    id: &str,
    version: i64,
    change: &RelationshipChange,
) -> Result<Value, BusinessError> {
    let change = serde_json::to_value(change).map_err(|_| readback_invalid())?;
    Ok(json!({"id": id, "change": change, "version": version}))
}

/// Deterministic identifier for a newly created relationship row. Replays keep
/// the same identifier because the Idempotency-Key and reviewed token bind it.
pub fn relationship_id(change: &RelationshipChange, digest: &str) -> String {
    let prefix = match change {
        RelationshipChange::Organization { .. } => "iam-organization-",
        RelationshipChange::Team { .. } => "iam-team-",
        RelationshipChange::Subject { .. } => "iam-subject-",
    };
    format!("{prefix}{}", &digest[..40.min(digest.len())])
}

pub fn preview_summary(change: &RelationshipChange) -> String {
    format!("{}: {}", change.operation(), change.name())
}

/// Preview/apply token validation: the engine only accepts the UUID shape the
/// registered transports already require.
pub fn valid_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn organization_snapshot() -> RelationshipSnapshot {
        RelationshipSnapshot {
            organizations: vec![OrganizationRow {
                id: "org-1".into(),
                name: "Contractor".into(),
                kind: "organization".into(),
                version: 2,
            }],
            teams: vec![TeamRow {
                id: "team-1".into(),
                name: "Delivery".into(),
                organization_id: "org-1".into(),
                department_id: None,
                version: 1,
            }],
            subjects: vec![SubjectRow {
                id: "subject-1".into(),
                name: "Contractor A".into(),
                email: "a@example.invalid".into(),
                kind: "external".into(),
                user_id: None,
                person_id: None,
                organization_id: Some("org-1".into()),
                team_id: Some("team-1".into()),
                sponsor_person_id: None,
                expires_at: None,
                status: "active".into(),
                version: 3,
            }],
            units: vec![UnitRow {
                id: "dep-1".into(),
                name: "Operations".into(),
                kind: "department".into(),
                branch_id: Some("branch-1".into()),
            }],
            authorizing_until: 1_000,
        }
    }

    fn organization_change(expected_version: i64) -> RelationshipChange {
        RelationshipChange::Organization {
            id: Some("org-1".into()),
            expected_version,
            name: "Contractor".into(),
            kind: "organization".into(),
            reason: "Rename the relationship".into(),
        }
    }

    #[test]
    fn rejects_privilege_and_malformed_operations() {
        assert!(
            serde_json::from_value::<RelationshipChange>(serde_json::json!({
                "operation": "grant",
                "subjectId": "s",
                "roleId": "r",
                "scopeKind": "holding",
                "scopeNodeId": null,
                "validUntil": 1,
                "reason": "grant"
            }))
            .is_err()
        );
        let short_name = serde_json::from_value::<RelationshipChange>(serde_json::json!({
            "operation": "organization",
            "expectedVersion": 0,
            "name": "x",
            "kind": "organization",
            "reason": "short name"
        }))
        .unwrap();
        assert_eq!(
            validate_change(short_name).unwrap_err().code,
            "PEOPLE_ACCESS_REQUEST_INVALID"
        );
    }

    #[test]
    fn rejects_invalid_email_and_status() {
        let change = RelationshipChange::Subject {
            id: None,
            expected_version: 0,
            name: "Contractor B".into(),
            email: "not-an-email".into(),
            organization_id: "org-1".into(),
            team_id: None,
            sponsor_person_id: None,
            expires_at: None,
            status: "active".into(),
            reason: "Record a relationship".into(),
        };
        assert_eq!(
            validate_change(change).unwrap_err().code,
            "PEOPLE_ACCESS_REQUEST_INVALID"
        );
        let change = RelationshipChange::Subject {
            id: None,
            expected_version: 0,
            name: "Contractor B".into(),
            email: "b@example.invalid".into(),
            organization_id: "org-1".into(),
            team_id: None,
            sponsor_person_id: None,
            expires_at: None,
            status: "retired".into(),
            reason: "Record a relationship".into(),
        };
        assert_eq!(
            validate_change(change).unwrap_err().code,
            "PEOPLE_ACCESS_REQUEST_INVALID"
        );
    }

    #[test]
    fn cas_version_must_match_the_reviewed_row() {
        let snapshot = organization_snapshot();
        validate_snapshot(&snapshot, &organization_change(2)).unwrap();
        assert_eq!(
            validate_snapshot(&snapshot, &organization_change(1))
                .unwrap_err()
                .code,
            "ACCESS_OBJECT_CHANGED"
        );
        let created = RelationshipChange::Organization {
            id: None,
            expected_version: 0,
            name: "New contractor".into(),
            kind: "independent".into(),
            reason: "Record a relationship".into(),
        };
        validate_snapshot(&snapshot, &created).unwrap();
        let created = RelationshipChange::Organization {
            id: None,
            expected_version: 1,
            name: "New contractor".into(),
            kind: "independent".into(),
            reason: "Record a relationship".into(),
        };
        assert_eq!(
            validate_snapshot(&snapshot, &created).unwrap_err().code,
            "ACCESS_OBJECT_CHANGED"
        );
    }

    #[test]
    fn team_scope_and_membership_are_enforced() {
        let snapshot = organization_snapshot();
        let mismatch = RelationshipChange::Team {
            id: None,
            expected_version: 0,
            name: "Delivery".into(),
            organization_id: "missing".into(),
            department_id: None,
            reason: "Record a team".into(),
        };
        assert_eq!(
            validate_snapshot(&snapshot, &mismatch).unwrap_err().code,
            "ACCESS_OBJECT_UNAVAILABLE"
        );
        let unknown_department = RelationshipChange::Team {
            id: None,
            expected_version: 0,
            name: "Delivery".into(),
            organization_id: "org-1".into(),
            department_id: Some("missing".into()),
            reason: "Record a team".into(),
        };
        assert_eq!(
            validate_snapshot(&snapshot, &unknown_department)
                .unwrap_err()
                .code,
            "ACCESS_OBJECT_UNAVAILABLE"
        );
        let move_with_members = RelationshipChange::Team {
            id: Some("team-1".into()),
            expected_version: 1,
            name: "Delivery".into(),
            organization_id: "org-2".into(),
            department_id: None,
            reason: "Move the team".into(),
        };
        let mut snapshot = snapshot;
        snapshot.organizations.push(OrganizationRow {
            id: "org-2".into(),
            name: "Partner".into(),
            kind: "independent".into(),
            version: 1,
        });
        assert_eq!(
            validate_snapshot(&snapshot, &move_with_members)
                .unwrap_err()
                .code,
            "TEAM_HAS_MEMBERS"
        );
    }

    #[test]
    fn subject_team_must_belong_to_the_organization() {
        let snapshot = organization_snapshot();
        let change = RelationshipChange::Subject {
            id: Some("subject-1".into()),
            expected_version: 3,
            name: "Contractor A".into(),
            email: "a@example.invalid".into(),
            organization_id: "org-1".into(),
            team_id: Some("missing-team".into()),
            sponsor_person_id: None,
            expires_at: Some(2_000),
            status: "active".into(),
            reason: "Update the relationship".into(),
        };
        assert_eq!(
            validate_snapshot(&snapshot, &change).unwrap_err().code,
            "TEAM_ORGANIZATION_MISMATCH"
        );
    }

    #[test]
    fn fingerprint_is_stable_and_actor_limited() {
        let snapshot = organization_snapshot();
        assert_eq!(
            snapshot.fingerprint_input().unwrap(),
            snapshot.fingerprint_input().unwrap()
        );
        assert!(valid_token("3f2504e0-4f89-11d3-9a0c-0305e82c3301"));
        assert!(!valid_token("not-a-token"));
        assert!(!valid_token("3f2504e0-4f89-11d3-9a0c-0305e82c330"));
    }

    #[test]
    fn platform_actor_cannot_run_relationship_commands() {
        let actor = PeopleAccessActor {
            user_id: UserId::new("user-1").unwrap(),
            session_id: RecordId::new("session-1").unwrap(),
            holding_id: HoldingId::new("holding-1").unwrap(),
            company_id: CompanyId::new("company-1").unwrap(),
            assignment_id: AssignmentId::new("assignment-1").unwrap(),
            scope_kind: "holding".into(),
            permissions: vec![RELATIONSHIP_MANAGE_PERMISSION.into()],
            platform: true,
            expires_at: 1_000,
        };
        assert_eq!(
            actor
                .require(RELATIONSHIP_MANAGE_PERMISSION, 0)
                .unwrap_err()
                .code,
            "COMPANY_SCOPE_REQUIRED"
        );
    }
}
